//! Explicit CPU-only streaming tests; no display, GPU or audio devices.
use super::*;

const TEST_TIMEOUT: gst::ClockTime = gst::ClockTime::from_seconds(5);
const TEST_POLL: Duration = Duration::from_millis(5);
const RECEIVE_EDGE: gst::ClockTime = gst::ClockTime::from_seconds(10);
const USER_POSITION: gst::ClockTime = gst::ClockTime::SECOND;

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
fn initial_alignment_waits_for_rendering_after_decoded_preroll()
-> Result<(), Box<dyn std::error::Error>> {
    let mut playback = Playback::new(StartPosition::LiveEdge)?;
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
        let mut playback = Playback::new(StartPosition::LiveEdge)?;
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
        playback
            .controller
            .retained(playback.pipeline.upcast_ref(), window, false)?;
        assert_eq!(playback.controller.seek_target(), Some(range.live_target()));
        playback.finish_seek()?;
        assert_eq!(playback.controller.phase(), Phase::Playing);
        assert!(
            playback
                .controller
                .snapshot()
                .position
                .is_some_and(|p| p >= range.live_target())
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
fn manual_pause_or_seek_before_initial_alignment_keeps_user_position()
-> Result<(), Box<dyn std::error::Error>> {
    enum Operation {
        Pause,
        Seek,
    }
    let range = Range::new(gst::ClockTime::ZERO, RECEIVE_EDGE).ok_or("range")?;
    for operation in [Operation::Pause, Operation::Seek] {
        let mut playback = Playback::new(StartPosition::LiveEdge)?;
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
        StartPosition::LiveEdge,
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
