use super::*;
use crate::transport::programs::catalog::Status;

const VIDEO_PID: Pid = Pid(0x41);
const AUDIO_PID: Pid = Pid(0x42);
const SEPARATE_CLOCK_PID: Pid = Pid(0x1ff);
const FOREIGN_PID: Pid = Pid(0x100);
const PCR_PID_OFFSET: usize = 8;
const CRC_BYTES: usize = 4;
const PID_RESERVED_BITS: u16 = 0xe000;
const SECTION_PREFIX_BYTES: usize = 3;
const SECTION_RESERVED_BITS: u16 = 0xb000;
const EMPTY_DESCRIPTOR_LOOP: [u8; 2] = [0xf0, 0];
const VIDEO_STREAM_ID: u8 = 0xe0;
const AUDIO_STREAM_ID: u8 = 0xc0;
const PES_MARKER: u8 = 0x80;
const PTS_ONLY: u8 = 0x80;
const TIMESTAMP_BYTES: u8 = 5;
const PES_TIMESTAMP_PREFIX: u8 = 0x20;
const TIMESTAMP_MARKER: u8 = 1;
const TIMESTAMP_PART_MASK: u64 = 0x7f;

fn acquired() -> (Index, u64) {
    let mut index = Index::new(1, true);
    let fixture = include_bytes!("../../../../../tests/fixtures/recording-seek.ts");
    for (number, packet) in fixture.as_chunks::<TS_PACKET_SIZE>().0.iter().enumerate() {
        let offset = (number * TS_PACKET_SIZE) as u64;
        if let Some(anchor) = index.packet(offset, packet)
            && index.view(anchor.time_ns).status == Status::Available
        {
            return (index, offset + TS_PACKET_SIZE as u64);
        }
    }
    panic!("fixture must acquire a present program");
}

fn separate_clock(index: &mut Index, offset: &mut u64) {
    let mut section = index.last_pmt[..index.last_pmt.len() - CRC_BYTES].to_vec();
    section[PCR_PID_OFFSET..PCR_PID_OFFSET + 2]
        .copy_from_slice(&(PID_RESERVED_BITS | SEPARATE_CLOCK_PID.0).to_be_bytes());
    publish_pmt(index, offset, section);
    assert_eq!(index.pcr_pid, Some(SEPARATE_CLOCK_PID));
}

fn publish_pmt(index: &mut Index, offset: &mut u64, mut section: Vec<u8>) {
    let length = (section.len() + CRC_BYTES - SECTION_PREFIX_BYTES) as u16;
    section[1..SECTION_PREFIX_BYTES]
        .copy_from_slice(&(SECTION_RESERVED_BITS | length).to_be_bytes());
    section.extend_from_slice(&crate::transport::wire::crc32_mpeg(&section).to_be_bytes());
    for packet in packetize(index.pmt_pid.unwrap(), &section)
        .as_chunks::<TS_PACKET_SIZE>()
        .0
    {
        index.packet(*offset, packet);
        *offset += TS_PACKET_SIZE as u64;
    }
}

fn timed_pes(pid: Pid, pts: u64) -> [u8; TS_PACKET_SIZE] {
    let mut packet = [STUFFING_BYTE; TS_PACKET_SIZE];
    packet[..TS_HEADER_BYTES].copy_from_slice(&[
        SYNC_BYTE,
        PAYLOAD_START_FLAG | (pid.0 >> u8::BITS) as u8,
        pid.0 as u8,
        PAYLOAD_ONLY,
    ]);
    // A PES header is enough to index its presentation time. Decoding and
    // hardware are deliberately outside this packet-level test.
    let stream_id = if pid == AUDIO_PID {
        AUDIO_STREAM_ID
    } else {
        VIDEO_STREAM_ID
    };
    let header = [
        0,
        0,
        1,
        stream_id,
        0,
        0,
        PES_MARKER,
        PTS_ONLY,
        TIMESTAMP_BYTES,
        PES_TIMESTAMP_PREFIX | (((pts >> 30) & 7) as u8) << 1 | TIMESTAMP_MARKER,
        (pts >> 22) as u8,
        (((pts >> 15) & TIMESTAMP_PART_MASK) as u8) << 1 | TIMESTAMP_MARKER,
        (pts >> 7) as u8,
        ((pts & TIMESTAMP_PART_MASK) as u8) << 1 | TIMESTAMP_MARKER,
    ];
    packet[TS_HEADER_BYTES..TS_HEADER_BYTES + header.len()].copy_from_slice(&header);
    packet
}

