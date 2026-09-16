//! Verify authored recovery fixtures before interpreting playback failures.
use super::{
    recording_service,
    wire::{Pid, PsiSection, TransportPacket},
};
use std::collections::BTreeSet;

const ORIGINAL: &[u8] = include_bytes!("../../../tests/fixtures/recording.ts");

struct Half {
    tables: BTreeSet<(u16, u8, u16)>, // table PID, version, PMT/PCR PID
    video_frames: usize,
    audio_packets: usize,
    pcr: Vec<u64>,
    discontinuities: BTreeSet<u16>,
}

fn inspect(bytes: &[u8], pmt: u16, video: u16, audio: u16) -> Half {
    assert_eq!(recording_service(bytes), Some(1));
    let mut half = Half {
        tables: BTreeSet::new(),
        video_frames: 0,
        audio_packets: 0,
        pcr: Vec::new(),
        discontinuities: BTreeSet::new(),
    };
    assert!(bytes.as_chunks::<188>().1.is_empty());
    for raw in bytes.as_chunks::<188>().0 {
        let packet = TransportPacket::parse(raw).expect("valid generated packet");
        if packet.discontinuity {
            half.discontinuities.insert(packet.pid.0);
        }
        if packet.pid == Pid(video) {
            if let Some(pcr) = packet.pcr {
                half.pcr.push(pcr);
            }
            if packet.start && packet.payload.starts_with(&[0, 0, 1, 0xe0]) {
                half.video_frames += 1;
            }
        }
        if packet.pid == Pid(audio) && !packet.payload.is_empty() {
            half.audio_packets += 1;
        }
        if [Pid::PAT, Pid(pmt)].contains(&packet.pid) && packet.start {
            let start = 1 + usize::from(packet.payload[0]);
            let data = &packet.payload[start..];
            let length = 3 + ((usize::from(data[1] & 15) << 8) | usize::from(data[2]));
            let section = PsiSection::parse(&data[..length]).expect("current CRC-checked PAT/PMT");
            let target = if packet.pid == Pid::PAT {
                let programs = section.pat_programs().unwrap();
                assert_eq!(programs, vec![(1, Pid(pmt))]);
                pmt
            } else {
                let map = section.program_map().unwrap();
                assert_eq!(map.service, 1);
                assert_eq!(map.pcr_pid, Pid(video));
                video
            };
            half.tables.insert((packet.pid.0, section.version, target));
        }
    }
    assert_eq!(half.video_frames, 75);
    assert!(half.audio_packets > 100);
    assert!(half.pcr.len() > 20);
    assert!(half.pcr.windows(2).all(|pair| pair[0] < pair[1]));
    half
}

#[test]
fn recovery_fixtures_change_tables_and_timestamps_without_changing_service() {
    let original = inspect(ORIGINAL, 0x20, 0x41, 0x42);
    assert_eq!(
        original.tables,
        BTreeSet::from([(0, 0, 0x20), (0x20, 0, 0x41)])
    );
    let changed = include_bytes!("../../../tests/fixtures/recording-pid-change.ts");
    assert_eq!(&changed[..ORIGINAL.len()], ORIGINAL);
    let changed = inspect(&changed[ORIGINAL.len()..], 0x120, 0x141, 0x142);
    assert_eq!(
        changed.tables,
        BTreeSet::from([(0, 1, 0x120), (0x120, 1, 0x141)])
    );
    assert_eq!(changed.pcr[0] - original.pcr[0], 288_000);
    assert!(changed.pcr[0] > *original.pcr.last().unwrap());
    assert_eq!(
        changed.discontinuities,
        BTreeSet::from([0, 0x120, 0x141, 0x142])
    );

    let reset = include_bytes!("../../../tests/fixtures/recording-clock-reset.ts");
    assert_eq!(&reset[..ORIGINAL.len()], ORIGINAL);
    let reset = inspect(&reset[ORIGINAL.len()..], 0x20, 0x41, 0x42);
    assert_eq!(reset.tables, original.tables);
    assert_eq!(
        reset.pcr, original.pcr,
        "the second half really rewinds the clock"
    );
    assert_eq!(reset.discontinuities, BTreeSet::from([0, 0x20, 0x41, 0x42]));
}
