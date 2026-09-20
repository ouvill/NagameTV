use super::*;
use gstreamer::{self as gst, prelude::*};
use std::time::Instant;

const TEST_DEADLINE: Duration = Duration::from_secs(5);
const TEST_POLL: Duration = Duration::from_millis(5);

#[test]
#[ignore = "hardware-free bounded I/O probe; requires NAGAMETV_RECORDING_PROBE"]
fn recording_metadata_probe_uses_bounded_io_and_stays_idle()
-> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::var_os("NAGAMETV_RECORDING_PROBE").ok_or("NAGAMETV_RECORDING_PROBE")?;
    let started = Instant::now();
    let recording = crate::playback::recording::Recording::open(Path::new(&path))?;
    let (mut reader, _worker) = file_reader_inspected(
        recording.path(),
        recording.service(),
        true,
        recording.inspection(),
    )?;
    let Shared::File(shared) = reader.shared.clone() else {
        unreachable!()
    };
    let deadline = Instant::now() + TEST_DEADLINE;
    loop {
        let ready = {
            let state = shared.lock().unwrap();
            matches!(state.discovery, Discovery::Finished) && state.additional_bytes > 0
        };
        if ready {
            break;
        }
        assert!(Instant::now() < deadline, "initial metadata did not finish");
        std::thread::sleep(TEST_POLL);
    }
    let before = shared.lock().unwrap().additional_bytes;
    assert!(
        before <= 12 * 1024 * 1024,
        "initial additional bytes: {before}"
    );
    {
        let state = shared.lock().unwrap();
        let start = state.index.entries().front().unwrap().time_ns;
        let view = state.index.view(start);
        eprintln!(
            "initial: elapsed_ms={} additional_bytes={} current={:?} next={:?}",
            started.elapsed().as_millis(),
            before,
            view.program
                .as_ref()
                .map(|p| (&p.name, p.start_at, p.duration)),
            view.next
                .as_ref()
                .map(|p| (&p.name, p.start_at, p.duration))
        );
        assert!(view.program.is_some() || view.next.is_some());
    }
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        shared.lock().unwrap().additional_bytes,
        before,
        "idle input kept scanning"
    );
    let target = reader.shared.window()?.ok_or("seek window")?.end * 4 / 5;
    let seeking = Instant::now();
    reader.prepare(target)?.execute()?;
    let deadline = Instant::now() + TEST_DEADLINE;
    while reader.time_ns < target {
        assert!(
            Instant::now() < deadline,
            "seek preroll did not reach target"
        );
        if matches!(reader.next()?, Output::End) {
            break;
        }
    }
    std::thread::sleep(Duration::from_millis(300));
    let after = shared.lock().unwrap().additional_bytes;
    assert!(
        after - before <= 32 * 1024 * 1024,
        "seek additional bytes: {}",
        after - before
    );
    let view = reader.clock.view(reader.time_ns);
    assert!(
        view.clock.is_some(),
        "seek did not acquire its own broadcast clock"
    );
    assert!(
        view.program.is_some(),
        "seek did not acquire its own programme"
    );
    eprintln!(
        "seek: elapsed_ms={} additional_bytes={} target_ns={} actual_ns={} program={:?}",
        seeking.elapsed().as_millis(),
        after - before,
        target,
        reader.time_ns,
        view.program.as_ref().map(|p| &p.name)
    );
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        shared.lock().unwrap().additional_bytes,
        after,
        "seek resumed a sequential scan"
    );
    Ok(())
}

