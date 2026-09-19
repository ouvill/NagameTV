//! EIT wire data through the position-scoped catalog; no devices or server.
use super::*;
use crate::audio::{self, Kind, Role};

const SECOND_NS: u64 = 1_000_000_000;
const SERVICE: u16 = 1;
const TRANSPORT: u16 = 1;
const SOURCE: u64 = 42;
const EIT_HEADER_BYTES: usize = 14;
const EVENT_HEADER_BYTES: usize = 12;
const CRC_BYTES: usize = 4;
const SECTION_PREFIX_BYTES: usize = 3;
const DUAL_MAIN: &[u8] = &[
    0xf2, 0x02, 0x10, 0x0f, 0xff, 0xc7, b'j', b'p', b'n', b'e', b'n', b'g',
];
const STEREO_SUB: &[u8] = &[0xf2, 0x03, 0x10, 0x0f, 0xff, 0x07, b'e', b'n', b'g'];

fn eit(event: u16, components: &[&[u8]]) -> Vec<u8> {
    let table = syntax::EIT_ACTUAL_PF_TABLE_ID;
    let mut section = vec![
        table,
        0xf0,
        0,
        0,
        SERVICE as u8,
        0xc1,
        0,
        1,
        0,
        TRANSPORT as u8,
        0,
        4,
        0,
        table,
    ];
    section.extend_from_slice(&event.to_be_bytes());
    // Undefined start/duration: EIT present still identifies the playing event.
    section.extend_from_slice(&[0xff; 8]);
    section.extend_from_slice(&[0x80, 0]);
    for body in components {
        section.extend_from_slice(&[
            gstreamer_mpegts::ffi::GST_MTS_DESC_ISDB_AUDIO_COMPONENT as u8,
            body.len() as u8,
        ]);
        section.extend_from_slice(body);
    }
    let descriptors = section.len() - EIT_HEADER_BYTES - EVENT_HEADER_BYTES;
    let length_offset = EIT_HEADER_BYTES + EVENT_HEADER_BYTES - 2;
    section[length_offset] |= (descriptors >> 8) as u8;
    section[length_offset + 1] = descriptors as u8;
    let length = section.len() + CRC_BYTES - SECTION_PREFIX_BYTES;
    section[1] |= (length >> 8) as u8;
    section[2] = length as u8;
    let crc = crate::transport::wire::crc32_mpeg(&section);
    section.extend_from_slice(&crc.to_be_bytes());
    section
}

fn observe(
    catalog: &mut catalog::Catalog,
    cursor: &mut catalog::ScanCursor,
    collector: &mut Collector,
    seconds: u64,
    section: &[u8],
) {
    collector.section(EIT_PIDS[0], section);
    catalog.observe(
        cursor,
        catalog::ScanPoint {
            accuracy: catalog::Accuracy::Indexed,
            epoch: 0,
            offset: seconds * TS_PACKET_SIZE as u64,
            position: seconds * SECOND_NS,
            end: (seconds + 1) * SECOND_NS,
            observation: Some(&Observation {
                pcr: seconds * SECOND_NS,
                information: collector.information.clone(),
            }),
        },
    );
}

#[test]
fn audio_follows_playhead_and_in_event_changes_without_clock_or_epg()
-> Result<(), Box<dyn std::error::Error>> {
    let mut collector = Collector::new(SERVICE);
    collector.transport(TRANSPORT);
    let mut catalog = catalog::Catalog::default();
    let mut cursor = catalog::ScanCursor::default();
    observe(
        &mut catalog,
        &mut cursor,
        &mut collector,
        0,
        &eit(1, &[DUAL_MAIN]),
    );
    observe(
        &mut catalog,
        &mut cursor,
        &mut collector,
        10,
        &eit(1, &[STEREO_SUB]),
    );
    observe(
        &mut catalog,
        &mut cursor,
        &mut collector,
        20,
        &eit(2, &[DUAL_MAIN]),
    );
    observe(&mut catalog, &mut cursor, &mut collector, 30, &eit(2, &[]));
    let metadata = |seconds| {
        let view = catalog.view(seconds * SECOND_NS);
        assert!(view.clock.is_none());
        view.program
            .map(|program| audio::Metadata::from_ts(SOURCE, program))
            .ok_or("program missing")
    };
    // Seek backwards from the receive edge, then forwards, including a format
    // change within the same event. Later text corrections must not move audio.
    for (seconds, expected) in [(25, Role::Both), (5, Role::Both), (15, Role::Sub)] {
        let metadata = metadata(seconds)?;
        let program = metadata.program();
        assert_eq!(program.key.start, None);
        assert_eq!(
            audio::matching(program.descriptors, Some(0x10))
                .ok_or("audio missing")?
                .role(),
            expected
        );
    }
    assert_eq!(metadata(5)?.program().key, metadata(15)?.program().key);
    assert_ne!(metadata(5)?.program().key, metadata(25)?.program().key);
    assert!(metadata(30)?.program().descriptors.is_empty());
    assert!(catalog.view(31 * SECOND_NS).program.is_none());
    let other = audio::Metadata::from_ts(
        SOURCE + 1,
        catalog.view(5 * SECOND_NS).program.ok_or("program")?,
    );
    assert_ne!(metadata(5)?.program().key, other.program().key);
    assert!(
        !catalog
            .view(5 * SECOND_NS)
            .presentation(5 * SECOND_NS)
            .0
            .contains("audios")
    );
    Ok(())
}

#[test]
fn malformed_or_ambiguous_eit_audio_never_assigns_a_guessed_role()
-> Result<(), Box<dyn std::error::Error>> {
    let mut collector = Collector::new(SERVICE);
    collector.transport(TRANSPORT);
    collector.section(EIT_PIDS[0], &eit(1, &[DUAL_MAIN]));
    // Structurally incomplete audio invalidates the update, retaining valid SI.
    collector.section(EIT_PIDS[0], &eit(2, &[&DUAL_MAIN[..DUAL_MAIN.len() - 1]]));
    let event = collector.information.current.as_ref().ok_or("event")?;
    assert_eq!(event.event_id, 1);
    assert_eq!(
        audio::matching(&event.audios, Some(0x10))
            .ok_or("audio")?
            .kind(),
        Kind::DualMono
    );
    collector.section(EIT_PIDS[0], &eit(2, &[DUAL_MAIN, STEREO_SUB]));
    let event = collector.information.current.as_ref().ok_or("event")?;
    assert_eq!(event.event_id, 2);
    assert!(audio::matching(&event.audios, Some(0x10)).is_none());
    assert!(audio::matching(&event.audios, Some(0x11)).is_none());
    assert!(audio::matching(&event.audios, None).is_none());
    Ok(())
}
