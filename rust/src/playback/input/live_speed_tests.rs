//! Real appsrc/demux/decode regression; explicitly memory output, no devices.
use super::*;
use crate::playback::{
    speed::Rate,
    timeline::{Controller, LiveWindow, Phase, Range, Resume, StartPosition},
};
use crate::transport::{
    pes::{PES_HEADER_BYTES, PesHeader},
    wire::{Pid, TransportPacket},
};
use gstreamer::{self as gst, prelude::*};

const PREBUFFER: Duration = Duration::from_secs(4);
const CATCH_UP_DEADLINE: Duration = Duration::from_secs(8);
const FOLLOWUP: Duration = Duration::from_secs(2);
const POLL: Duration = Duration::from_millis(10);
const AUDIO_PID: Pid = Pid(0x42);
const AUDIO_LEAD_TICKS: u64 = 72_000; // 800 ms on the 90 kHz PES clock.
const PTS_LOW_MASK: u64 = 0x7f;
const PTS_HIGH_MASK: u64 = 0x07;
const PTS_PREFIX_MASK: u8 = 0xf0;
const PTS_BYTES: usize = 5;

// An unselected elementary stream may extend the receive index past the last
// decodable video frame. Keep PCR/video intact; only move the audio PTS ahead.
fn ahead_audio_fixture() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/recording-seek.ts");
    let mut bytes = std::fs::read(path)?;
    let mut changed = 0;
    for packet in bytes.as_chunks_mut::<TS_PACKET_SIZE>().0 {
        let ts = TransportPacket::parse(packet).unwrap();
        if ts.pid != AUDIO_PID || !ts.start {
            continue;
        }
        let pts = PesHeader::parse(ts.payload).unwrap().pts_ticks.unwrap() + AUDIO_LEAD_TICKS;
        let offset = TS_PACKET_SIZE - ts.payload.len() + PES_HEADER_BYTES;
        let prefix = packet[offset] & PTS_PREFIX_MASK;
        packet[offset..offset + PTS_BYTES].copy_from_slice(&[
            prefix | (((pts >> 30) & PTS_HIGH_MASK) as u8) << 1 | 1,
            (pts >> 22) as u8,
            (((pts >> 15) & PTS_LOW_MASK) as u8) << 1 | 1,
            (pts >> 7) as u8,
            ((pts & PTS_LOW_MASK) as u8) << 1 | 1,
        ]);
        changed += 1;
    }
    assert!(changed > 0, "fixture audio PID changed");
    Ok(bytes)
}