#[test]
fn normalized_raw_input_seeks_across_pid_changes_without_losing_pause()
-> Result<(), Box<dyn std::error::Error>> {
    gst::init()?;
    for name in [
        "recording-seek.ts",
        "recording-pid-change.ts",
        "recording-clock-reset.ts",
    ] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures")
            .join(name);
        let (reader, worker) = file_reader(&path, 1, true)?;
        let shared = reader.shared.clone();
        let deadline = Instant::now() + TEST_DEADLINE;
        while !shared
            .window()?
            .is_some_and(|window| matches!(window.coverage, Coverage::Complete))
        {
            assert!(Instant::now() < deadline, "index timeout");
            std::thread::sleep(TEST_POLL);
        }
        let pipeline = gst::parse::launch("appsrc name=source ! tsdemux name=demux demux. ! queue ! mpegvideoparse ! avdec_mpeg2video ! appsink name=output max-buffers=1 drop=true sync=true")?.downcast::<gst::Pipeline>().map_err(|_| "pipeline")?;
        struct Stop(gst::Pipeline, Arc<AtomicBool>);
        impl Drop for Stop {
            fn drop(&mut self) {
                self.1.store(true, Ordering::Release);
                let _ = self.0.set_state(gst::State::Null);
            }
        }
        let interrupted = Arc::new(AtomicBool::new(false));
        let stop = Stop(pipeline.clone(), interrupted.clone());
        let scope = crate::features::subscriptions::Subscriptions::default();
        source::configure(
            &pipeline
                .by_name("source")
                .ok_or("source")?
                .downcast()
                .map_err(|_| "appsrc")?,
            Arc::new(Mutex::new(reader)),
            interrupted,
            Arc::new(source::Feedback::default()),
            &scope,
        );
        let sink = pipeline.by_name("output").ok_or("sink")?;
        let mut controller = super::super::timeline::Controller::new(
            &sink,
            super::super::timeline::StartPosition::Beginning,
        )?;
        controller.pause(
            pipeline.upcast_ref(),
            super::super::timeline::Resume::Paused,
        )?;
        pipeline
            .state(gst::ClockTime::from_seconds(TEST_DEADLINE.as_secs()))
            .0?;
        controller.poll(pipeline.upcast_ref())?;
        let targets: &[u64] = if name == "recording-seek.ts" {
            &[30_000, 10_000, 45_000, 5_000]
        } else {
            &[4_500, 1_000, 4_500]
        };
        for (step, &target) in targets.iter().enumerate() {
            controller
                .prepare(pipeline.upcast_ref())?
                .seek(target as f64)?;
            let deadline = Instant::now() + TEST_DEADLINE;
            while matches!(
                controller.phase(),
                super::super::timeline::Phase::Seeking(_)
            ) {
                controller.poll(pipeline.upcast_ref())?;
                assert!(Instant::now() < deadline, "{name}: timed out at {target}");
                std::thread::sleep(TEST_POLL);
            }
            assert_eq!(controller.phase(), super::super::timeline::Phase::Paused);
            let actual = controller.snapshot().position.ok_or("position")?.mseconds();
            const POSITION_TOLERANCE_MS: u64 = 500;
            assert!(
                actual.abs_diff(target) < POSITION_TOLERANCE_MS,
                "{name}: requested {target}, got {actual}"
            );
            // This video-only pipeline also crosses PMT/PID and clock resets.
            let rates = [5, 11, 15, 20];
            let rate = crate::playback::speed::Rate::checked(rates[step]).unwrap();
            controller.prepare(pipeline.upcast_ref())?.set_rate(rate)?;
            let deadline = Instant::now() + TEST_DEADLINE;
            while matches!(
                controller.phase(),
                super::super::timeline::Phase::Seeking(_)
            ) {
                controller.poll(pipeline.upcast_ref())?;
                assert!(Instant::now() < deadline, "{name}: rate change timed out");
                std::thread::sleep(TEST_POLL);
            }
            assert_eq!(controller.speed().applied, rate);
            assert_eq!(controller.phase(), super::super::timeline::Phase::Paused);
        }
        drop(stop);
        scope.close();
        drop(worker);
    }
    Ok(())
}

