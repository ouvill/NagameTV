//! Time-based transport for recordings and retained live streams. A prepared operation borrows the same controller
//! and pipeline whose TIME seeking capability was checked.
use gstreamer::{self as gst, prelude::*};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const POSITION_SAMPLE_INTERVAL: Duration = Duration::from_millis(200);
const SEEK_TIMEOUT: Duration = Duration::from_secs(10);
const RECOVERY_HEADROOM: gst::ClockTime = gst::ClockTime::from_seconds(2);
const EXPIRED_NOTICE: &str = "保持期限を過ぎたため、再生位置を余裕のある保持範囲内へ移動しました";

/// Forward buffering does not constrain frames already queued by the decoder.
/// Only retained history is a user-visible playback window.
pub(super) enum LiveWindow {
    ForwardBuffer(Range),
    History(Range),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resume {
    Playing,
    Paused,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Playing,
    Paused,
    Seeking(Resume),
    Ended,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub position: Option<gst::ClockTime>,
    pub duration: Option<gst::ClockTime>,
    pub estimated: bool,
    pub range: Option<Range>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Range {
    start: gst::ClockTime,
    end: gst::ClockTime,
}

impl Range {
    pub(super) fn new(start: gst::ClockTime, end: gst::ClockTime) -> Option<Self> {
        (end > start).then_some(Self { start, end })
    }
    pub(super) fn recovery_target(self) -> gst::ClockTime {
        let preroll = gst::ClockTime::from_nseconds(super::input::SEEK_PREROLL.as_nanos() as u64);
        self.start + (preroll + RECOVERY_HEADROOM).min((self.end - self.start) / 2)
    }
    pub fn start(self) -> gst::ClockTime {
        self.start
    }
    pub fn end(self) -> gst::ClockTime {
        self.end
    }
    fn target(self, milliseconds: f64) -> Result<gst::ClockTime, Error> {
        if !milliseconds.is_finite() || milliseconds < 0.0 {
            return Err(Error::InvalidPosition);
        }
        // QML uses milliseconds. Clamp before conversion, including very large
        // finite input; seek just inside EOS so there is a frame to preroll.
        let end = self
            .end
            .saturating_sub(gst::ClockTime::MSECOND)
            .max(self.start);
        let ns = (milliseconds.min(end.mseconds() as f64) as u64) * 1_000_000;
        Ok(gst::ClockTime::from_nseconds(ns).clamp(self.start, end))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("シークをまだ利用できません")]
    Unavailable,
    #[error("再生位置が不正です")]
    InvalidPosition,
    #[error("シーク要求が拒否されました")]
    Rejected,
    #[error("シークの完了を確認できませんでした")]
    TimedOut,
    #[error("再生位置の監視に失敗しました")]
    Observer,
    #[error("再生状態の変更に失敗しました: {0}")]
    State(#[from] gst::StateChangeError),
}

#[derive(Default)]
struct Output {
    segment: Option<gst::Seqnum>,
    buffered: Option<gst::Seqnum>,
    eos: Option<gst::Seqnum>,
}

struct Seek {
    sequence: gst::Seqnum,
    target: gst::ClockTime,
    resume: Resume,
    next: Option<gst::ClockTime>,
    deadline: Instant,
}

enum State {
    Playing,
    Paused,
    ExpiredPause,
    Seeking(Seek),
    Ended,
}

pub(super) struct Controller {
    state: State,
    snapshot: Snapshot,
    output: Arc<Mutex<Output>>,
    subscriptions: crate::features::subscriptions::Subscriptions,
    next_sample: Instant,
    notice: Option<&'static str>,
}

/// A short exclusive borrow, never an interchangeable capability token.
pub(super) struct Ready<'a> {
    controller: &'a mut Controller,
    pipeline: &'a gst::Element,
    range: Range,
}

impl Controller {
    pub fn new(sink: &gst::Element) -> Result<Self, Error> {
        let pad = sink.static_pad("sink").ok_or(Error::Observer)?;
        let output = Arc::new(Mutex::new(Output::default()));
        let observed = output.clone();
        let subscriptions = crate::features::subscriptions::Subscriptions::default();
        let probe = pad.add_probe(
            gst::PadProbeType::EVENT_DOWNSTREAM
                | gst::PadProbeType::EVENT_FLUSH
                | gst::PadProbeType::BUFFER
                | gst::PadProbeType::BUFFER_LIST,
            move |_, info| {
                if let Ok(mut state) = observed.lock() {
                    if let Some(event) = info.event() {
                        match event.view() {
                            gst::EventView::FlushStart(_) | gst::EventView::FlushStop(_) => {
                                *state = Output::default();
                            }
                            gst::EventView::Eos(_) => state.eos = Some(event.seqnum()),
                            gst::EventView::Segment(_) => {
                                state.eos = None;
                                state.segment = Some(event.seqnum());
                                state.buffered = None;
                            }
                            _ => {}
                        }
                    }
                    if info.buffer().is_some() || info.buffer_list().is_some() {
                        state.buffered = state.segment;
                    }
                }
                gst::PadProbeReturn::Ok
            },
        );
        if probe.is_none() {
            return Err(Error::Observer);
        }
        subscriptions.probe(&pad, probe);
        Ok(Self {
            state: State::Playing,
            snapshot: Snapshot::default(),
            output,
            subscriptions,
            next_sample: Instant::now(),
            notice: None,
        })
    }

    pub fn phase(&self) -> Phase {
        match &self.state {
            State::Playing => Phase::Playing,
            State::Paused | State::ExpiredPause => Phase::Paused,
            State::Seeking(seek) => Phase::Seeking(seek.resume),
            State::Ended => Phase::Ended,
        }
    }

    pub fn take_notice(&mut self) -> Option<&'static str> {
        self.notice.take()
    }

    pub fn set_estimated(&mut self, estimated: bool) {
        self.snapshot.estimated = estimated;
    }

    pub fn seek_target(&self) -> Option<gst::ClockTime> {
        match &self.state {
            State::Seeking(seek) => Some(seek.next.unwrap_or(seek.target)),
            State::Playing | State::Paused | State::ExpiredPause | State::Ended => None,
        }
    }
    pub fn snapshot(&self) -> Snapshot {
        self.snapshot
    }

    fn sample(&mut self, pipeline: &gst::Element) {
        self.next_sample = Instant::now() + POSITION_SAMPLE_INTERVAL;
        let duration = pipeline.query_duration::<gst::ClockTime>();
        let mut query = gst::query::Seeking::new(gst::Format::Time);
        let range = if pipeline.query(&mut query) {
            match query.result() {
                (
                    true,
                    gst::GenericFormattedValue::Time(Some(start)),
                    gst::GenericFormattedValue::Time(end),
                ) => end
                    .or(duration)
                    .filter(|end| *end > start)
                    .map(|end| Range { start, end }),
                _ => None,
            }
        } else {
            None
        };
        self.snapshot = Snapshot {
            position: pipeline.query_position::<gst::ClockTime>(),
            duration,
            estimated: self.snapshot.estimated,
            range,
        };
    }

    pub fn retained(
        &mut self,
        pipeline: &gst::Element,
        window: LiveWindow,
        expired: bool,
    ) -> Result<(), Error> {
        let (range, history) = match window {
            LiveWindow::ForwardBuffer(range) => (range, None),
            LiveWindow::History(range) => (range, Some(range)),
        };
        self.snapshot.range = history;
        self.snapshot.duration = history.map(|range| range.end);
        let behind = history.is_some()
            && self
                .snapshot
                .position
                .is_some_and(|position| position < range.start);
        if matches!(self.state, State::Paused | State::ExpiredPause) && (expired || behind) {
            // Keep the paused frame. Do not keep decoding/seek at every eviction;
            // resume will flush the old queue and choose a safe interior point.
            self.state = State::ExpiredPause;
            return Ok(());
        }
        if expired || (behind && !matches!(self.state, State::Seeking(_))) {
            let recovery = range.recovery_target();
            let (target, resume) = match &self.state {
                State::Paused | State::ExpiredPause => (recovery, Resume::Paused),
                State::Playing | State::Ended => (recovery, Resume::Playing),
                State::Seeking(seek) => {
                    (seek.next.unwrap_or(seek.target).max(recovery), seek.resume)
                }
            };
            self.start_seek(pipeline, target, resume)?;
            if history.is_some() {
                self.notice = Some(EXPIRED_NOTICE);
            }
            tracing::info!(
                start = range.start.mseconds(),
                target = target.mseconds(),
                "TS cursor expired; recovering inside available history"
            );
        }
        Ok(())
    }

    pub fn prepare<'a>(&'a mut self, pipeline: &'a gst::Element) -> Result<Ready<'a>, Error> {
        let (_, state, _) = pipeline.state(gst::ClockTime::ZERO);
        if state < gst::State::Paused {
            return Err(Error::Unavailable);
        }
        self.sample(pipeline);
        let range = self.snapshot.range.ok_or(Error::Unavailable)?;
        Ok(Ready {
            controller: self,
            pipeline,
            range,
        })
    }

    pub fn pause(&mut self, pipeline: &gst::Element, resume: Resume) -> Result<(), Error> {
        if matches!(self.state, State::Ended) {
            return Err(Error::Unavailable);
        }
        if matches!(self.state, State::ExpiredPause) {
            if resume == Resume::Playing {
                self.sample(pipeline);
                let target = self
                    .snapshot
                    .range
                    .ok_or(Error::Unavailable)?
                    .recovery_target();
                self.start_seek(pipeline, target, resume)?;
                self.notice = Some(EXPIRED_NOTICE);
            }
            return Ok(());
        }
        pipeline.set_state(match resume {
            Resume::Playing => gst::State::Playing,
            Resume::Paused => gst::State::Paused,
        })?;
        match &mut self.state {
            State::Seeking(seek) => seek.resume = resume,
            State::Playing | State::Paused => {
                self.state = match resume {
                    Resume::Playing => State::Playing,
                    Resume::Paused => State::Paused,
                }
            }
            State::Ended | State::ExpiredPause => unreachable!("handled before state change"),
        }
        Ok(())
    }

    fn start_seek(
        &mut self,
        pipeline: &gst::Element,
        target: gst::ClockTime,
        resume: Resume,
    ) -> Result<(), Error> {
        let event = gst::event::Seek::new(
            1.0,
            // TS key-unit seeks can land several seconds before the requested
            // time. Decode preroll and clip to the requested stream time.
            gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE,
            gst::SeekType::Set,
            target,
            gst::SeekType::None,
            gst::ClockTime::NONE,
        );
        let sequence = event.seqnum();
        if !pipeline.send_event(event) {
            return Err(Error::Rejected);
        }
        self.state = State::Seeking(Seek {
            sequence,
            target,
            resume,
            next: None,
            deadline: Instant::now() + SEEK_TIMEOUT,
        });
        pipeline.set_state(match resume {
            Resume::Playing => gst::State::Playing,
            Resume::Paused => gst::State::Paused,
        })?;
        Ok(())
    }

    /// Only EOS belonging to the current output segment may finish a recording.
    pub fn ended(&mut self, sequence: gst::Seqnum) -> Result<bool, Error> {
        let output = self.output.lock().map_err(|_| Error::Observer)?;
        if output.segment != Some(sequence) {
            return Ok(false);
        }
        if let State::Seeking(seek) = &self.state
            && (seek.sequence != sequence || seek.next.is_some())
        {
            return Ok(false);
        }
        self.state = State::Ended;
        Ok(true)
    }

    pub fn poll(&mut self, pipeline: &gst::Element) -> Result<(), Error> {
        if let State::Seeking(seek) = &self.state {
            let (ready, eos) = {
                let output = self.output.lock().map_err(|_| Error::Observer)?;
                (
                    output.buffered == Some(seek.sequence),
                    output.eos == Some(seek.sequence),
                )
            };
            let (result, _, pending) = pipeline.state(gst::ClockTime::ZERO);
            if eos || (ready && result.is_ok() && pending == gst::State::VoidPending) {
                let resume = seek.resume;
                let next = seek.next;
                self.state = if eos && seek.next.is_none() {
                    State::Ended
                } else {
                    match resume {
                        Resume::Playing => State::Playing,
                        Resume::Paused => State::Paused,
                    }
                };
                if let Some(next) = next {
                    self.start_seek(pipeline, next, resume)?;
                }
                self.next_sample = Instant::now();
            } else if Instant::now() >= seek.deadline {
                return Err(Error::TimedOut);
            }
        }
        if Instant::now() >= self.next_sample {
            self.sample(pipeline);
        }
        Ok(())
    }

    pub fn relative_target(&self, delta_ms: f64) -> Result<f64, Error> {
        if !delta_ms.is_finite() {
            return Err(Error::InvalidPosition);
        }
        let position = match &self.state {
            State::Seeking(seek) => Some(seek.next.unwrap_or(seek.target)),
            State::Playing | State::Paused | State::ExpiredPause | State::Ended => {
                self.snapshot.position
            }
        }
        .ok_or(Error::Unavailable)?;
        Ok((position.mseconds() as f64 + delta_ms).max(0.0))
    }
}

impl Ready<'_> {
    pub fn seek_live(mut self, milliseconds: f64, corrected: bool) -> Result<(), Error> {
        let expired = milliseconds < self.range.start.mseconds() as f64;
        let target = if expired {
            self.range.recovery_target().mseconds() as f64
        } else {
            milliseconds
        };
        self.apply(target)?;
        if corrected || expired {
            self.controller.notice = Some(EXPIRED_NOTICE);
        }
        Ok(())
    }

