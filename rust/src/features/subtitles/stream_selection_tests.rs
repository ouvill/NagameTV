//! End-to-end alternative-caption regression through TS, PES and libaribcaption.
use super::transport::TransportParser;
#[path = "../../../crates/libaribcaption/tests/fixtures/sample.rs"]
mod fixture;

#[test]
fn synthetic_tables_preserve_caption_alternatives() {
    use super::{
        selection::select_caption,
        wire::{Pid, PsiSection, TransmissionLayer},
    };
    // Authored from the wire format, with arbitrary test service/PID values.
    // No received broadcast bytes are embedded in these fixtures.
    let alternatives = pmt(&[
        TestStream::low(),
        TestStream::high(),
        TestStream {
            pid: 0x220,
            tag: 0x38,
            hierarchy: None,
        }, // Superimpose is separate.
    ]);
    let alternatives = PsiSection::parse(&alternatives)
        .unwrap()
        .program_map()
        .unwrap();
    assert_eq!(alternatives.service, TEST_SERVICE);
    assert_eq!(alternatives.captions.len(), 2);
    assert_eq!(
        select_caption(&alternatives.captions),
        Some(Pid(PRIMARY_PID))
    );
    let high = alternatives
        .captions
        .iter()
        .find(|stream| stream.pid == Pid(PRIMARY_PID))
        .unwrap();
    let low = alternatives
        .captions
        .iter()
        .find(|stream| stream.pid == Pid(SECONDARY_PID))
        .unwrap();
    assert_eq!(high.hierarchy.unwrap().layer, TransmissionLayer::High);
    assert_eq!(high.hierarchy.unwrap().reference, Some(low.pid));
    assert_eq!(low.hierarchy.unwrap().layer, TransmissionLayer::Low);
    assert_eq!(low.hierarchy.unwrap().reference, Some(high.pid));

    let ordinary = pmt(&[TestStream {
        pid: PRIMARY_PID,
        tag: 0x30,
        hierarchy: None,
    }]);
    let ordinary = PsiSection::parse(&ordinary).unwrap().program_map().unwrap();
    assert_eq!(ordinary.service, TEST_SERVICE);
    assert_eq!(ordinary.captions.len(), 1);
    assert_eq!(select_caption(&ordinary.captions), Some(Pid(PRIMARY_PID)));
    assert_eq!(ordinary.captions[0].hierarchy, None);
}

fn section(mut bytes: Vec<u8>) -> Vec<u8> {
    let length = bytes.len() - 3 + 4;
    bytes[1] = 0xb0 | (length >> 8) as u8;
    bytes[2] = length as u8;
    let mut crc = 0xffff_ffff_u32;
    for &byte in &bytes {
        crc ^= u32::from(byte) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 {
                (crc << 1) ^ 0x04c1_1db7
            } else {
                crc << 1
            };
        }
    }
    bytes.extend_from_slice(&crc.to_be_bytes());
    bytes
}

fn packet(pid: u16, payload: &[u8]) -> Vec<u8> {
    assert!(payload.len() <= 182);
    let mut packet = vec![0xff; 188];
    packet[..6].copy_from_slice(&[
        0x47,
        0x40 | (pid >> 8) as u8,
        pid as u8,
        0x30,
        (183 - payload.len()) as u8,
        0,
    ]);
    packet[188 - payload.len()..].copy_from_slice(payload);
    packet
}

// Synthetic transport identifiers; unrelated to any broadcaster's assignment.
const TEST_SERVICE: u16 = 7;
const PMT_PID: u16 = 0x100;
const PRIMARY_PID: u16 = 0x210;
const SECONDARY_PID: u16 = 0x211;

struct TestStream {
    pid: u16,
    tag: u8,
    hierarchy: Option<(bool, u16)>, // high layer flag and reference PID
}
impl TestStream {
    fn high() -> Self {
        Self {
            pid: PRIMARY_PID,
            tag: 0x30,
            hierarchy: Some((true, SECONDARY_PID)),
        }
    }
    fn low() -> Self {
        Self {
            pid: SECONDARY_PID,
            tag: 0x31,
            hierarchy: Some((false, PRIMARY_PID)),
        }
    }
}

fn pmt(streams: &[TestStream]) -> Vec<u8> {
    let mut pmt = vec![2, 0, 0]; // table_id and section_length (filled by section()).
    pmt.extend(TEST_SERVICE.to_be_bytes());
    pmt.extend([0xc1, 0, 0]); // Current version 0, one section.
    pmt.extend((0xe000 | PRIMARY_PID).to_be_bytes()); // PCR_PID.
    pmt.extend([0xf0, 0]); // Empty program descriptor loop.
    for stream in streams {
        let mut descriptors = vec![
            0x52, 1, stream.tag, // Stream identifier descriptor.
            0xfd, 2, 0, 8, // ARIB caption data component descriptor.
        ];
        if let Some((high, reference)) = stream.hierarchy {
            descriptors.extend([0xc0, 3, 0xfe | u8::from(high)]);
            descriptors.extend((0xe000 | reference).to_be_bytes());
        }
        pmt.push(0x06); // Private PES stream.
        pmt.extend((0xe000 | stream.pid).to_be_bytes());
        pmt.extend((0xf000 | descriptors.len() as u16).to_be_bytes());
        pmt.extend(descriptors);
    }
    section(pmt)
}