#[test]
fn raw_program_index_follows_playhead_and_survives_backwards_seek()
-> Result<(), Box<dyn std::error::Error>> {
    let bytes = include_bytes!("../../../../tests/fixtures/recording-seek.ts");
    let mut index = Index::new(1, true);
    for (number, packet) in bytes.as_chunks::<TS_PACKET_SIZE>().0.iter().enumerate() {
        index.packet((number * TS_PACKET_SIZE) as u64, packet);
    }
    const EARLY_NS: u64 = 10_000_000_000;
    const LATE_NS: u64 = 40_000_000_000;
    let early = index.program(EARLY_NS).ok_or("early program")?;
    let late = index.program(LATE_NS).ok_or("late program")?;
    assert_ne!(early.0, late.0);
    index.clear_entries();
    assert_eq!(index.program(EARLY_NS), Some(early));
    assert_eq!(index.program(LATE_NS), Some(late));
    Ok(())
}

#[test]
fn sequential_playback_across_discontinuities_keeps_frames_and_monotonic_clock()
-> Result<(), Box<dyn std::error::Error>> {
    gst::init()?;
    for name in ["recording-pid-change.ts", "recording-clock-reset.ts"] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures")
            .join(name);
        let (reader, _worker) = file_reader(&path, 1, true)?;
        let pipeline = gst::parse::launch("appsrc name=source ! tsdemux name=demux demux. ! queue ! mpegvideoparse ! avdec_mpeg2video ! fakesink name=output sync=false demux. ! queue ! aacparse ! avdec_aac ! fakesink name=audio sync=false")?.downcast::<gst::Pipeline>().map_err(|_| "pipeline")?;
        let interrupted = Arc::new(AtomicBool::new(false));
        let scope = crate::features::subscriptions::Subscriptions::default();
        let failure = Arc::new(source::Feedback::default());
        source::configure(
            &pipeline
                .by_name("source")
                .ok_or("source")?
                .downcast()
                .map_err(|_| "appsrc")?,
            Arc::new(Mutex::new(reader)),
            interrupted.clone(),
            failure.clone(),
            &scope,
        );
        let frames = Arc::new(Mutex::new(Vec::<u64>::new()));
        let output = frames.clone();
        let pad = pipeline
            .by_name("output")
            .ok_or("sink")?
            .static_pad("sink")
            .ok_or("pad")?;
        scope.probe(
            &pad,
            pad.add_probe(gst::PadProbeType::BUFFER, move |_, info| {
                if let Some(pts) = info.buffer().and_then(|buffer| buffer.pts()) {
                    output.lock().unwrap().push(pts.nseconds());
                }
                gst::PadProbeReturn::Ok
            }),
        );
        pipeline.set_state(gst::State::Playing)?;
        let message = pipeline.bus().ok_or("bus")?.timed_pop_filtered(
            gst::ClockTime::from_seconds(TEST_DEADLINE.as_secs()),
            &[gst::MessageType::Error, gst::MessageType::Eos],
        );
        interrupted.store(true, Ordering::Release);
        pipeline.set_state(gst::State::Null)?;
        scope.close();
        assert!(
            message.is_some_and(|message| message.type_() == gst::MessageType::Eos),
            "{name}: {:?}",
            failure.failure.lock().unwrap()
        );
        let frames = frames.lock().unwrap();
        const EXPECTED_FRAMES: usize = 150;
        const BOUNDARY_TOLERANCE: usize = 2;
        assert!(
            frames.len() >= EXPECTED_FRAMES - BOUNDARY_TOLERANCE,
            "{name}: {} frames",
            frames.len()
        );
        assert!(
            frames.windows(2).all(|pair| pair[0] <= pair[1]),
            "{name}: output clock regressed"
        );
    }
    Ok(())
}

