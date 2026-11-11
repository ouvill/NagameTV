//! Explicit CPU-only streaming tests; no display, GPU or audio devices.
use super::*;

const TEST_TIMEOUT: gst::ClockTime = gst::ClockTime::from_seconds(5);
const TEST_POLL: Duration = Duration::from_millis(5);
const RECEIVE_EDGE: gst::ClockTime = gst::ClockTime::from_seconds(10);
const USER_POSITION: gst::ClockTime = gst::ClockTime::SECOND;

#[test]
fn live_reserve_keeps_audio_on_time_through_a_short_delivery_stall()
-> Result<(), Box<dyn std::error::Error>> {
    use gstreamer_app::AppSrc;
    use std::sync::atomic::{AtomicU64, Ordering};
    const SAMPLE_RATE: u64 = 48_000;
    const PACKET: Duration = Duration::from_millis(20);
    const END: Duration = Duration::from_secs(4);
    const STALL_AT: Duration = Duration::from_secs(2);
    const STALL: Duration = Duration::from_secs(1);
    gst::init()?;
    // Explicit synchronized memory output; this checks audio deadlines without
    // a display or physical audio device. Reception is paced, never a file burst.
    let pipeline = gst::parse::launch(&format!(
        "appsrc name=source format=time max-bytes=1000000 ! audio/x-raw,format=S16LE,rate={SAMPLE_RATE},channels=1,layout=interleaved ! fakesink name=output sync=true"
    ))?.downcast::<gst::Pipeline>().map_err(|_| "pipeline")?;
    let sink = pipeline.by_name("output").ok_or("output")?;
    sink.set_property("max-lateness", PACKET.as_nanos() as i64);
    let controller = Controller::new(&sink, StartPosition::LiveEdge(LiveBuffer::default()))?;
    let mut playback = Playback {
        pipeline,
        controller,
    };
    let source = playback
        .pipeline
        .by_name("source")
        .ok_or("source")?
        .downcast::<AppSrc>()
        .map_err(|_| "appsrc")?;
    let received = Arc::new(AtomicU64::new(0));
    let edge = received.clone();
    let began = Instant::now();
    // Reception must continue independently while the pipeline prerolls.
    let receiver = std::thread::spawn(move || -> Result<(), gst::FlowError> {
        let mut pts = Duration::ZERO;
        while pts < END {
            let arrival = if (STALL_AT..STALL_AT + STALL).contains(&pts) {
                STALL_AT + STALL
            } else {
                pts
            };
            std::thread::sleep((began + arrival).saturating_duration_since(Instant::now()));
            let mut buffer = gst::Buffer::from_mut_slice(vec![
                0;
                (SAMPLE_RATE * 2 * PACKET.as_millis() as u64 / 1000)
                    as usize
            ]);
            buffer
                .make_mut()
                .set_pts(gst::ClockTime::from_nseconds(pts.as_nanos() as u64));
            buffer
                .make_mut()
                .set_duration(gst::ClockTime::from_nseconds(PACKET.as_nanos() as u64));
            edge.store((pts + PACKET).as_nanos() as u64, Ordering::Release);
            source.push_buffer(buffer)?;
            pts += PACKET;
        }
        source.end_of_stream()?;
        Ok(())
    });
    playback.state(playback.controller.start().state())?;
    let bus = playback.pipeline.bus().ok_or("bus")?;
    let deadline = began + END + Duration::from_secs(4);
    let mut finished = false;
    while Instant::now() < deadline {
        playback.controller.poll(playback.pipeline.upcast_ref())?;
        let end = gst::ClockTime::from_nseconds(received.load(Ordering::Acquire));
        if let Some(range) = Range::new(gst::ClockTime::ZERO, end) {
            playback.controller.retained(
                playback.pipeline.upcast_ref(),
                LiveWindow::ForwardBuffer(range),
                false,
                crate::playback::input::ReadProgress::idle(),
            )?;
        }
        while let Some(message) = bus.pop() {
            assert_ne!(message.type_(), gst::MessageType::Error, "{message:?}");
            finished |= message.type_() == gst::MessageType::Eos;
        }
        if finished {
            break;
        }
        std::thread::sleep(TEST_POLL);
    }
    receiver.join().expect("receiver panicked")?;
    assert!(finished, "buffered audio did not finish");
    let counts = crate::playback::stats::frame_counters(&sink).ok_or("audio counters")?;
    assert_eq!(
        counts.rendered,
        (END.as_millis() / PACKET.as_millis()) as u64
    );
    assert_eq!(counts.dropped, 0, "audio missed its playback deadline");
    assert!(playback.controller.seek_target().is_none());
    Ok(())
}

