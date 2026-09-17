//! Check the generated stream addition independently of the Python authoring code.
use super::{
    Sections,
    wire::{ComponentTag, Pid, PsiSection, TS_PACKET_SIZE, TransportPacket},
};
use crate::features::subtitles::transport::TransportParser;
use std::collections::BTreeMap;

const ORIGINAL: &[u8] = include_bytes!("../../../tests/fixtures/recording-seek.ts");
const CHANGED: &[u8] = include_bytes!("../../../tests/fixtures/recording-caption-change.ts");
const SERVICE: u16 = 1;
const PMT: Pid = Pid(0x20);
const VIDEO: Pid = Pid(0x41);
const AUDIO: Pid = Pid(0x42);
const SECOND_AUDIO: Pid = Pid(0x43);
const CAPTION: Pid = Pid(0x130);
const SUPERIMPOSE: Pid = Pid(0x138);
const TICKS_PER_MS: u64 = 90;
const CAPTION_START_MS: u64 = 15_000;
const AUDIO_START_MS: u64 = 15_200;
const PCR_INTERVAL_MS: u64 = 80;
const FIRST_CAPTION_SECOND: u64 = 15;
const DURATION_SECONDS: u64 = 60;
const FULLWIDTH_DIGITS: [char; 10] = ['０', '１', '２', '３', '４', '５', '６', '７', '８', '９'];

#[derive(Debug, PartialEq, Eq)]
enum Stage {
    SuperimposeOnly,
    Caption,
    SecondAudio,
}

fn media_packets(bytes: &[u8]) -> Vec<&[u8; TS_PACKET_SIZE]> {
    assert!(bytes.as_chunks::<TS_PACKET_SIZE>().1.is_empty());
    bytes
        .as_chunks::<TS_PACKET_SIZE>()
        .0
        .iter()
        .filter(|raw| {
            let packet = TransportPacket::parse(raw.as_slice()).unwrap();
            ![PMT, SECOND_AUDIO, CAPTION, SUPERIMPOSE].contains(&packet.pid)
        })
        .collect()
}

// The production parser validates the whole PMT but intentionally only exposes
// caption streams. Inspect the ES loop separately to assert the audio addition.
fn elementary_pids(section: &[u8]) -> Vec<Pid> {
    const PROGRAM_INFO_LENGTH_OFFSET: usize = 10;
    const PROGRAM_DESCRIPTOR_OFFSET: usize = 12;
    const ES_HEADER_BYTES: usize = 5;
    const ES_PID_OFFSET: usize = 1;
    const ES_INFO_LENGTH_OFFSET: usize = 3;
    const CRC_BYTES: usize = 4;
    const LENGTH_MASK: u16 = 0x0fff;
    const PID_MASK: u16 = 0x1fff;
    let word = |at| u16::from_be_bytes([section[at], section[at + 1]]);
    let mut cursor =
        PROGRAM_DESCRIPTOR_OFFSET + usize::from(word(PROGRAM_INFO_LENGTH_OFFSET) & LENGTH_MASK);
    let mut pids = Vec::new();
    while cursor < section.len() - CRC_BYTES {
        pids.push(Pid(word(cursor + ES_PID_OFFSET) & PID_MASK));
        cursor += ES_HEADER_BYTES + usize::from(word(cursor + ES_INFO_LENGTH_OFFSET) & LENGTH_MASK);
    }
    assert_eq!(cursor, section.len() - CRC_BYTES);
    pids
}