#[test]
fn live_catch_up_recovers_when_receive_index_is_ahead_of_decodable_video()
-> Result<(), Box<dyn std::error::Error>> {
    gst::init()?;
    let bytes = ahead_audio_fixture()?;
    let mut index = Index::new(1, true);
    for (number, packet) in bytes.as_chunks::<TS_PACKET_SIZE>().0.iter().enumerate() {
        index.packet((number * TS_PACKET_SIZE) as u64, packet);
    }
    let offset_at = |time: Duration| {
        index
            .entries()
            .iter()
            .find(|anchor| anchor.time_ns >= time.as_nanos() as u64)
            .expect("fixture long enough")
            .offset as usize
    };
    let store = Arc::new(Mutex::new(Store::new(
        Policy::new(
            Retention::Memory,
            Limits::new(
                limits::MIN_CAPACITY_MIB,
                limits::MIN_CAPACITY_MIB,
                limits::MIN_RETENTION_MINUTES,
            )
            .unwrap(),
        ),
        1,
        true,
    )?));
    let mut received = offset_at(PREBUFFER);
    store.lock().unwrap().append(&bytes[..received])?;
    let shared = Shared::Live(store.clone());
    let reader = Reader {
        source: ReaderSource::Live(store.clone()),
        shared: shared.clone(),
        offset: 0,
        framing: Framing::transport(),
        service: 1,
        filter: tsreadex::Filter::new(1)?,
        clock: Index::new(1, false),
        bootstrap: Vec::new(),
        time_ns: 0,
        epoch: 0,
        pending: VecDeque::new(),
        pending_seek: None,
    };
    let pipeline = gst::parse::launch("appsrc name=source ! tsdemux name=demux demux. ! queue ! mpegvideoparse ! avdec_mpeg2video ! fakesink name=output sync=true")?
        .downcast::<gst::Pipeline>().map_err(|_| "pipeline")?;
    let interrupted = Arc::new(AtomicBool::new(false));
    struct Stop(gst::Pipeline, Arc<AtomicBool>);
    impl Drop for Stop {
        fn drop(&mut self) {
            self.1.store(true, Ordering::Release);
            let _ = self.0.set_state(gst::State::Null);
        }
    }
    let _stop = Stop(pipeline.clone(), interrupted.clone());
    let scope = crate::features::subscriptions::Subscriptions::default();
    let feedback = Arc::new(source::Feedback::default());
    source::configure(
        &pipeline
            .by_name("source")
            .ok_or("source")?
            .downcast()
            .map_err(|_| "appsrc")?,
        Arc::new(Mutex::new(reader)),
        interrupted,
        feedback.clone(),
        &scope,
    );
    let mut controller = Controller::new(
        &pipeline.by_name("output").ok_or("output")?,
        StartPosition::Beginning,
    )?;
    controller.pause(pipeline.upcast_ref(), Resume::Paused)?;
    pipeline
        .state(gst::ClockTime::from_seconds(CATCH_UP_DEADLINE.as_secs()))
        .0?;
    controller.poll(pipeline.upcast_ref())?;
    let fast = Rate::checked(20).unwrap();
    controller.prepare(pipeline.upcast_ref())?.set_rate(fast)?;
    let seeking = Instant::now();
    while matches!(controller.phase(), Phase::Seeking(_)) {
        controller.poll(pipeline.upcast_ref())?;
        assert!(
            seeking.elapsed() < CATCH_UP_DEADLINE,
            "initial rate seek timed out"
        );
        std::thread::sleep(POLL);
    }
    controller.pause(pipeline.upcast_ref(), Resume::Playing)?;
    let started = Instant::now();
    let mut restored = None;
    loop {
        let elapsed = started.elapsed();
        let next = offset_at(PREBUFFER + elapsed);
        if next > received {
            store.lock().unwrap().append(&bytes[received..next])?;
            received = next;
        }
        controller.poll(pipeline.upcast_ref())?;
        let window = shared.window()?.ok_or("window")?;
        controller.retained(
            pipeline.upcast_ref(),
            LiveWindow::History(
                Range::new(
                    gst::ClockTime::from_nseconds(window.start),
                    gst::ClockTime::from_nseconds(window.end),
                )
                .unwrap(),
            ),
            feedback.take_expired(),
            feedback.read_progress()?,
        )?;
        if matches!(controller.phase(), Phase::Seeking(_)) {
            assert!(
                elapsed < CATCH_UP_DEADLINE,
                "normal-rate seek did not finish"
            );
            std::thread::sleep(POLL);
            continue;
        }
        let position = pipeline
            .query_position::<gst::ClockTime>()
            .ok_or("position")?;
        let delay = window.end.saturating_sub(position.nseconds());
        if let Some((at, before)) = restored {
            assert_eq!(controller.speed().applied, Rate::NORMAL);
            assert!(
                controller.speed().at_live_edge,
                "live status lost after catch-up: delay={}ms",
                delay / 1_000_000
            );
            assert_eq!(controller.phase(), Phase::Playing);
            if elapsed >= at + FOLLOWUP {
                assert!(
                    position > before + gst::ClockTime::SECOND,
                    "playback stopped after catch-up"
                );
                break;
            }
        } else if controller.speed().applied == Rate::NORMAL {
            assert!(
                elapsed > Duration::from_secs(2),
                "read-ahead triggered an early return"
            );
            assert!(controller.speed().at_live_edge);
            restored = Some((elapsed, position));
        } else {
            assert!(
                elapsed < CATCH_UP_DEADLINE,
                "still at {}x after {:?}, delay={}ms, phase={:?}",
                controller.speed().applied.multiplier(),
                elapsed,
                delay / 1_000_000,
                controller.phase()
            );
        }
        std::thread::sleep(POLL);
    }
    Ok(())
}