#[test]
fn file_framing_preserves_packets_for_ts_m2ts_and_parity_frames()
-> Result<(), Box<dyn std::error::Error>> {
    let ts = include_bytes!("../../../../tests/fixtures/recording.ts");
    let directory = tempfile::tempdir()?;
    let mut expected = None;
    const M2TS_PREFIX: usize = 4;
    const RS_PARITY: usize = 16;
    for (prefix, suffix) in [(0, 0), (M2TS_PREFIX, 0), (0, RS_PARITY)] {
        let path = directory
            .path()
            .join(format!("framing-{prefix}-{suffix}.ts"));
        let bytes: Vec<_> = ts
            .as_chunks::<TS_PACKET_SIZE>()
            .0
            .iter()
            .flat_map(|packet| {
                std::iter::repeat_n(0, prefix)
                    .chain(packet.iter().copied())
                    .chain(std::iter::repeat_n(0, suffix))
            })
            .collect();
        std::fs::write(&path, bytes)?;
        let recording = super::super::recording::Recording::open(&path)?;
        let (mut reader, _worker) = file_reader(&path, recording.service(), true)?;
        let mut output = Vec::new();
        loop {
            match reader.next()? {
                Output::Data { bytes, .. } => output.extend(bytes),
                Output::End => break,
                Output::Filtered => {}
                Output::Awaiting | Output::Expired => panic!("file did not complete"),
            }
        }
        if let Some(expected) = &expected {
            assert_eq!(&output, expected);
        } else {
            expected = Some(output);
        }
    }
    Ok(())
}

#[test]
fn stopping_detaches_reader_even_when_playbin_keeps_its_appsrc()
-> Result<(), Box<dyn std::error::Error>> {
    gst::init()?;
    // Both elements remain in NULL. No sink, decoder or output device is opened.
    let playbin = gst::ElementFactory::make("playbin3").build()?;
    let source = gstreamer_app::AppSrc::builder().build();
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/recording.ts");
    let input = Input::file(&playbin, &path, 1, true)?;
    let weak = input.index_lifetime().ok_or("file index lifetime")?;
    playbin.emit_by_name::<()>("source-setup", &[source.upcast_ref::<gst::Element>()]);
    drop(input);
    let deadline = Instant::now() + TEST_DEADLINE;
    while weak.upgrade().is_some() {
        assert!(
            Instant::now() < deadline,
            "appsrc kept the stopped reader alive"
        );
        std::thread::sleep(TEST_POLL);
    }
    drop(source);
    Ok(())
}

#[test]
fn expired_pause_keeps_its_frame_then_resumes_inside_history()
-> Result<(), Box<dyn std::error::Error>> {
    exercise_expired_pause(Expiry::Natural)
}

#[test]
fn reduced_limits_move_the_paused_frame_without_resuming() -> Result<(), Box<dyn std::error::Error>>
{
    exercise_expired_pause(Expiry::Settings)
}