struct Playback {
    pipeline: gst::Pipeline,
    controller: Controller,
}

impl Playback {
    fn new(start: StartPosition) -> Result<Self, Box<dyn std::error::Error>> {
        gst::init()?;
        let pipeline = gst::parse::launch(
            "videotestsrc name=source ! video/x-raw,width=16,height=16,framerate=25/1 ! fakesink name=output sync=true",
        )?
        .downcast::<gst::Pipeline>()
        .map_err(|_| "pipeline")?;
        let controller = Controller::new(&pipeline.by_name("output").ok_or("sink")?, start)?;
        Ok(Self {
            pipeline,
            controller,
        })
    }

    fn state(&self, state: gst::State) -> Result<(), Box<dyn std::error::Error>> {
        self.pipeline.set_state(state)?;
        self.pipeline.state(TEST_TIMEOUT).0?;
        if state == gst::State::Playing {
            let sink = self.pipeline.by_name("output").ok_or("sink")?;
            let deadline = Instant::now() + Duration::from_nanos(TEST_TIMEOUT.nseconds());
            while !crate::playback::stats::frame_counters(&sink)
                .is_some_and(|counts| counts.rendered > 0)
            {
                assert!(Instant::now() < deadline, "no rendered output");
                std::thread::sleep(TEST_POLL);
            }
        }
        Ok(())
    }

    fn finish_seek(&mut self) -> Result<(), Error> {
        let deadline = Instant::now() + Duration::from_nanos(TEST_TIMEOUT.nseconds());
        while matches!(self.controller.phase(), Phase::Seeking(_)) {
            self.controller.poll(self.pipeline.upcast_ref())?;
            assert!(Instant::now() < deadline, "seek did not complete");
            std::thread::sleep(TEST_POLL);
        }
        Ok(())
    }
}