    pub fn seek(mut self, milliseconds: f64) -> Result<(), Error> {
        self.apply(milliseconds)
    }
    fn apply(&mut self, milliseconds: f64) -> Result<(), Error> {
        let target = self.range.target(milliseconds)?;
        let resume = match &mut self.controller.state {
            State::Seeking(seek) => {
                seek.next = Some(target);
                return Ok(());
            }
            State::Playing | State::Ended => Resume::Playing,
            State::Paused | State::ExpiredPause => Resume::Paused,
        };
        self.controller.start_seek(self.pipeline, target, resume)
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        // Session drops this controller only after the pipeline stopped.
        self.subscriptions.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejected_live_seek_keeps_the_paused_frame_and_does_not_report_recovery() {
        gst::init().unwrap();
        let sink = gst::ElementFactory::make("fakesink").build().unwrap();
        let mut controller = Controller::new(&sink).unwrap();
        let paused = gst::ClockTime::from_seconds(1);
        let range = Range::new(
            gst::ClockTime::from_seconds(10),
            gst::ClockTime::from_seconds(20),
        )
        .unwrap();
        controller.state = State::ExpiredPause;
        controller.snapshot.position = Some(paused);
        controller.snapshot.range = Some(range);
        // The final native event can be rejected even after a capability check.
        // This CPU-only sink deterministically rejects it without opening output.
        let ready = Ready {
            controller: &mut controller,
            pipeline: &sink,
            range,
        };
        assert!(matches!(
            ready.seek_live(paused.mseconds() as f64, true),
            Err(Error::Rejected)
        ));
        assert_eq!(controller.phase(), Phase::Paused);
        assert_eq!(controller.snapshot().position, Some(paused));
        assert!(controller.seek_target().is_none());
        assert!(controller.take_notice().is_none());
    }
    #[test]
    fn forward_buffer_eviction_does_not_expire_queued_video() {
        gst::init().unwrap();
        let sink = gst::ElementFactory::make("fakesink").build().unwrap();
        let mut controller = Controller::new(&sink).unwrap();
        controller.snapshot.position = Some(gst::ClockTime::ZERO);
        const BUFFER_SECONDS: u64 = 2;
        const EVICTIONS: u64 = 60;
        for start in 1..=EVICTIONS {
            controller
                .retained(
                    &sink,
                    LiveWindow::ForwardBuffer(
                        Range::new(
                            gst::ClockTime::from_seconds(start),
                            gst::ClockTime::from_seconds(start + BUFFER_SECONDS),
                        )
                        .unwrap(),
                    ),
                    false,
                )
                .unwrap();
            assert_eq!(controller.phase(), Phase::Playing);
            assert!(controller.snapshot().range.is_none());
            assert!(controller.take_notice().is_none());
        }
    }

    #[test]
    fn recovery_leaves_preroll_and_eviction_headroom_or_uses_short_window_midpoint() {
        let start = gst::ClockTime::from_seconds(30);
        let long = Range::new(start, start + gst::ClockTime::from_seconds(60)).unwrap();
        let preroll =
            gst::ClockTime::from_nseconds(super::super::input::SEEK_PREROLL.as_nanos() as u64);
        assert_eq!(long.recovery_target(), start + preroll + RECOVERY_HEADROOM);
        let short = Range::new(start, start + RECOVERY_HEADROOM).unwrap();
        assert_eq!(short.recovery_target(), start + RECOVERY_HEADROOM / 2);
    }
    #[test]
    fn positions_are_validated_before_conversion_and_clamped_inside_eos() {
        let range = Range {
            start: gst::ClockTime::ZERO,
            end: gst::ClockTime::from_seconds(60),
        };
        for value in [f64::NAN, f64::INFINITY, -1.0] {
            assert!(matches!(range.target(value), Err(Error::InvalidPosition)));
        }
        assert_eq!(
            range.target(f64::MAX).unwrap(),
            gst::ClockTime::from_mseconds(59_999)
        );
        assert_eq!(
            range.target(12_500.0).unwrap(),
            gst::ClockTime::from_mseconds(12_500)
        );
    }

    #[test]
    fn seeks_real_ts_in_memory_and_preserves_pause_and_latest_request()
    -> Result<(), Box<dyn std::error::Error>> {
        // Explicit CPU demux/decode test with an in-memory sink. No display,
        // GPU or audio device is constructed; this is not a playback fallback.
        gst::init()?;
        struct Pipeline(gst::Pipeline);
        impl Drop for Pipeline {
            fn drop(&mut self) {
                let _ = self.0.set_state(gst::State::Null);
            }
        }
        let graph = gst::parse::launch(
            "filesrc name=source ! tsdemux name=demux demux. ! queue ! mpegvideoparse ! avdec_mpeg2video ! appsink name=output max-buffers=1 drop=true sync=true"
        )?.downcast::<gst::Pipeline>().map_err(|_| "pipeline")?;
        let pipeline = Pipeline(graph);
        pipeline.0.by_name("source").ok_or("source")?.set_property(
            "location",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../tests/fixtures/recording-seek.ts"
            ),
        );
        let sink = pipeline.0.by_name("output").ok_or("output")?;
        let mut control = Controller::new(&sink)?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            control.pause(pipeline.0.upcast_ref(), Resume::Paused)?;
            pipeline.0.state(gst::ClockTime::from_seconds(5)).0?;
            control.poll(pipeline.0.upcast_ref())?;
            assert!(
                control
                    .snapshot()
                    .duration
                    .is_some_and(|time| time.seconds() >= 59)
            );
            for target in [30_000.0, 10_000.0, 45_000.0, 5_000.0] {
                control.prepare(pipeline.0.upcast_ref())?.seek(target)?;
                let deadline = Instant::now() + Duration::from_secs(5);
                while matches!(control.phase(), Phase::Seeking(_)) {
                    control.poll(pipeline.0.upcast_ref())?;
                    assert!(Instant::now() < deadline, "seek timed out at {target}");
                    std::thread::sleep(Duration::from_millis(5));
                }
                assert_eq!(control.phase(), Phase::Paused);
                let actual = control.snapshot().position.ok_or("position")?.mseconds() as f64;
                assert!((actual - target).abs() < 1500.0, "{target}: {actual}");
            }
            control.prepare(pipeline.0.upcast_ref())?.seek(20_000.0)?;
            control.prepare(pipeline.0.upcast_ref())?.seek(40_000.0)?;
            control.prepare(pipeline.0.upcast_ref())?.seek(15_000.0)?;
            let deadline = Instant::now() + Duration::from_secs(5);
            while matches!(control.phase(), Phase::Seeking(_)) {
                control.poll(pipeline.0.upcast_ref())?;
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(5));
            }
            let actual = control.snapshot().position.ok_or("position")?.mseconds();
            assert!(
                (13_500..=16_500).contains(&actual),
                "latest target: {actual}"
            );
            // A request beyond the duration is clamped inside EOS. Even if no
            // video frame remains there, EOS must complete the operation.
            control.prepare(pipeline.0.upcast_ref())?.seek(f64::MAX)?;
            let deadline = Instant::now() + Duration::from_secs(5);
            while matches!(control.phase(), Phase::Seeking(_)) {
                control.poll(pipeline.0.upcast_ref())?;
                assert!(Instant::now() < deadline, "end seek never completed");
                std::thread::sleep(Duration::from_millis(5));
            }
            control.prepare(pipeline.0.upcast_ref())?.seek(5_000.0)?;
            let deadline = Instant::now() + Duration::from_secs(5);
            while matches!(control.phase(), Phase::Seeking(_)) {
                control.poll(pipeline.0.upcast_ref())?;
                assert!(
                    Instant::now() < deadline,
                    "seek back from EOS never completed"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(!matches!(control.phase(), Phase::Ended));
            Ok(())
        })();
        // Match the production resource contract even after a test failure.
        pipeline.0.set_state(gst::State::Ready)?;
        drop(control);
        result
    }
}