enum Expiry {
    Natural,
    Settings,
}
fn exercise_expired_pause(expiry: Expiry) -> Result<(), Box<dyn std::error::Error>> {
    gst::init()?;
    let fixture = include_bytes!("../../../../tests/fixtures/recording-seek.ts");
    let bytes = fixture.repeat(2);
    const ADVANCED_SECONDS: u64 = 70;
    let mut index = Index::new(1, true);
    for (number, packet) in bytes.as_chunks::<TS_PACKET_SIZE>().0.iter().enumerate() {
        index.packet((number * TS_PACKET_SIZE) as u64, packet);
    }
    let offset_at = |seconds| {
        index
            .entries()
            .iter()
            .find(|anchor| anchor.time_ns >= gst::ClockTime::from_seconds(seconds).nseconds())
            .unwrap()
            .offset as usize
    };
    let initial_end = offset_at(1);
    let advanced_end = offset_at(ADVANCED_SECONDS);
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
    store.lock().unwrap().append(&bytes[..initial_end])?;
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
    let pipeline = gst::parse::launch("appsrc name=source ! tsdemux name=demux demux. ! queue ! mpegvideoparse ! avdec_mpeg2video ! fakesink name=output sync=true")?.downcast::<gst::Pipeline>().map_err(|_| "pipeline")?;
    let interrupted = Arc::new(AtomicBool::new(false));
    struct Stop(gst::Pipeline, Arc<AtomicBool>);
    impl Drop for Stop {
        fn drop(&mut self) {
            self.1.store(true, Ordering::Release);
            let _ = self.0.set_state(gst::State::Null);
        }
    }
    let stop = Stop(pipeline.clone(), interrupted.clone());
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
    let mut controller = super::super::timeline::Controller::new(
        &pipeline.by_name("output").ok_or("sink")?,
        super::super::timeline::StartPosition::LiveEdge(crate::settings::LiveBuffer::default()),
    )?;
    controller.pause(
        pipeline.upcast_ref(),
        super::super::timeline::Resume::Paused,
    )?;
    pipeline
        .state(gst::ClockTime::from_seconds(TEST_DEADLINE.as_secs()))
        .0?;
    controller.poll(pipeline.upcast_ref())?;
    store
        .lock()
        .unwrap()
        .append(&bytes[initial_end..advanced_end])?;
    assert!(matches!(
        store.lock().unwrap().read(0)?,
        ReadResult::Expired
    ));
    let window = shared.window()?.ok_or("window")?;
    if let Expiry::Settings = expiry {
        controller.retention_changed(super::super::timeline::RetentionChange::ClampPosition);
    }
    controller.retained(
        pipeline.upcast_ref(),
        super::super::timeline::LiveWindow::History(
            super::super::timeline::Range::new(
                gst::ClockTime::from_nseconds(window.start),
                gst::ClockTime::from_nseconds(window.end),
            )
            .unwrap(),
        ),
        true,
        feedback.read_progress()?,
    )?;
    match expiry {
        Expiry::Natural => {
            assert_eq!(controller.phase(), super::super::timeline::Phase::Paused);
            assert!(
                controller
                    .snapshot()
                    .position
                    .ok_or("paused position")?
                    .nseconds()
                    < window.start
            );
        }
        Expiry::Settings => {
            let deadline = Instant::now() + TEST_DEADLINE;
            while matches!(
                controller.phase(),
                super::super::timeline::Phase::Seeking(_)
            ) {
                controller.poll(pipeline.upcast_ref())?;
                assert!(Instant::now() < deadline, "paused correction timed out");
                std::thread::sleep(TEST_POLL);
            }
            assert_eq!(controller.phase(), super::super::timeline::Phase::Paused);
            assert!(
                controller
                    .snapshot()
                    .position
                    .ok_or("corrected position")?
                    .nseconds()
                    >= window.start
            );
        }
    }
    controller.pause(
        pipeline.upcast_ref(),
        super::super::timeline::Resume::Playing,
    )?;
    let deadline = Instant::now() + TEST_DEADLINE;
    while matches!(
        controller.phase(),
        super::super::timeline::Phase::Seeking(_)
    ) {
        controller.poll(pipeline.upcast_ref())?;
        assert!(Instant::now() < deadline, "expired seek timed out");
        std::thread::sleep(TEST_POLL);
    }
    assert_eq!(controller.phase(), super::super::timeline::Phase::Playing);
    assert!(controller.snapshot().position.ok_or("position")?.nseconds() >= window.start);
    assert!(controller.take_notice().is_some());
    // Keep receiving at broadcast speed after resume. A point on the oldest
    // edge used to expire repeatedly while its replacement seek was prerolling.
    const FOLLOWUP_SECONDS: u64 = 3;
    let resumed = Instant::now();
    let mut received_until = advanced_end;
    let mut previous_position = controller.snapshot().position.ok_or("position")?;
    while resumed.elapsed() < Duration::from_secs(FOLLOWUP_SECONDS) {
        let next = offset_at(ADVANCED_SECONDS + resumed.elapsed().as_secs());
        if next > received_until {
            store.lock().unwrap().append(&bytes[received_until..next])?;
            received_until = next;
        }
        controller.poll(pipeline.upcast_ref())?;
        let window = shared.window()?.ok_or("window")?;
        controller.retained(
            pipeline.upcast_ref(),
            super::super::timeline::LiveWindow::History(
                super::super::timeline::Range::new(
                    gst::ClockTime::from_nseconds(window.start),
                    gst::ClockTime::from_nseconds(window.end),
                )
                .unwrap(),
            ),
            feedback.take_expired(),
            feedback.read_progress()?,
        )?;
        assert_eq!(
            controller.phase(),
            super::super::timeline::Phase::Playing,
            "resume must not enter a repeated recovery seek"
        );
        let position = controller.snapshot().position.ok_or("playing position")?;
        assert!(position >= previous_position);
        previous_position = position;
        assert!(controller.take_notice().is_none());
        std::thread::sleep(TEST_POLL);
    }
    drop(stop);
    scope.close();
    Ok(())
}