impl Drop for Playback {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

#[test]
fn catch_up_uses_current_position_even_between_ui_samples() -> Result<(), Box<dyn std::error::Error>>
{
    let mut playback = Playback::new(StartPosition::Beginning)?;
    playback.state(gst::State::Playing)?;
    playback.controller.start_change(
        playback.pipeline.upcast_ref(),
        USER_POSITION,
        Resume::Playing,
        Rate::checked(20).unwrap(),
        Completion::Position,
    )?;
    playback.finish_seek()?;
    let position = playback
        .pipeline
        .query_position::<gst::ClockTime>()
        .ok_or("position")?;
    let buffer = live_headroom(LiveBuffer::default());
    let range = Range::new(gst::ClockTime::ZERO, position + buffer / 2).ok_or("range")?;
    playback.controller.live_position = LivePosition::Behind;
    // Two-times playback moves 400 ms during the UI's 200 ms sample interval.
    let stale_by = gst::ClockTime::from_nseconds(POSITION_SAMPLE_INTERVAL.as_nanos() as u64) * 2;
    playback.controller.snapshot.position = Some(position - stale_by);
    playback.controller.next_sample = Instant::now() + POSITION_SAMPLE_INTERVAL;
    playback.controller.retained(
        playback.pipeline.upcast_ref(),
        LiveWindow::History(range),
        false,
        crate::playback::input::ReadProgress::idle(),
    )?;
    assert_eq!(
        playback.controller.requested_rate(),
        Rate::NORMAL,
        "the fresh playhead has reached the configured reserve"
    );
    playback.finish_seek()?;
    assert_eq!(playback.controller.speed().applied, Rate::NORMAL);
    assert_eq!(playback.controller.take_notice(), Some(Notice::CaughtUp));
    Ok(())
}

#[test]
fn confirmed_live_return_sets_live_status_despite_receive_progress_during_seek()
-> Result<(), Box<dyn std::error::Error>> {
    let mut playback = Playback::new(StartPosition::Beginning)?;
    playback.state(gst::State::Playing)?;
    playback.controller.live_position = LivePosition::Behind;
    let range = Range::new(gst::ClockTime::ZERO, RECEIVE_EDGE).ok_or("range")?;
    Ready {
        controller: &mut playback.controller,
        pipeline: playback.pipeline.upcast_ref(),
        range,
    }
    .return_to_live()?;
    assert!(
        !playback.controller.speed().at_live_edge,
        "a pending seek is not confirmation"
    );
    playback.finish_seek()?;
    let position = playback
        .pipeline
        .query_position::<gst::ClockTime>()
        .ok_or("position")?;
    let delivery_progress = gst::ClockTime::from_mseconds(100);
    let later = Range::new(
        gst::ClockTime::ZERO,
        position + live_headroom(LiveBuffer::default()) + delivery_progress,
    )
    .ok_or("range")?;
    playback.controller.retained(
        playback.pipeline.upcast_ref(),
        LiveWindow::History(later),
        false,
        crate::playback::input::ReadProgress::idle(),
    )?;
    assert!(
        playback.controller.speed().at_live_edge,
        "confirmed live return must tolerate reception advancing during preroll"
    );
    assert!(!playback.controller.recovery_is_history());
    // A growing receive delay alone must not disable stall detection.
    playback.controller.observe_live(
        Range::new(
            gst::ClockTime::ZERO,
            later.end() + gst::ClockTime::from_seconds(20),
        )
        .unwrap(),
        true,
    );
    assert!(!playback.controller.recovery_is_history());
    Ready {
        controller: &mut playback.controller,
        pipeline: playback.pipeline.upcast_ref(),
        range: later,
    }
    .seek(USER_POSITION.mseconds() as f64)?;
    assert!(playback.controller.recovery_is_history());
    playback.finish_seek()?;
    playback.controller.retained(
        playback.pipeline.upcast_ref(),
        LiveWindow::History(later),
        false,
        crate::playback::input::ReadProgress::idle(),
    )?;
    assert!(
        !playback.controller.speed().at_live_edge,
        "rewinding must leave live status"
    );
    Ok(())
}

#[test]
fn rewind_supersedes_pending_live_return_without_publishing_live_status()
-> Result<(), Box<dyn std::error::Error>> {
    let mut playback = Playback::new(StartPosition::Beginning)?;
    playback.state(gst::State::Playing)?;
    playback.controller.live_position = LivePosition::Behind;
    let range = Range::new(gst::ClockTime::ZERO, RECEIVE_EDGE).ok_or("range")?;
    Ready {
        controller: &mut playback.controller,
        pipeline: playback.pipeline.upcast_ref(),
        range,
    }
    .return_to_live()?;
    Ready {
        controller: &mut playback.controller,
        pipeline: playback.pipeline.upcast_ref(),
        range,
    }
    .seek(USER_POSITION.mseconds() as f64)?;
    playback.finish_seek()?;
    assert!(
        !playback.controller.speed().at_live_edge,
        "superseded live confirmation is stale"
    );
    assert!(playback.controller.recovery_is_history());
    assert!(playback.controller.take_notice().is_none());
    Ok(())
}

#[test]
fn live_start_waits_for_preroll_and_reserve_then_aligns_only_once()
-> Result<(), Box<dyn std::error::Error>> {
    for (history, milliseconds) in [
        (false, LiveBuffer::DEFAULT_MS),
        (true, LiveBuffer::DEFAULT_MS),
        (false, LiveBuffer::MAX_MS),
        (true, LiveBuffer::MAX_MS),
    ] {
        let buffer = LiveBuffer::checked(milliseconds).unwrap();
        let mut playback = Playback::new(StartPosition::LiveEdge(buffer))?;
        assert_eq!(playback.controller.start().state(), gst::State::Paused);
        let short = Range::new(gst::ClockTime::ZERO, live_headroom(buffer) / 2).unwrap();
        playback
            .controller
            .align_live_start(playback.pipeline.upcast_ref(), short)?;
        assert!(matches!(
            playback.controller.startup,
            Startup::AwaitingLiveOutput
        ));
        playback.state(gst::State::Paused)?;
        playback
            .controller
            .align_live_start(playback.pipeline.upcast_ref(), short)?;
        assert_eq!(
            playback.pipeline.current_state(),
            gst::State::Paused,
            "insufficient reserve must wait"
        );
        let range = Range::new(gst::ClockTime::ZERO, RECEIVE_EDGE).unwrap();
        playback.controller.retained(
            playback.pipeline.upcast_ref(),
            if history {
                LiveWindow::History(range)
            } else {
                LiveWindow::ForwardBuffer(range)
            },
            false,
            crate::playback::input::ReadProgress::idle(),
        )?;
        playback.pipeline.state(TEST_TIMEOUT).0?;
        assert_eq!(playback.pipeline.current_state(), gst::State::Playing);
        assert!(matches!(playback.controller.startup, Startup::Complete));
        assert!(playback.controller.seek_target().is_none());
        assert_eq!(
            playback.pipeline.start_time(),
            Some(RECEIVE_EDGE - live_headroom(buffer))
        );
        let before = playback.pipeline.start_time();
        let later = Range::new(gst::ClockTime::ZERO, RECEIVE_EDGE * 2).unwrap();
        assert!(
            !playback
                .controller
                .align_live_start(playback.pipeline.upcast_ref(), later)?
        );
        assert_eq!(
            playback.pipeline.start_time(),
            before,
            "ordinary viewing must not realign"
        );
    }
    Ok(())
}

#[test]
fn live_return_and_forward_expiry_do_not_add_preroll_reception_to_the_reserve()
-> Result<(), Box<dyn std::error::Error>> {
    for expired in [false, true] {
        let mut playback = Playback::new(StartPosition::Beginning)?;
        playback.state(gst::State::Playing)?;
        let range = Range::new(gst::ClockTime::ZERO, RECEIVE_EDGE).unwrap();
        if expired {
            playback.controller.retained(
                playback.pipeline.upcast_ref(),
                LiveWindow::ForwardBuffer(range),
                true,
                crate::playback::input::ReadProgress::idle(),
            )?;
        } else {
            Ready {
                controller: &mut playback.controller,
                pipeline: playback.pipeline.upcast_ref(),
                range,
            }
            .return_to_live()?;
        }
        playback.state(gst::State::Paused)?;
        // Reception progresses while the decoder is prerolling. Resuming the
        // original seek target alone would leave two extra seconds of delay.
        let later = Range::new(
            gst::ClockTime::ZERO,
            RECEIVE_EDGE + gst::ClockTime::from_seconds(2),
        )
        .unwrap();
        playback.controller.retained(
            playback.pipeline.upcast_ref(),
            if expired {
                LiveWindow::ForwardBuffer(later)
            } else {
                LiveWindow::History(later)
            },
            false,
            crate::playback::input::ReadProgress::idle(),
        )?;
        playback.finish_seek()?;
        let target = later.live_target(LiveBuffer::default());
        let deadline = Instant::now() + Duration::from_millis(300);
        loop {
            let position = playback
                .pipeline
                .query_position::<gst::ClockTime>()
                .ok_or("position")?;
            if position.absdiff(target) < gst::ClockTime::from_mseconds(250) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "preroll increased live delay: {position}"
            );
            std::thread::sleep(TEST_POLL);
        }
        assert_eq!(playback.controller.phase(), Phase::Playing);
        assert!(playback.controller.seek_target().is_none());
    }
    Ok(())
}

