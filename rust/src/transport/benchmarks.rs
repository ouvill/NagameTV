//! Manual CPU-only measurements; no media decoding, display, audio or network access.
use super::{Sections, programs::Collector, wire::*};
use std::{hint::black_box, time::Instant};

#[test]
#[ignore = "manual transport CPU benchmark: run with --ignored --nocapture"]
fn benchmark_transport_processing() {
    const ROUNDS: usize = 10_000;
    const SAMPLES: usize = 5;
    const SAMPLE_PACKETS: usize = 64;
    const FIXTURE_SERVICE: u16 = 1;
    const FIXTURE_TRANSPORT: u16 = 1;
    const FIXTURE_PCR: Pid = Pid(0x41);
    const SDT: Pid = Pid(0x11);
    const EIT: Pid = Pid(0x12);
    const TIME_TABLE: Pid = Pid(0x14);
    let fixture = include_bytes!("../../../tests/fixtures/recording-seek.ts");
    let packets: Vec<_> = fixture
        .as_chunks::<TS_PACKET_SIZE>()
        .0
        .iter()
        .map(|bytes| {
            (
                bytes,
                TransportPacket::parse(bytes).expect("valid fixture packet"),
            )
        })
        .filter(|(_, packet)| [SDT, EIT, TIME_TABLE].contains(&packet.pid))
        .take(SAMPLE_PACKETS)
        .collect();
    assert!(!packets.is_empty());
    let mut assembly = Sections::si();
    let sections: Vec<_> = packets
        .iter()
        .filter(|(_, packet)| packet.pid == EIT)
        .flat_map(|(_, packet)| assembly.push(packet.start, packet.payload))
        .collect();
    assert!(!sections.is_empty());
    for _ in 0..SAMPLES {
        let start = Instant::now();
        for _ in 0..ROUNDS {
            for section in &sections {
                black_box(crc32_mpeg(black_box(section)));
            }
        }
        eprintln!(
            "transport_crc sections={} elapsed_us={}",
            ROUNDS * sections.len(),
            start.elapsed().as_micros()
        );

        let mut collector = Collector::new(FIXTURE_SERVICE);
        collector.transport(FIXTURE_TRANSPORT);
        collector.pcr_pid(FIXTURE_PCR);
        let start = Instant::now();
        for _ in 0..ROUNDS {
            for (bytes, packet) in &packets {
                collector.packet(black_box(packet), black_box(bytes));
            }
        }
        black_box(collector);
        eprintln!(
            "transport_si packets={} elapsed_us={}",
            ROUNDS * packets.len(),
            start.elapsed().as_micros()
        );
    }
}