#[test]
fn normalizer_announces_and_feeds_missing_captions_before_broadcast_changes() {
    use crate::transport::{
        Sections,
        wire::{Pid, PsiSection, TransportPacket},
    };
    const NORMALIZED_PMT: Pid = Pid(0x01f0);
    let fixture = include_bytes!("../../../../tests/fixtures/recording-seek.ts");
    let output = tsreadex::Filter::new(1).unwrap().push(fixture).unwrap();
    let mut sections = std::collections::BTreeMap::<Pid, Sections>::new();
    let mut caption = None;
    let mut payload_seen = false;
    for bytes in output.as_chunks::<TS_PACKET_SIZE>().0 {
        let packet = TransportPacket::parse(bytes).unwrap();
        if caption == Some(packet.pid) && packet.start && !packet.payload.is_empty() {
            payload_seen = true;
        }
        if packet.pid != NORMALIZED_PMT {
            continue;
        }
        for data in sections
            .entry(packet.pid)
            .or_default()
            .push(packet.start, packet.payload)
        {
            if let Ok(section) = PsiSection::parse(&data)
                && let Ok(map) = section.program_map()
            {
                assert!(
                    !map.captions.is_empty(),
                    "PMT must announce captions even before they appear in the broadcast"
                );
                caption = Some(map.captions[0].pid);
            }
        }
    }
    assert!(caption.is_some());
    assert!(
        payload_seen,
        "announced caption stream must receive management data for preroll"
    );
}

#[test]
fn slow_short_read_does_not_start_another_io_after_exploration_deadline() {
    struct SlowFile {
        reads: usize,
    }
    impl Read for SlowFile {
        fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
            self.reads += 1;
            std::thread::sleep(Duration::from_millis(40));
            bytes[0] = 0;
            Ok(1)
        }
    }
    let mut source = SlowFile { reads: 0 };
    let deadline = Instant::now() + Duration::from_millis(20);
    assert_eq!(
        read_exploration(&mut source, READ_BYTES, || Instant::now() >= deadline)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(source.reads, 1);
}

#[test]
fn inspected_input_construction_never_opens_the_file_on_the_caller() {
    let inspection = crate::playback::recording::Inspection {
        prefix: Arc::new(include_bytes!("../../../../tests/fixtures/recording-seek.ts").to_vec()),
        size: u64::MAX,
        framing: Framing::transport(),
        deadline: Instant::now() - EXPLORATION_TIMEOUT,
    };
    let dir = tempfile::tempdir().unwrap();
    let (reader, _worker) =
        file_reader_inspected(&dir.path().join("unavailable.ts"), 1, true, &inspection).unwrap();
    let Shared::File(shared) = reader.shared else {
        unreachable!()
    };
    let deadline = Instant::now() + TEST_DEADLINE;
    loop {
        let state = shared.lock().unwrap();
        assert!(!matches!(state.status, Status::Failed(_)));
        if matches!(state.discovery, Discovery::Finished) {
            break;
        }
        assert!(Instant::now() < deadline);
        drop(state);
        std::thread::sleep(TEST_POLL);
    }
}