#[test]
fn manual_pause_or_seek_before_initial_alignment_keeps_user_position()
-> Result<(), Box<dyn std::error::Error>> {
    enum Operation {
        Pause,
        Seek,
    }
    let range = Range::new(gst::ClockTime::ZERO, RECEIVE_EDGE).ok_or("range")?;
    for operation in [Operation::Pause, Operation::Seek] {
        let mut playback = Playback::new(StartPosition::LiveEdge(
            crate::settings::LiveBuffer::default(),
        ))?;
        playback.state(gst::State::Playing)?;
        match operation {
            Operation::Pause => {
                playback
                    .controller
                    .pause(playback.pipeline.upcast_ref(), Resume::Paused)?;
                playback.state(gst::State::Paused)?;
                assert!(
                    !playback
                        .controller
                        .align_live_start(playback.pipeline.upcast_ref(), range)?
                );
                assert_eq!(playback.controller.phase(), Phase::Paused);
                playback
                    .controller
                    .pause(playback.pipeline.upcast_ref(), Resume::Playing)?;
                playback.state(gst::State::Playing)?;
            }
            Operation::Seek => {
                Ready {
                    controller: &mut playback.controller,
                    pipeline: playback.pipeline.upcast_ref(),
                    range,
                }
                .seek(USER_POSITION.mseconds() as f64)?;
                playback.finish_seek()?;
            }
        }
        assert!(
            !playback
                .controller
                .align_live_start(playback.pipeline.upcast_ref(), range)?
        );
        assert!(playback.controller.seek_target().is_none());
        assert!(
            playback
                .pipeline
                .query_position::<gst::ClockTime>()
                .is_some_and(|p| p < RECEIVE_EDGE)
        );
    }
    Ok(())
}