#[test]
fn caption_transition_preserves_media_and_updates_current_crc_checked_tables() {
    assert_eq!(
        media_packets(CHANGED),
        media_packets(ORIGINAL),
        "video/audio payload, PCR/PTS/DTS, PAT and SI must remain byte-identical"
    );
    let mut sections = Sections::default();
    let mut counters = BTreeMap::<Pid, u8>::new();
    let mut first_pcr = None;
    let mut elapsed_ms = 0;
    let mut transitions = Vec::new();
    let mut previous_version = None;
    let mut stage = Stage::SuperimposeOnly;
    let mut secondary_packets = 0;
    let mut caption_packets = 0;
    for raw in CHANGED.as_chunks::<TS_PACKET_SIZE>().0 {
        let packet = TransportPacket::parse(raw).expect("valid transport packet");
        if packet.pid == VIDEO
            && let Some(pcr) = packet.pcr
        {
            elapsed_ms = (pcr - *first_pcr.get_or_insert(pcr)) / TICKS_PER_MS;
        }
        if !packet.payload.is_empty()
            && let Some(previous) = counters.insert(packet.pid, packet.continuity_counter)
        {
            assert_eq!(
                packet.continuity_counter,
                (previous + 1) % 16,
                "payload continuity for {:?}",
                packet.pid
            );
        }
        if packet.pid == CAPTION {
            assert_ne!(stage, Stage::SuperimposeOnly);
            caption_packets += 1;
        }
        if packet.pid == SECOND_AUDIO {
            assert_eq!(stage, Stage::SecondAudio);
            if secondary_packets == 0 {
                assert!(
                    packet.start,
                    "new audio must begin with a complete PES header"
                );
                assert!(crate::transport::pes::PesHeader::parse(packet.payload).is_some());
            }
            secondary_packets += 1;
        }
        if packet.pid != PMT {
            continue;
        }
        for bytes in sections.push(packet.start, packet.payload) {
            let section = PsiSection::parse(&bytes).expect("current PMT and valid CRC");
            let map = section.program_map().unwrap();
            assert_eq!(map.service, SERVICE);
            assert_eq!(map.pcr_pid, VIDEO);
            stage = match section.version {
                0 => Stage::SuperimposeOnly,
                1 => Stage::Caption,
                2 => Stage::SecondAudio,
                version => panic!("unexpected PMT version {version}"),
            };
            let (expected_pids, caption_count) = match stage {
                Stage::SuperimposeOnly => (vec![VIDEO, AUDIO, SUPERIMPOSE], 0),
                Stage::Caption => (vec![VIDEO, AUDIO, CAPTION, SUPERIMPOSE], 1),
                Stage::SecondAudio => (vec![VIDEO, AUDIO, SECOND_AUDIO, CAPTION, SUPERIMPOSE], 1),
            };
            assert_eq!(elementary_pids(&bytes), expected_pids);
            assert_eq!(map.captions.len(), caption_count);
            for caption in &map.captions {
                assert_eq!(caption.pid, CAPTION);
                assert_eq!(caption.component_tag, ComponentTag::DEFAULT_CAPTION);
            }
            if previous_version != Some(section.version) {
                transitions.push((section.version, elapsed_ms));
                previous_version = Some(section.version);
            }
        }
    }
    assert_eq!(transitions.len(), 3);
    assert_eq!(transitions[0], (0, 0));
    for ((version, actual), (expected_version, target)) in transitions[1..]
        .iter()
        .zip([(1, CAPTION_START_MS), (2, AUDIO_START_MS)])
    {
        assert_eq!(*version, expected_version);
        assert!(
            actual.abs_diff(target) <= PCR_INTERVAL_MS,
            "{actual} vs {target}"
        );
    }
    assert!(caption_packets > 0);
    assert!(secondary_packets > 0);
    assert_eq!(stage, Stage::SecondAudio);
}

#[test]
fn caption_transition_decodes_authored_statements_before_and_after_audio_addition() {
    let normalized = tsreadex::Filter::new(SERVICE)
        .unwrap()
        .push(CHANGED)
        .unwrap();
    for bytes in [CHANGED, normalized.as_slice()] {
        let mut parser = TransportParser::new(true);
        parser.select_service(SERVICE);
        let mut texts = Vec::new();
        let mut previous_pts = None;
        for raw in bytes.as_chunks::<TS_PACKET_SIZE>().0 {
            for cue in parser.push(raw) {
                if cue.text.is_empty() {
                    continue; // Normalizer's initial empty caption management.
                }
                assert_eq!((cue.plane_width, cue.plane_height), (960, 540));
                let pts = cue.pts_ms.expect("authored caption has a PTS");
                assert!(previous_pts.is_none_or(|previous| previous < pts));
                previous_pts = Some(pts);
                texts.push(cue.text);
            }
        }
        // Normal-size ARIB alphanumeric characters decode to fullwidth Unicode.
        let expected: Vec<_> = (FIRST_CAPTION_SECOND..DURATION_SECONDS)
            .map(|second| {
                format!(
                    "ＣＡＰＴＩＯＮ　{}{}ｓ",
                    FULLWIDTH_DIGITS[second as usize / 10],
                    FULLWIDTH_DIGITS[second as usize % 10],
                )
            })
            .collect();
        assert_eq!(
            texts, expected,
            "both raw and normalized captions must survive the PMT update"
        );
    }
}