#[test]
fn playback_speed_uses_normalized_ts_and_preserves_pause_seek_and_latest_request()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::playback::{
        speed::Rate,
        timeline::{Controller, Phase, Resume, StartPosition},
    };
    gst::init()?;
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/recording-seek.ts");
    let (reader, _worker) = file_reader(&path, 1, true)?;
    let pipeline = gst::parse::launch("appsrc name=source ! tsdemux name=demux demux. ! queue ! mpegvideoparse ! avdec_mpeg2video ! fakesink name=output sync=true demux. ! queue ! aacparse ! avdec_aac ! audioconvert ! scaletempo name=tempo ! fakesink name=audio sync=true")?
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
    source::configure(
        &pipeline
            .by_name("source")
            .ok_or("source")?
            .downcast()
            .map_err(|_| "appsrc")?,
        Arc::new(Mutex::new(reader)),
        interrupted,
        Arc::new(source::Feedback::default()),
        &scope,
    );
    let sink = pipeline.by_name("output").ok_or("output")?;
    let mut controller = Controller::new(&sink, StartPosition::Beginning)?;
    let finish = |controller: &mut Controller| -> Result<(), Box<dyn std::error::Error>> {
        let deadline = Instant::now() + TEST_DEADLINE;
        while matches!(controller.phase(), Phase::Seeking(_)) {
            controller.poll(pipeline.upcast_ref())?;
            assert!(Instant::now() < deadline, "speed/seek never completed");
            std::thread::sleep(TEST_POLL);
        }
        Ok(())
    };
    controller.pause(pipeline.upcast_ref(), Resume::Paused)?;
    pipeline.state(gst::ClockTime::from_seconds(5)).0?;
    controller.poll(pipeline.upcast_ref())?;
    controller.prepare(pipeline.upcast_ref())?.seek(10_000.)?;
    finish(&mut controller)?;
    for tenths in [5, 11, 15, 20, 10] {
        controller.pause(pipeline.upcast_ref(), Resume::Paused)?;
        pipeline.state(gst::ClockTime::from_seconds(5)).0?;
        let before = pipeline
            .query_position::<gst::ClockTime>()
            .ok_or("before")?;
        let rate = Rate::checked(tenths).unwrap();
        controller.prepare(pipeline.upcast_ref())?.set_rate(rate)?;
        finish(&mut controller)?;
        assert_eq!(controller.phase(), Phase::Paused);
        assert_eq!(controller.speed().applied, rate);
        let after = pipeline.query_position::<gst::ClockTime>().ok_or("after")?;
        assert!(
            after.nseconds().abs_diff(before.nseconds())
                < gst::ClockTime::from_mseconds(150).nseconds(),
            "rate {tenths}: position jumped from {before} to {after}"
        );
        controller.pause(pipeline.upcast_ref(), Resume::Playing)?;
        pipeline.state(gst::ClockTime::from_seconds(5)).0?;
        let start = pipeline.query_position::<gst::ClockTime>().ok_or("start")?;
        let wall = Instant::now();
        std::thread::sleep(Duration::from_millis(800));
        let end = pipeline.query_position::<gst::ClockTime>().ok_or("end")?;
        let expected = wall.elapsed().as_secs_f64() * rate.multiplier();
        let actual = end.saturating_sub(start).nseconds() as f64 / 1_000_000_000.;
        assert!(
            (actual - expected).abs() < 0.15,
            "rate {tenths}: media {actual}s, expected {expected}s"
        );
        let tempo = pipeline.by_name("tempo").ok_or("tempo")?;
        assert!((tempo.property::<f64>("rate") - rate.multiplier()).abs() < 0.000_001);
    }
    controller.pause(pipeline.upcast_ref(), Resume::Paused)?;
    pipeline.state(gst::ClockTime::from_seconds(5)).0?;
    controller
        .prepare(pipeline.upcast_ref())?
        .set_rate(Rate::checked(15).unwrap())?;
    controller.prepare(pipeline.upcast_ref())?.seek(30_000.)?;
    controller
        .prepare(pipeline.upcast_ref())?
        .set_rate(Rate::checked(12).unwrap())?;
    finish(&mut controller)?;
    assert_eq!(controller.phase(), Phase::Paused);
    assert_eq!(controller.speed().applied, Rate::checked(12).unwrap());
    let position = pipeline
        .query_position::<gst::ClockTime>()
        .ok_or("position")?;
    assert!(position.mseconds().abs_diff(30_000) < 150);
    pipeline.set_state(gst::State::Ready)?;
    scope.close();
    Ok(())
}
