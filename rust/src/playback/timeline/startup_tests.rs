//! Explicit CPU-only streaming tests; no display, GPU or audio devices.
use super::*;

const TEST_TIMEOUT: gst::ClockTime = gst::ClockTime::from_seconds(5);
const TEST_POLL: Duration = Duration::from_millis(5);
const RECEIVE_EDGE: gst::ClockTime = gst::ClockTime::from_seconds(10);
const USER_POSITION: gst::ClockTime = gst::ClockTime::SECOND;

#[test]
fn live_alignment_keeps_audio_on_time_through_a_short_delivery_stall()
-> Result<(), Box<dyn std::error::Error>> {
    use gstreamer_app::{AppSrc, AppSrcCallbacks};

    const SAMPLE_RATE: u64 = 48_000;
    const SAMPLE_BYTES: u64 = 2;
    const PACKET: gst::ClockTime = gst::ClockTime::from_mseconds(20);
    const STALL_AT: gst::ClockTime = gst::ClockTime::SECOND;
    const STALL: gst::ClockTime = gst::ClockTime::from_mseconds(120);
    const AFTER_EDGE: gst::ClockTime = gst::ClockTime::from_seconds(2);
    enum Delivery {
        Preroll {
            next: gst::ClockTime,
        },
        Following {
            next: gst::ClockTime,
            started: Instant,
        },
    }

    gst::init()?;
    // Explicit CPU sink: count missed audio deadlines without opening a device.
    // The source has retained history, then delivers clock-paced live packets.
    let pipeline = gst::parse::launch(&format!(
        "appsrc name=source format=time stream-type=seekable ! audio/x-raw,format=S16LE,rate={SAMPLE_RATE},channels=1,layout=interleaved ! fakesink name=output sync=true",
    ))?
    .downcast::<gst::Pipeline>()
    .map_err(|_| "pipeline")?;
    let sink = pipeline.by_name("output").ok_or("sink")?;
    sink.set_property("max-lateness", PACKET.nseconds() as i64);
    let controller = Controller::new(
        &sink,
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
        .downcast::<AppSrc>()
        .map_err(|_| "appsrc")?;
    let delivery = Arc::new(Mutex::new(Delivery::Preroll {
        next: gst::ClockTime::ZERO,
    }));
    let seeking = delivery.clone();
    source.set_callbacks(
        AppSrcCallbacks::builder()
            .seek_data(move |_, target| {
                let next = gst::ClockTime::from_nseconds(target);
                *seeking.lock().unwrap() = if target == 0 {
                    Delivery::Preroll { next }
                } else {
                    Delivery::Following {
                        next,
                        started: Instant::now(),
                    }
                };
                true
            })
            .need_data(move |source, _| {
                let (pts, arrival) = {
                    let mut delivery = delivery.lock().unwrap();
                    let (next, started) = match &mut *delivery {
                        Delivery::Preroll { next } => (next, None),
                        Delivery::Following { next, started } => (next, Some(*started)),
                    };
                    let pts = *next;
                    *next += PACKET;
                    let arrival = started.map(|started| {
                        let since_edge = pts.saturating_sub(RECEIVE_EDGE);
                        // One delayed delivery, followed by a catch-up burst; no
                        // missing samples or timestamp discontinuity in the source.
                        let arrival = if (STALL_AT..STALL_AT + STALL).contains(&since_edge) {
                            STALL_AT + STALL
                        } else {
                            since_edge
                        };
                        started + Duration::from_nanos(arrival.nseconds())
                    });
                    (pts, arrival)
                };
                if pts >= RECEIVE_EDGE + AFTER_EDGE {
                    let _ = source.end_of_stream();
                    return;
                }
                if let Some(arrival) = arrival {
                    std::thread::sleep(arrival.saturating_duration_since(Instant::now()));
                }
                let bytes = (SAMPLE_RATE * SAMPLE_BYTES * PACKET.nseconds()
                    / gst::ClockTime::SECOND.nseconds()) as usize;
                let mut buffer = gst::Buffer::from_mut_slice(vec![0; bytes]);
                buffer.make_mut().set_pts(pts);
                buffer.make_mut().set_duration(PACKET);
                let _ = source.push_buffer(buffer);
            })
            .build(),
    );

    playback.state(gst::State::Playing)?;
    let range = Range::new(gst::ClockTime::ZERO, RECEIVE_EDGE).ok_or("range")?;
    assert!(
        playback
            .controller
            .align_live_start(playback.pipeline.upcast_ref(), range)?
    );
    playback.finish_seek()?;
    let message = playback
        .pipeline
        .bus()
        .ok_or("bus")?
        .timed_pop_filtered(
            TEST_TIMEOUT,
            &[gst::MessageType::Error, gst::MessageType::Eos],
        )
        .ok_or("live audio did not finish")?;
    assert_eq!(message.type_(), gst::MessageType::Eos, "{message:?}");
    let counts = crate::playback::stats::frame_counters(&sink).ok_or("audio counters")?;
    assert!(counts.rendered + counts.dropped >= AFTER_EDGE.nseconds() / PACKET.nseconds());
    assert_eq!(
        counts.dropped, 0,
        "audio packets missed their playback deadlines"
    );
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
    Ready {
        controller: &mut playback.controller,
        pipeline: playback.pipeline.upcast_ref(),
        range: later,
    }
    .seek(USER_POSITION.mseconds() as f64)?;
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
    assert!(playback.controller.take_notice().is_none());
    Ok(())
}

#[test]
fn initial_alignment_waits_for_rendering_after_decoded_preroll()
-> Result<(), Box<dyn std::error::Error>> {
    let mut playback = Playback::new(StartPosition::LiveEdge(
        crate::settings::LiveBuffer::default(),
    ))?;
    // A future first PTS puts the graph in PLAYING with decoded preroll but no
    // rendered frames. No wall-clock sleep is needed to reproduce this ordering.
    playback
        .pipeline
        .by_name("source")
        .ok_or("source")?
        .set_property("timestamp-offset", RECEIVE_EDGE.nseconds() as i64);
    playback.pipeline.set_state(gst::State::Playing)?;
    playback.pipeline.state(TEST_TIMEOUT).0?;
    assert!(
        playback
            .controller
            .output
            .lock()
            .unwrap()
            .buffered
            .is_some()
    );
    let range = Range::new(gst::ClockTime::ZERO, RECEIVE_EDGE).ok_or("range")?;
    assert!(
        !playback
            .controller
            .align_live_start(playback.pipeline.upcast_ref(), range)?
    );
    assert!(matches!(
        playback.controller.startup,
        Startup::AwaitingLiveOutput
    ));
    assert!(playback.controller.seek_target().is_none());
    Ok(())
}

#[test]
fn live_start_waits_for_output_and_aligns_only_once_per_source()
-> Result<(), Box<dyn std::error::Error>> {
    let range = Range::new(gst::ClockTime::ZERO, RECEIVE_EDGE).ok_or("range")?;
    // A replacement source owns a fresh controller. Both user-visible history
    // and the small forward buffer permit this private startup operation.
    for window in [LiveWindow::History(range), LiveWindow::ForwardBuffer(range)] {
        let mut playback = Playback::new(StartPosition::LiveEdge(
            crate::settings::LiveBuffer::default(),
        ))?;
        playback
            .controller
            .align_live_start(playback.pipeline.upcast_ref(), range)?;
        assert!(playback.controller.seek_target().is_none());
        playback.state(gst::State::Paused)?;
        assert!(
            playback
                .controller
                .output
                .lock()
                .unwrap()
                .buffered
                .is_some()
        );
        assert!(
            !playback
                .controller
                .align_live_start(playback.pipeline.upcast_ref(), range)?
        );
        playback.state(gst::State::Playing)?;
        playback.controller.retained(
            playback.pipeline.upcast_ref(),
            window,
            false,
            crate::playback::input::ReadProgress::idle(),
        )?;
        assert_eq!(
            playback.controller.seek_target(),
            Some(range.live_target(LiveBuffer::default()))
        );
        playback.finish_seek()?;
        assert_eq!(playback.controller.phase(), Phase::Playing);
        assert!(
            playback
                .controller
                .snapshot()
                .position
                .is_some_and(|p| p >= range.live_target(LiveBuffer::default()))
        );
        let later = Range::new(gst::ClockTime::ZERO, RECEIVE_EDGE * 2).ok_or("range")?;
        assert!(
            !playback
                .controller
                .align_live_start(playback.pipeline.upcast_ref(), later)?
        );
        assert!(playback.controller.seek_target().is_none());
        assert!(playback.controller.take_notice().is_none());
    }
    Ok(())
}

#[test]
fn startup_already_inside_the_live_reserve_does_not_seek_backwards()
-> Result<(), Box<dyn std::error::Error>> {
    let mut playback = Playback::new(StartPosition::LiveEdge(
        crate::settings::LiveBuffer::default(),
    ))?;
    playback.state(gst::State::Playing)?;
    let position = playback
        .pipeline
        .query_position::<gst::ClockTime>()
        .ok_or("position")?;
    let range = Range::new(
        gst::ClockTime::ZERO,
        position + live_headroom(LiveBuffer::default()) / 2,
    )
    .ok_or("range")?;
    assert!(
        !playback
            .controller
            .align_live_start(playback.pipeline.upcast_ref(), range)?
    );
    assert!(matches!(playback.controller.startup, Startup::Complete));
    assert!(playback.controller.seek_target().is_none());
    let later = Range::new(gst::ClockTime::ZERO, RECEIVE_EDGE).ok_or("range")?;
    assert!(
        !playback
            .controller
            .align_live_start(playback.pipeline.upcast_ref(), later)?
    );
    assert_eq!(playback.controller.phase(), Phase::Playing);
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
fn rejected_initial_seek_keeps_playback_and_is_not_retried()
-> Result<(), Box<dyn std::error::Error>> {
    use std::sync::atomic::{AtomicUsize, Ordering};

    gst::init()?;
    // A streaming appsrc accepts buffers but rejects seeks. Exercise the native
    // rejection boundary without substituting any hardware-dependent output.
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
    playback.state(gst::State::Playing)?;
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
        !playback
            .controller
            .align_live_start(playback.pipeline.upcast_ref(), range)?
    );
    assert_eq!(attempts.load(Ordering::Relaxed), 1);
    assert_eq!(playback.controller.phase(), Phase::Playing);
    assert!(playback.controller.seek_target().is_none());
    assert!(playback.controller.take_notice().is_none());
    assert!(
        !playback
            .controller
            .align_live_start(playback.pipeline.upcast_ref(), range)?
    );
    assert_eq!(attempts.load(Ordering::Relaxed), 1);
    Ok(())
}