fn tables(low_only: bool) -> Vec<u8> {
    let mut pat = vec![0, 0, 0, 0, 1, 0xc1, 0, 0];
    pat.extend(TEST_SERVICE.to_be_bytes());
    pat.extend((0xe000 | PMT_PID).to_be_bytes());
    // List the low layer first to verify selection does not depend on ES order.
    let streams = if low_only {
        vec![TestStream::low()]
    } else {
        vec![TestStream::low(), TestStream::high()]
    };
    let mut data = packet(0, &[&[0][..], &section(pat)].concat());
    data.extend(packet(PMT_PID, &[&[0][..], &pmt(&streams)].concat()));
    data
}

fn caption(low_quality: bool, pts: u64) -> Vec<u8> {
    let mut data = fixture::SAMPLE.to_vec();
    if low_quality {
        // SD counterpart: SWF9 (720x480), 24x24 glyphs instead of HD36x36.
        let swf = data.windows(4).position(|w| w == b"\x9b7 S").unwrap();
        data[swf + 1] = b'9';
        let ssm = data.windows(8).position(|w| w == b"\x9b36;36 W").unwrap();
        data[ssm + 1..ssm + 6].copy_from_slice(b"24;24");
        let mut crc = 0_u16;
        for &byte in &data[3..data.len() - 2] {
            crc ^= u16::from(byte) << 8;
            for _ in 0..8 {
                crc = if crc & 0x8000 != 0 {
                    (crc << 1) ^ 0x1021
                } else {
                    crc << 1
                };
            }
        }
        let end = data.len();
        data[end - 2..].copy_from_slice(&crc.to_be_bytes());
    }
    let mut pes = vec![0, 0, 1, 0xbd];
    pes.extend_from_slice(&((data.len() + 8) as u16).to_be_bytes());
    pes.extend_from_slice(&[
        0x80,
        0x80,
        5,
        0x21 | ((pts >> 29) as u8 & 14),
        (pts >> 22) as u8,
        ((pts >> 14) as u8 & 0xfe) | 1,
        (pts >> 7) as u8,
        ((pts << 1) as u8 & 0xfe) | 1,
    ]);
    pes.extend(data);
    packet(
        if low_quality {
            SECONDARY_PID
        } else {
            PRIMARY_PID
        },
        &pes,
    )
}

#[test]
fn low_quality_duplicate_cannot_replace_the_hd_caption() {
    let mut parser = TransportParser::new(true);
    parser.select_service(TEST_SERVICE);
    let mut data = tables(false);
    data.extend(caption(false, 90_000));
    data.extend(caption(true, 99_000)); // Same text 100ms later, 0.75x displayed size.
    let cues = parser.push(&data);
    assert_eq!(
        cues.len(),
        1,
        "alternative captions must not overwrite the primary"
    );
    assert_eq!(cues[0].text, "♬〜");
    assert_eq!(cues[0].pts_ms, Some(1000));
    assert_eq!((cues[0].plane_width, cues[0].plane_height), (960, 540));
    assert!(cues[0].cells.iter().all(|cell| cell.glyph_height == 36));
}

#[test]
fn sd_only_caption_remains_usable() {
    let mut parser = TransportParser::new(true);
    parser.select_service(TEST_SERVICE);
    let mut data = tables(true);
    data.extend(caption(true, 90_000));
    let cues = parser.push(&data);
    assert_eq!(cues.len(), 1);
    assert_eq!((cues[0].plane_width, cues[0].plane_height), (720, 480));
    assert!(cues[0].cells.iter().all(|cell| cell.glyph_height == 24));
}

#[test]
fn duplicate_with_discontinuity_and_updated_pcr_does_not_emit_twice() {
    let mut parser = TransportParser::new(true);
    parser.select_service(TEST_SERVICE);
    parser.push(&tables(false));
    let mut first = caption(false, 90_000);
    assert!(first[4] >= 7, "fixture needs room for PCR");
    first[5] = 0x90; // discontinuity_indicator and PCR_flag
    first[6..12].copy_from_slice(&[0, 0, 0, 0, 0x7e, 0]);
    assert_eq!(parser.push(&first).len(), 1);
    let mut duplicate = first.clone();
    duplicate[11] = 1; // A duplicate is allowed to refresh PCR.
    assert!(parser.push(&duplicate).is_empty());
}

#[test]
fn changed_caption_pid_clears_already_scheduled_screens() {
    use super::{SubtitleClock, SubtitleUpdate, ingest::Ingest};
    use gstreamer as gst;
    gst::init().unwrap();
    let clock = SubtitleClock::default();
    let mut parser = TransportParser::new(true);
    parser.select_service(TEST_SERVICE);
    let ingest = Ingest::new(parser, clock.clone());
    let mut initial = tables(false);
    initial.extend(caption(false, 90_000));
    let initial = gst::Buffer::from_slice(initial);
    ingest.consume([initial.as_ref()]);
    assert_eq!(clock.pending_count(), Some(1));
    let mut changed = tables(true);
    changed[188 + 3] |= 1; // Next PMT payload continuity counter.
    let changed = gst::Buffer::from_slice(changed);
    ingest.consume([changed.as_ref()]);
    assert_eq!(clock.pending_count(), Some(0));
    assert!(matches!(clock.poll(None).unwrap(), SubtitleUpdate::Clear));
}