#[test]
fn beginning_playback_is_not_advanced_to_the_receive_edge() -> Result<(), Box<dyn std::error::Error>>
{
    let mut playback = Playback::new(StartPosition::Beginning)?;
    playback.state(gst::State::Playing)?;
    let range = Range::new(gst::ClockTime::ZERO, RECEIVE_EDGE).ok_or("range")?;
    assert!(
        !playback
            .controller
            .align_live_start(playback.pipeline.upcast_ref(), range)?
    );
    assert!(playback.controller.seek_target().is_none());
    Ok(())
}

#[test]
fn live_start_does_not_require_seek_support() -> Result<(), Box<dyn std::error::Error>> {
    use std::sync::atomic::{AtomicUsize, Ordering};

    gst::init()?;
    // Live preroll can establish a reserve even when seeking is unsupported.
    let pipeline =
        gst::parse::launch("appsrc name=source format=time ! fakesink name=output sync=false")?
            .downcast::<gst::Pipeline>()
            .map_err(|_| "pipeline")?;
    let controller = Controller::new(
        &pipeline.by_name("output").ok_or("sink")?,
        StartPosition::LiveEdge(crate::settings::LiveBuffer::default()),
    )?;
    let mut playback = Playback {
        pipeline,
        controller,
    };
    let source = playback
        .pipeline
        .by_name("source")
        .ok_or("source")?
        .downcast::<gstreamer_app::AppSrc>()
        .map_err(|_| "appsrc")?;
    let mut buffer = gst::Buffer::new();
    buffer.make_mut().set_pts(gst::ClockTime::ZERO);
    buffer.make_mut().set_duration(USER_POSITION);
    source.push_buffer(buffer)?;
    playback.state(gst::State::Paused)?;
    assert!(
        playback
            .pipeline
            .query_position::<gst::ClockTime>()
            .is_some()
    );
    let attempts = Arc::new(AtomicUsize::new(0));
    let seen = attempts.clone();
    source.static_pad("src").ok_or("source pad")?.add_probe(
        gst::PadProbeType::EVENT_UPSTREAM,
        move |_, info| {
            if info
                .event()
                .is_some_and(|event| matches!(event.view(), gst::EventView::Seek(_)))
            {
                seen.fetch_add(1, Ordering::Relaxed);
            }
            gst::PadProbeReturn::Ok
        },
    );
    let range = Range::new(gst::ClockTime::ZERO, RECEIVE_EDGE).ok_or("range")?;
    assert!(
        playback
            .controller
            .align_live_start(playback.pipeline.upcast_ref(), range)?
    );
    assert_eq!(attempts.load(Ordering::Relaxed), 0);
    assert_eq!(playback.controller.phase(), Phase::Playing);
    assert!(playback.controller.seek_target().is_none());
    assert!(playback.controller.take_notice().is_none());
    assert!(
        !playback
            .controller
            .align_live_start(playback.pipeline.upcast_ref(), range)?
    );
    assert_eq!(attempts.load(Ordering::Relaxed), 0);
    Ok(())
}

#[test]
fn recording_network_wait_freezes_the_clock_and_preserves_user_pause_intent()
-> Result<(), Box<dyn std::error::Error>> {
    for pause_during_recovery in [false, true] {
        let mut playback = Playback::new(StartPosition::Beginning)?;
        playback.state(gst::State::Playing)?;
        playback
            .controller
            .recording_network(playback.pipeline.upcast_ref(), true)?;
        playback.pipeline.state(TEST_TIMEOUT).0?;
        let before = playback
            .pipeline
            .query_position::<gst::ClockTime>()
            .ok_or("position")?;
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(
            playback.pipeline.query_position::<gst::ClockTime>(),
            Some(before)
        );
        assert_eq!(playback.controller.phase(), Phase::Playing);
        if pause_during_recovery {
            playback
                .controller
                .pause(playback.pipeline.upcast_ref(), Resume::Paused)?;
        } else {
            playback
                .controller
                .pause(playback.pipeline.upcast_ref(), Resume::Playing)?;
            assert_eq!(playback.pipeline.current_state(), gst::State::Paused);
        }
        playback
            .controller
            .recording_network(playback.pipeline.upcast_ref(), false)?;
        playback.pipeline.state(TEST_TIMEOUT).0?;
        assert_eq!(
            playback.pipeline.current_state(),
            if pause_during_recovery {
                gst::State::Paused
            } else {
                gst::State::Playing
            }
        );
        assert!(playback.controller.seek_target().is_none());
    }
    Ok(())
}