#[test]
fn separate_pcr_pid_keeps_received_video_and_audio_programs_available() {
    use crate::playback::{
        live_timeline::{History, Presenter, Reading},
        timeline::Phase,
    };
    for pid in [VIDEO_PID, AUDIO_PID] {
        let (mut index, mut offset) = acquired();
        separate_clock(&mut index, &mut offset);
        let (pcr, ticks) = index.clock.unwrap();
        let presentation = ticks_to_ns(ticks + PCR_HZ);
        let pts = (pcr + PCR_HZ) % PCR_WRAP;
        let received_end = index.end_ns();
        assert!(presentation > received_end.unwrap());
        let mut history = History::from_catalog(index.catalog());
        history.observe(
            index.epoch,
            ticks_to_ns(ticks),
            received_end.unwrap(),
            index.programs.as_ref(),
        );
        let mut presenter = Presenter::new();
        let before = presenter.project(
            &mut history,
            ticks_to_ns(ticks),
            received_end.unwrap(),
            false,
            Reading {
                phase: Phase::Playing,
                position_ns: Some(ticks_to_ns(ticks)),
                target_ns: None,
            },
        );
        assert_eq!(before.program_status(), Status::Available);

        // A PES from another service must not extend this service's scope.
        index.packet(offset, &timed_pes(FOREIGN_PID, pts));
        offset += TS_PACKET_SIZE as u64;
        assert_eq!(index.end_ns(), received_end);

        index.packet(offset, &timed_pes(pid, pts));
        let view = index.view(presentation);
        assert_eq!(view.status, Status::Available, "PID {pid:?}");
        assert_eq!(view.program.unwrap().event_id, 1);
        assert!(index.end_ns().unwrap() > presentation);
        history.advance_end(index.end_ns().unwrap());
        let after = presenter.project(
            &mut history,
            ticks_to_ns(ticks),
            index.end_ns().unwrap(),
            false,
            Reading {
                phase: Phase::Playing,
                position_ns: Some(presentation),
                target_ns: None,
            },
        );
        assert_eq!(after.program_status(), Status::Available);
        let program: serde_json::Value = serde_json::from_str(after.viewing_program().0).unwrap();
        assert_eq!(program["eventId"], 1);
    }
}

#[test]
fn pmt_changes_remove_old_pids_and_ancillary_pts_cannot_extend_media() {
    use gstreamer_mpegts::ffi as mpegts;
    let (mut index, mut offset) = acquired();
    separate_clock(&mut index, &mut offset);
    let mut section = index.last_pmt[..PCR_PID_OFFSET].to_vec();
    section.extend_from_slice(&(PID_RESERVED_BITS | SEPARATE_CLOCK_PID.0).to_be_bytes());
    section.extend_from_slice(&EMPTY_DESCRIPTOR_LOOP);
    for (kind, pid) in [
        (mpegts::GST_MPEGTS_STREAM_TYPE_VIDEO_MPEG2, FOREIGN_PID),
        (
            mpegts::GST_MPEGTS_STREAM_TYPE_PRIVATE_PES_PACKETS,
            AUDIO_PID,
        ),
    ] {
        section.push(kind as u8);
        section.extend_from_slice(&(PID_RESERVED_BITS | pid.0).to_be_bytes());
        section.extend_from_slice(&EMPTY_DESCRIPTOR_LOOP);
    }
    publish_pmt(&mut index, &mut offset, section);
    let (pcr, ticks) = index.clock.unwrap();
    let pts = (pcr + PCR_HZ) % PCR_WRAP;
    let previous_end = index.end_ns();
    for pid in [VIDEO_PID, AUDIO_PID] {
        index.packet(offset, &timed_pes(pid, pts));
        offset += TS_PACKET_SIZE as u64;
        assert_eq!(
            index.end_ns(),
            previous_end,
            "removed or private PID {pid:?}"
        );
    }
    index.packet(offset, &timed_pes(FOREIGN_PID, pts));
    offset += TS_PACKET_SIZE as u64;
    assert_eq!(
        index.view(ticks_to_ns(ticks + PCR_HZ)).status,
        Status::Available
    );

    let previous_end = index.end_ns();
    index.discontinuity();
    index.packet(offset, &timed_pes(FOREIGN_PID, (pts + PCR_HZ) % PCR_WRAP));
    assert_eq!(
        index.end_ns(),
        previous_end,
        "reconnect must wait for fresh tables"
    );
}

#[test]
#[ignore = "hardware-free received timestamp probe; requires NAGAMETV_LIVE_METADATA_PROBE"]
fn captured_broadcast_covers_received_presentation_timestamps()
-> Result<(), Box<dyn std::error::Error>> {
    use std::io::Read;
    const PROBE_BYTES: u64 = 64 * 1024 * 1024;
    let path =
        std::env::var_os("NAGAMETV_LIVE_METADATA_PROBE").ok_or("NAGAMETV_LIVE_METADATA_PROBE")?;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(PROBE_BYTES)
        .read_to_end(&mut bytes)?;
    let framing = crate::transport::framing::Framing::detect(&bytes).ok_or("TS framing")?;
    let mut index = Index::new(0, true);
    let mut checked = 0;
    let mut separate_clock = 0;
    for (offset, raw) in framing.packets(&bytes[framing.offset() as usize..]) {
        index.packet(offset, raw);
        let Ok(packet) = TransportPacket::parse(raw) else {
            continue;
        };
        if !packet.start || !index.presentation_pids.contains(&packet.pid) {
            continue;
        }
        let Some(pts) = crate::transport::pes::PesHeader::parse(packet.payload)
            .and_then(|header| header.pts_ticks)
        else {
            continue;
        };
        let Some((pcr, ticks)) = index.clock else {
            continue;
        };
        let distance = (pts + PCR_WRAP - pcr) % PCR_WRAP;
        if distance >= DISCONTINUITY_TICKS
            || index.view(ticks_to_ns(ticks)).status != Status::Available
        {
            continue;
        }
        let presentation = ticks_to_ns(ticks + distance);
        assert_eq!(
            index.view(presentation).status,
            Status::Available,
            "offset={offset} pid={:?} presentation={presentation} end={:?}",
            packet.pid,
            index.end_ns()
        );
        checked += 1;
        separate_clock += usize::from(Some(packet.pid) != index.pcr_pid);
    }
    assert!(
        checked > 0 && separate_clock > 0,
        "probe needs acquired programs and PTS on a non-PCR PID"
    );
    eprintln!(
        "Received timestamp probe: {checked} headers, {separate_clock} on non-PCR PIDs, {} bytes",
        bytes.len()
    );
    Ok(())
}
