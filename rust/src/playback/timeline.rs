//! Time-based transport for recordings and retained live streams. A prepared
//! operation borrows its controller and pipeline with a validated TIME range:
//! a seeking query for user transport, or the active source's receive window.
use super::speed::{self, Availability, Rate};
mod supply;
use crate::settings::LiveBuffer;
use gstreamer::{self as gst, prelude::*};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const POSITION_SAMPLE_INTERVAL: Duration = Duration::from_millis(200);
const LIVE_RATE_HYSTERESIS: gst::ClockTime = gst::ClockTime::from_seconds(1);
const RECEPTION_STALL: Duration = Duration::from_secs(2);
const SEEK_TIMEOUT: Duration = Duration::from_secs(10);
const RECOVERY_HEADROOM: gst::ClockTime = gst::ClockTime::from_seconds(2);
// Decode preroll supplies reference frames before the seek target, not reserve
// data after it. Keep a small receive-to-playhead margin for live delivery jitter.
fn live_headroom(buffer: LiveBuffer) -> gst::ClockTime {
    gst::ClockTime::from_mseconds(buffer.milliseconds() as u64)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notice {
    Expired,
    SettingsClamped,
    SettingsReturnedToLive,
    CaughtUp,
    ReceptionStalled,
}

#[derive(Clone, Copy)]
pub(super) enum RetentionChange {
    ClampPosition,
    ReturnToLive,
}

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

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Arrival {
    None,
    Live,
}

pub(super) enum StartPosition {
    Beginning,
    LiveEdge(LiveBuffer),
}

enum Startup {
    AwaitingLiveOutput,
    Complete,
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
    fn live_target(self, buffer: LiveBuffer) -> gst::ClockTime {
        self.end
            .saturating_sub(live_headroom(buffer))
            .max(self.start)
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
    #[error("一時停止用の保持を開始できません: {0}")]
    Retention(#[from] super::input::Error),
    #[error("シークをまだ利用できません")]
    Unavailable,
    #[error("再生速度を変更できません")]
    RateUnavailable,
    #[error("再生速度は0.5〜2.0倍の0.1刻みで指定してください")]
    InvalidRate,
    #[error("再生位置が不正です")]
    InvalidPosition,
    #[error("シーク要求が拒否されました")]
    Rejected,
    #[error("再生速度の変更を確認できなかったため一時停止しました。再開時は等速に戻します")]
    RateTimedOut,
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
    rate: Option<f64>,
}

#[derive(Clone, Copy)]
enum Target {
    At(gst::ClockTime),
    Current,
}

#[derive(Clone, Copy)]
enum Completion {
    Position,
    Live,
    CaughtUp,
    ReceptionStalled,
}

#[derive(Clone, Copy)]
struct Pending {
    target: Target,
    rate: Rate,
    completion: Completion,
}

#[derive(Clone, Copy)]
enum LivePosition {
    Recording,
    ForwardBuffer,
    Near,
    Behind,
}

struct Seek {
    sequence: gst::Seqnum,
    target: gst::ClockTime,
    resume: Resume,
    next: Option<Pending>,
    rate: Rate,
    completion: Completion,
    deadline: Instant,
}

enum State {
    Playing,
    Paused,
    ExpiredPause,
    FailedPause,
    Seeking(Seek),
    Ended,
}

pub(super) struct Controller {
    state: State,
    startup: Startup,
    live_buffer: LiveBuffer,
    live_position: LivePosition,
    reception: Option<(gst::ClockTime, Instant)>,
    supply: supply::Supply,
    rate: Rate,
    sink: gst::glib::WeakRef<gst::Element>,
    snapshot: Snapshot,
    output: Arc<Mutex<Output>>,
    subscriptions: crate::features::subscriptions::Subscriptions,
    next_sample: Instant,
    notice: Option<Notice>,
    retention_change: Option<RetentionChange>,
}

/// A short exclusive borrow, never an interchangeable capability token.
pub(super) struct Ready<'a> {
    controller: &'a mut Controller,
    pipeline: &'a gst::Element,
    range: Range,
}

impl Controller {
    pub fn new(sink: &gst::Element, start: StartPosition) -> Result<Self, Error> {
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
                            gst::EventView::Segment(event) => {
                                state.rate =
                                    Some(event.segment().rate() * event.segment().applied_rate());
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
            startup: match start {
                StartPosition::Beginning => Startup::Complete,
                StartPosition::LiveEdge(_) => Startup::AwaitingLiveOutput,
            },
            live_buffer: match start {
                StartPosition::Beginning => LiveBuffer::default(),
                StartPosition::LiveEdge(buffer) => buffer,
            },
            live_position: match start {
                StartPosition::Beginning => LivePosition::Recording,
                StartPosition::LiveEdge(_) => LivePosition::ForwardBuffer,
            },
            reception: None,
            supply: supply::Supply::default(),
            rate: Rate::NORMAL,
            sink: sink.downgrade(),
            snapshot: Snapshot::default(),
            output,
            subscriptions,
            next_sample: Instant::now(),
            notice: None,
            retention_change: None,
        })
    }

    pub fn phase(&self) -> Phase {
        match &self.state {
            State::Playing => Phase::Playing,
            State::Paused | State::ExpiredPause | State::FailedPause => Phase::Paused,
            State::Seeking(seek) => Phase::Seeking(seek.resume),
            State::Ended => Phase::Ended,
        }
    }

    /// Update only future live targets; never seek or replace a pending target.
    pub fn configure_live_buffer(&mut self, buffer: LiveBuffer) {
        self.live_buffer = buffer;
    }

    pub fn take_notice(&mut self) -> Option<Notice> {
        self.notice.take()
    }

    pub fn set_estimated(&mut self, estimated: bool) {
        self.snapshot.estimated = estimated;
    }

    pub fn seek_target(&self) -> Option<gst::ClockTime> {
        match &self.state {
            State::Seeking(seek) => Some(
                seek.next
                    .and_then(|next| match next.target {
                        Target::At(target) => Some(target),
                        Target::Current => None,
                    })
                    .unwrap_or(seek.target),
            ),
            State::Playing
            | State::Paused
            | State::ExpiredPause
            | State::FailedPause
            | State::Ended => None,
        }
    }
    pub fn snapshot(&self) -> Snapshot {
        self.snapshot
    }

    pub fn requested_rate(&self) -> Rate {
        match &self.state {
            State::Seeking(seek) => seek.next.map_or(seek.rate, |next| next.rate),
            State::Playing
            | State::Paused
            | State::ExpiredPause
            | State::FailedPause
            | State::Ended => self.rate,
        }
    }

    pub fn speed(&self) -> speed::Snapshot {
        let availability = match self.state {
            State::Ended => Availability::Ended,
            State::ExpiredPause | State::FailedPause => Availability::Preparing,
            State::Playing | State::Paused | State::Seeking(_) => {
                if self.snapshot.range.is_none()
                    || (self.snapshot.position.is_none()
                        && !matches!(self.state, State::Seeking(_)))
                {
                    match self.live_position {
                        LivePosition::ForwardBuffer => Availability::LiveOnly,
                        LivePosition::Recording | LivePosition::Near | LivePosition::Behind => {
                            Availability::Preparing
                        }
                    }
                } else {
                    match self.live_position {
                        LivePosition::Recording | LivePosition::Behind => Availability::Variable,
                        LivePosition::Near | LivePosition::ForwardBuffer => Availability::LiveOnly,
                    }
                }
            }
        };
        speed::Snapshot {
            applied: self.rate,
            requested: self.requested_rate(),
            availability,
            at_live_edge: matches!(
                self.live_position,
                LivePosition::Near | LivePosition::ForwardBuffer
            ),
        }
    }

    fn observe_live(&mut self, range: Range, history: bool) {
        if self.reception.is_none_or(|(end, _)| end != range.end) {
            self.reception = Some((range.end, Instant::now()));
        }
        if !history {
            self.live_position = LivePosition::ForwardBuffer;
            return;
        }
        let Some(position) = self.snapshot.position else {
            return;
        };
        let delay = range.end.saturating_sub(position);
        // Catch up to the same configured reserve as an explicit live return.
        // Hysteresis only delays enabling variable speed again after catch-up.
        let threshold = live_headroom(self.live_buffer);
        self.live_position = match self.live_position {
            LivePosition::Behind if delay > threshold => LivePosition::Behind,
            LivePosition::Recording
            | LivePosition::ForwardBuffer
            | LivePosition::Near
            | LivePosition::Behind => {
                if delay >= threshold + LIVE_RATE_HYSTERESIS {
                    LivePosition::Behind
                } else {
                    LivePosition::Near
                }
            }
        };
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

    pub(super) fn retention_changed(&mut self, change: RetentionChange) {
        self.startup = Startup::Complete;
        self.retention_change = Some(match (self.retention_change, change) {
            (Some(RetentionChange::ReturnToLive), RetentionChange::ClampPosition)
            | (_, RetentionChange::ReturnToLive) => {
                self.snapshot.range = None;
                self.snapshot.duration = None;
                RetentionChange::ReturnToLive
            }
            (None | Some(RetentionChange::ClampPosition), RetentionChange::ClampPosition) => {
                RetentionChange::ClampPosition
            }
        });
    }

    pub fn retained(
        &mut self,
        pipeline: &gst::Element,
        window: LiveWindow,
        expired: bool,
        reader: super::input::ReadProgress,
    ) -> Result<(), Error> {
        let (range, history) = match window {
            LiveWindow::ForwardBuffer(range) => (range, None),
            LiveWindow::History(range) => (range, Some(range)),
        };
        self.snapshot.range = history;
        self.snapshot.duration = history.map(|range| range.end);
        if matches!(self.state, State::Playing) {
            // The receive edge is fresh on every poll. Comparing it with the
            // 200 ms UI sample can leave fast playback permanently "behind".
            self.snapshot.position = pipeline.query_position::<gst::ClockTime>();
        }
        self.observe_live(range, history.is_some());
        let limited = match (self.phase(), self.snapshot.position) {
            (Phase::Playing, Some(position)) => {
                self.supply
                    .limited(self.rate, position, reader, Instant::now())
            }
            _ => {
                self.supply.reset();
                false
            }
        };
        if let Some(change) = self.retention_change {
            if pipeline.state(gst::ClockTime::ZERO).1 < gst::State::Paused {
                return Ok(());
            }
            self.sample(pipeline);
            self.snapshot.range = history;
            self.snapshot.duration = history.map(|range| range.end);
            let position = self.seek_target().or(self.snapshot.position);
            let correction = match change {
                RetentionChange::ReturnToLive => {
                    Some((range.live_target(self.live_buffer), Resume::Playing))
                }
                RetentionChange::ClampPosition => {
                    if expired || position.is_some_and(|position| position < range.start) {
                        let resume = match self.phase() {
                            Phase::Paused | Phase::Seeking(Resume::Paused) => Resume::Paused,
                            Phase::Playing | Phase::Ended | Phase::Seeking(Resume::Playing) => {
                                Resume::Playing
                            }
                        };
                        Some((range.recovery_target(), resume))
                    } else {
                        None
                    }
                }
            };
            if let Some((target, resume)) = correction {
                let completion = match change {
                    RetentionChange::ClampPosition => Completion::Position,
                    RetentionChange::ReturnToLive => Completion::Live,
                };
                self.start_change(pipeline, target, resume, Rate::NORMAL, completion)?;
                self.notice = Some(match change {
                    RetentionChange::ClampPosition => Notice::SettingsClamped,
                    RetentionChange::ReturnToLive => Notice::SettingsReturnedToLive,
                });
            }
            self.retention_change = None;
            return Ok(());
        }
        if self.align_live_start(pipeline, range)? {
            return Ok(());
        }
        let behind = history.is_some()
            && self
                .snapshot
                .position
                .is_some_and(|position| position < range.start);
        if matches!(
            self.state,
            State::Paused | State::ExpiredPause | State::FailedPause
        ) && (expired || behind)
        {
            // Keep the paused frame. Do not keep decoding/seek at every eviction;
            // resume will flush the old queue and choose a safe interior point.
            self.state = State::ExpiredPause;
            return Ok(());
        }
        if expired || (behind && !matches!(self.state, State::Seeking(_))) {
            let recovery = range.recovery_target();
            let (target, resume) = match &self.state {
                State::Paused | State::ExpiredPause | State::FailedPause => {
                    (recovery, Resume::Paused)
                }
                State::Playing | State::Ended => (recovery, Resume::Playing),
                State::Seeking(seek) => (
                    seek.next
                        .and_then(|next| match next.target {
                            Target::At(target) => Some(target),
                            Target::Current => None,
                        })
                        .unwrap_or(seek.target)
                        .max(recovery),
                    seek.resume,
                ),
            };
            self.start_change(pipeline, target, resume, Rate::NORMAL, Completion::Position)?;
            if history.is_some() {
                self.notice = Some(Notice::Expired);
            }
            tracing::info!(
                start = range.start.mseconds(),
                target = target.mseconds(),
                "TS cursor expired; recovering inside available history"
            );
        }
        if matches!(self.state, State::Playing) && self.rate.faster() {
            let stalled = self
                .reception
                .is_some_and(|(_, at)| at.elapsed() >= RECEPTION_STALL);
            if (stalled
                || limited
                || matches!(
                    self.live_position,
                    LivePosition::Near | LivePosition::ForwardBuffer
                ))
                && let Some(position) = pipeline.query_position::<gst::ClockTime>()
            {
                tracing::info!(
                    rate = self.rate.multiplier(),
                    delay_ms = range.end.saturating_sub(position).mseconds(),
                    reserve_ms = self.live_buffer.milliseconds(),
                    limited,
                    stalled,
                    "Returning live playback to normal speed"
                );
                self.start_change(
                    pipeline,
                    position,
                    Resume::Playing,
                    Rate::NORMAL,
                    if stalled {
                        Completion::ReceptionStalled
                    } else {
                        Completion::CaughtUp
                    },
                )?;
            }
        }
        Ok(())
    }

    fn align_live_start(&mut self, pipeline: &gst::Element, range: Range) -> Result<bool, Error> {
        if !matches!(self.startup, Startup::AwaitingLiveOutput)
            || !matches!(self.state, State::Playing)
        {
            return Ok(false);
        }
        // Wait for this session's first decoded output and completed startup.
        // A PLAYING request alone does not mean a flushing seek is ready.
        if !matches!(
            pipeline.state(gst::ClockTime::ZERO),
            (Ok(_), gst::State::Playing, gst::State::VoidPending)
        ) {
            return Ok(false);
        }
        {
            let output = self.output.lock().map_err(|_| Error::Observer)?;
            if output.buffered.is_none() || output.eos.is_some() {
                return Ok(false);
            }
        }
        // Preroll can deliver a decoded buffer before its first display time.
        // Wait until the sink has rendered in this stream, so the adjustment
        // does not race the initial clock wait and leave the same startup lag.
        if !self
            .sink
            .upgrade()
            .and_then(|sink| super::stats::frame_counters(&sink))
            .is_some_and(|counts| counts.rendered > 0)
        {
            return Ok(false);
        }
        let Some(position) = pipeline.query_position::<gst::ClockTime>() else {
            return Ok(false);
        };
        // Consume the startup adjustment even if native seeking is rejected.
        // Never keep jumping to live during ordinary viewing or user transport.
        self.startup = Startup::Complete;
        let target = range.live_target(self.live_buffer);
        if target <= position {
            return Ok(false);
        }
        // The receive window belongs to this active source, including its
        // bounded forward buffer when user-facing timeshift is disabled.
        let ready = Ready {
            controller: self,
            pipeline,
            range,
        };
        match ready.return_to_live() {
            Ok(()) => {
                tracing::info!(
                    position_ms = position.mseconds(),
                    target_ms = target.mseconds(),
                    "Aligning new live playback to the receive edge"
                );
                Ok(true)
            }
            Err(Error::Rejected) => {
                tracing::warn!("Initial live alignment rejected; keeping current playback");
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }

    pub fn prepare<'a>(&'a mut self, pipeline: &'a gst::Element) -> Result<Ready<'a>, Error> {
        let (_, state, _) = pipeline.state(gst::ClockTime::ZERO);
        if state < gst::State::Paused {
            return Err(Error::Unavailable);
        }
        self.sample(pipeline);
        let range = self.snapshot.range.ok_or(Error::Unavailable)?;
        if matches!(
            self.live_position,
            LivePosition::Near | LivePosition::Behind
        ) {
            self.observe_live(range, true);
        }
        Ok(Ready {
            controller: self,
            pipeline,
            range,
        })
    }

    pub fn pause(&mut self, pipeline: &gst::Element, resume: Resume) -> Result<(), Error> {
        self.supply.reset();
        if matches!(self.state, State::Ended) {
            return Err(Error::Unavailable);
        }
        if matches!(self.state, State::FailedPause) {
            if resume == Resume::Playing {
                let target = pipeline
                    .query_position::<gst::ClockTime>()
                    .or(self.snapshot.position)
                    .ok_or(Error::Unavailable)?;
                self.start_change(pipeline, target, resume, Rate::NORMAL, Completion::Position)?;
            }
            return Ok(());
        }
        if matches!(self.state, State::ExpiredPause) {
            if resume == Resume::Playing {
                self.sample(pipeline);
                let target = self
                    .snapshot
                    .range
                    .ok_or(Error::Unavailable)?
                    .recovery_target();
                self.start_change(pipeline, target, resume, Rate::NORMAL, Completion::Position)?;
                self.notice = Some(Notice::Expired);
            }
            self.startup = Startup::Complete;
            return Ok(());
        }
        if resume == Resume::Playing
            && self.requested_rate().faster()
            && matches!(
                self.live_position,
                LivePosition::Near | LivePosition::ForwardBuffer
            )
            && !matches!(self.state, State::Seeking(_))
        {
            let position = pipeline
                .query_position::<gst::ClockTime>()
                .ok_or(Error::Unavailable)?;
            self.start_change(
                pipeline,
                position,
                resume,
                Rate::NORMAL,
                Completion::CaughtUp,
            )?;
            return Ok(());
        }
        pipeline.set_state(match resume {
            Resume::Playing => gst::State::Playing,
            Resume::Paused => gst::State::Paused,
        })?;
        self.startup = Startup::Complete;
        match &mut self.state {
            State::Seeking(seek) => seek.resume = resume,
            State::Playing | State::Paused => {
                self.state = match resume {
                    Resume::Playing => State::Playing,
                    Resume::Paused => State::Paused,
                }
            }
            State::Ended | State::ExpiredPause | State::FailedPause => {
                unreachable!("handled before state change")
            }
        }
        if !matches!(self.state, State::Seeking(_)) {
            self.snapshot.position = pipeline
                .query_position::<gst::ClockTime>()
                .or(self.snapshot.position);
        }
        // Initial preroll can still be pending. Sample the range again on the
        // next poll instead of postponing its first valid publication.
        self.next_sample = Instant::now();
        Ok(())
    }

    fn start_change(
        &mut self,
        pipeline: &gst::Element,
        target: gst::ClockTime,
        resume: Resume,
        rate: Rate,
        completion: Completion,
    ) -> Result<(), Error> {
        self.supply.reset();
        let event = gst::event::Seek::new(
            rate.multiplier(),
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
            if rate != self.rate {
                let confirmed = self
                    .output
                    .lock()
                    .map_err(|_| Error::Observer)?
                    .rate
                    .and_then(Rate::from_multiplier)
                    == Some(self.rate);
                if !confirmed {
                    pipeline.set_state(gst::State::Paused)?;
                    self.state = State::FailedPause;
                }
            }
            return Err(Error::Rejected);
        }
        self.state = State::Seeking(Seek {
            sequence,
            target,
            resume,
            next: None,
            rate,
            completion,
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
        if let State::Seeking(seek) = &self.state {
            self.rate = seek.rate;
        }
        self.state = State::Ended;
        Ok(true)
    }

    pub fn poll(&mut self, pipeline: &gst::Element) -> Result<Arrival, Error> {
        let mut arrival = Arrival::None;
        if let State::Seeking(seek) = &self.state {
            let (ready, eos) = {
                let output = self.output.lock().map_err(|_| Error::Observer)?;
                (
                    output.buffered == Some(seek.sequence)
                        && output
                            .rate
                            .is_some_and(|rate| (rate - seek.rate.multiplier()).abs() < 0.000_001),
                    output.eos == Some(seek.sequence),
                )
            };
            let (result, _, pending) = pipeline.state(gst::ClockTime::ZERO);
            if eos || (ready && result.is_ok() && pending == gst::State::VoidPending) {
                let resume = seek.resume;
                let next = seek.next;
                self.rate = seek.rate;
                // A requested live target is not proof of arrival. Mark live
                // only for the confirmed segment, with no newer user intent.
                // The existing exit hysteresis absorbs reception during preroll.
                if next.is_none() {
                    let reached_live = match seek.completion {
                        Completion::Live => true,
                        Completion::CaughtUp => self.snapshot.range.is_some_and(|range| {
                            range.end.saturating_sub(seek.target)
                                <= live_headroom(self.live_buffer) + LIVE_RATE_HYSTERESIS
                        }),
                        Completion::Position | Completion::ReceptionStalled => false,
                    };
                    if !eos && resume == Resume::Playing && reached_live {
                        arrival = Arrival::Live;
                    }
                    match seek.completion {
                        Completion::Position => {}
                        Completion::Live => self.live_position = LivePosition::Near,
                        Completion::CaughtUp => {
                            self.live_position = LivePosition::Near;
                            self.notice = Some(Notice::CaughtUp);
                        }
                        Completion::ReceptionStalled => {
                            self.notice = Some(Notice::ReceptionStalled);
                        }
                    }
                }
                self.state = if eos && seek.next.is_none() {
                    State::Ended
                } else {
                    match resume {
                        Resume::Playing => State::Playing,
                        Resume::Paused => State::Paused,
                    }
                };
                if let Some(next) = next {
                    let target = match next.target {
                        Target::At(target) => target,
                        Target::Current => pipeline
                            .query_position::<gst::ClockTime>()
                            .ok_or(Error::Unavailable)?,
                    };
                    self.start_change(pipeline, target, resume, next.rate, next.completion)?;
                }
                self.next_sample = Instant::now();
            } else if Instant::now() >= seek.deadline {
                let changing_rate = seek.rate != self.rate;
                pipeline.set_state(gst::State::Paused)?;
                self.state = State::FailedPause;
                return Err(if changing_rate {
                    Error::RateTimedOut
                } else {
                    Error::TimedOut
                });
            }
        }
        if Instant::now() >= self.next_sample {
            self.sample(pipeline);
        }
        Ok(arrival)
    }

    pub fn relative_target(&self, delta_ms: f64) -> Result<f64, Error> {
        if !delta_ms.is_finite() {
            return Err(Error::InvalidPosition);
        }
        let position = match &self.state {
            State::Seeking(seek) => Some(
                seek.next
                    .and_then(|next| match next.target {
                        Target::At(target) => Some(target),
                        Target::Current => None,
                    })
                    .unwrap_or(seek.target),
            ),
            State::Playing
            | State::Paused
            | State::ExpiredPause
            | State::FailedPause
            | State::Ended => self.snapshot.position,
        }
        .ok_or(Error::Unavailable)?;
        Ok((position.mseconds() as f64 + delta_ms).max(0.0))
    }
}

impl Ready<'_> {
    pub fn return_to_live(self) -> Result<(), Error> {
        let target = self.range.live_target(self.controller.live_buffer);
        match &mut self.controller.state {
            State::Seeking(seek) => {
                self.pipeline.set_state(gst::State::Playing)?;
                seek.next = Some(Pending {
                    target: Target::At(target),
                    rate: Rate::NORMAL,
                    completion: Completion::Live,
                });
                seek.resume = Resume::Playing;
            }
            State::Playing
            | State::Paused
            | State::ExpiredPause
            | State::FailedPause
            | State::Ended => {
                self.controller.start_change(
                    self.pipeline,
                    target,
                    Resume::Playing,
                    Rate::NORMAL,
                    Completion::Live,
                )?;
            }
        }
        self.controller.startup = Startup::Complete;
        Ok(())
    }

    pub fn set_rate(self, rate: Rate) -> Result<(), Error> {
        let availability = self.controller.speed().availability;
        if availability != Availability::Variable
            && !(availability == Availability::LiveOnly && rate == Rate::NORMAL)
        {
            return Err(Error::RateUnavailable);
        }
        if self.controller.requested_rate() == rate {
            return Ok(());
        }
        match &mut self.controller.state {
            State::Seeking(seek) => {
                let target = seek.next.map_or(Target::Current, |next| next.target);
                seek.next = Some(Pending {
                    target,
                    rate,
                    completion: Completion::Position,
                });
            }
            State::Playing | State::Paused => {
                let resume = if matches!(self.controller.state, State::Paused) {
                    Resume::Paused
                } else {
                    Resume::Playing
                };
                let position = self
                    .pipeline
                    .query_position::<gst::ClockTime>()
                    .ok_or(Error::Unavailable)?;
                self.controller.start_change(
                    self.pipeline,
                    position,
                    resume,
                    rate,
                    Completion::Position,
                )?;
            }
            State::ExpiredPause | State::FailedPause | State::Ended => {
                return Err(Error::RateUnavailable);
            }
        }
        self.controller.startup = Startup::Complete;
        Ok(())
    }

    pub fn seek_live(mut self, milliseconds: f64, corrected: bool) -> Result<(), Error> {
        let expired = milliseconds < self.range.start.mseconds() as f64;
        let target = if expired {
            self.range.recovery_target().mseconds() as f64
        } else {
            milliseconds
        };
        self.apply(target)?;
        if corrected || expired {
            self.controller.notice = Some(Notice::Expired);
        }
        Ok(())
    }

    pub fn seek(mut self, milliseconds: f64) -> Result<(), Error> {
        self.apply(milliseconds)
    }
    fn apply(&mut self, milliseconds: f64) -> Result<(), Error> {
        let target = self.range.target(milliseconds)?;
        let rate = if matches!(self.controller.state, State::FailedPause) {
            Rate::NORMAL
        } else {
            self.controller.requested_rate()
        };
        let resume = match &mut self.controller.state {
            State::Seeking(seek) => {
                seek.next = Some(Pending {
                    target: Target::At(target),
                    rate,
                    completion: Completion::Position,
                });
                self.controller.startup = Startup::Complete;
                return Ok(());
            }
            State::Playing | State::Ended => Resume::Playing,
            State::Paused | State::ExpiredPause | State::FailedPause => Resume::Paused,
        };
        self.controller
            .start_change(self.pipeline, target, resume, rate, Completion::Position)?;
        self.controller.startup = Startup::Complete;
        Ok(())
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        // Session drops this controller only after the pipeline stopped.
        self.subscriptions.close();
    }
}

#[cfg(test)]
mod startup_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn live_target_leaves_delivery_headroom_within_the_available_window() {
        let start = gst::ClockTime::from_seconds(10);
        let end = gst::ClockTime::from_seconds(20);
        let range = Range::new(start, end).unwrap();
        assert_eq!(
            range.live_target(LiveBuffer::default()),
            end - live_headroom(LiveBuffer::default())
        );
        // A new or recently reset input may not yet have a full reserve.
        let short = Range::new(start, start + live_headroom(LiveBuffer::default()) / 2).unwrap();
        assert_eq!(short.live_target(LiveBuffer::default()), start);
        let initial = Range::new(
            gst::ClockTime::ZERO,
            live_headroom(LiveBuffer::default()) / 2,
        )
        .unwrap();
        assert_eq!(
            initial.live_target(LiveBuffer::default()),
            gst::ClockTime::ZERO
        );
        // Explicit history/file seeks retain their original endpoint semantics.
        assert_eq!(
            range.target(end.mseconds() as f64).unwrap(),
            end - gst::ClockTime::MSECOND
        );
    }
    #[test]
    fn rejected_live_seek_keeps_the_paused_frame_and_does_not_report_recovery() {
        gst::init().unwrap();
        let sink = gst::ElementFactory::make("fakesink").build().unwrap();
        let mut controller = Controller::new(&sink, StartPosition::Beginning).unwrap();
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
        let ready = Ready {
            controller: &mut controller,
            pipeline: &sink,
            range,
        };
        assert!(matches!(ready.return_to_live(), Err(Error::Rejected)));
        assert_eq!(controller.phase(), Phase::Paused);
        assert_eq!(controller.snapshot().position, Some(paused));
        assert!(controller.seek_target().is_none());
    }

    #[test]
    fn return_to_live_replaces_pending_rewind_and_resumes_after_the_active_seek()
    -> Result<(), Box<dyn std::error::Error>> {
        gst::init()?;
        let sink = gst::ElementFactory::make("fakesink")
            .property("async", false)
            .build()?;
        let mut controller = Controller::new(&sink, StartPosition::Beginning)?;
        let sequence = gst::Seqnum::next();
        controller.state = State::Seeking(Seek {
            sequence,
            target: gst::ClockTime::from_seconds(5),
            resume: Resume::Paused,
            next: Some(Pending {
                target: Target::At(gst::ClockTime::from_seconds(2)),
                rate: Rate::NORMAL,
                completion: Completion::Position,
            }),
            rate: Rate::NORMAL,
            completion: Completion::Position,
            deadline: Instant::now() + SEEK_TIMEOUT,
        });
        let latest_edge = gst::ClockTime::from_seconds(20);
        let buffer = LiveBuffer::checked(100).ok_or("live buffer")?;
        controller.configure_live_buffer(buffer);
        assert_eq!(
            controller.seek_target(),
            Some(gst::ClockTime::from_seconds(2))
        );
        assert_eq!(controller.phase(), Phase::Seeking(Resume::Paused));
        let range = Range::new(gst::ClockTime::ZERO, latest_edge).ok_or("range")?;
        let ready = Ready {
            controller: &mut controller,
            pipeline: &sink,
            range,
        };
        let result = ready.return_to_live();
        // A standalone CPU sink has no upstream source and opens no device.
        sink.set_state(gst::State::Null)?;
        result?;
        let State::Seeking(seek) = &controller.state else {
            panic!("the active seek still needs to finish");
        };
        assert_eq!(seek.sequence, sequence);
        assert_eq!(seek.resume, Resume::Playing);
        assert_eq!(
            controller.seek_target(),
            Some(latest_edge - live_headroom(buffer))
        );
        assert!(controller.take_notice().is_none());
        Ok(())
    }
    #[test]
    fn history_release_requires_confirmed_live_output_and_playing_intent()
    -> Result<(), Box<dyn std::error::Error>> {
        gst::init()?;
        // An explicit CPU-only sink exercises output generations and native
        // transition completion without opening a display or an audio device.
        let sink = gst::ElementFactory::make("fakesink")
            .property("async", false)
            .build()?;
        sink.set_state(gst::State::Playing)?;
        let edge = gst::ClockTime::from_seconds(20);
        let near = edge - live_headroom(LiveBuffer::default());
        let behind = gst::ClockTime::from_seconds(10);
        for (completion, resume, target, confirmed, releases) in [
            (Completion::Live, Resume::Playing, near, false, false),
            (Completion::Live, Resume::Playing, near, true, true),
            (Completion::Live, Resume::Paused, near, true, false),
            (Completion::Position, Resume::Playing, near, true, false),
            (Completion::CaughtUp, Resume::Playing, near, true, true),
            (Completion::CaughtUp, Resume::Playing, behind, true, false),
            (
                Completion::ReceptionStalled,
                Resume::Playing,
                near,
                true,
                false,
            ),
        ] {
            let mut control = Controller::new(&sink, StartPosition::Beginning)?;
            let sequence = gst::Seqnum::next();
            control.snapshot.range = Range::new(gst::ClockTime::ZERO, edge);
            control.next_sample = Instant::now() + POSITION_SAMPLE_INTERVAL;
            control.state = State::Seeking(Seek {
                sequence,
                target,
                resume,
                next: None,
                rate: Rate::NORMAL,
                completion,
                deadline: Instant::now() + SEEK_TIMEOUT,
            });
            {
                let mut output = control.output.lock().unwrap();
                output.buffered = Some(if confirmed {
                    sequence
                } else {
                    gst::Seqnum::next()
                });
                output.rate = Some(Rate::NORMAL.multiplier());
            }
            assert_eq!(control.poll(&sink)? == Arrival::Live, releases);
        }
        sink.set_state(gst::State::Null)?;
        Ok(())
    }
    #[test]
    fn forward_buffer_eviction_does_not_expire_queued_video() {
        gst::init().unwrap();
        let sink = gst::ElementFactory::make("fakesink").build().unwrap();
        let mut controller = Controller::new(&sink, StartPosition::Beginning).unwrap();
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
                    crate::playback::input::ReadProgress::idle(),
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
        let mut control = Controller::new(&sink, StartPosition::Beginning)?;
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

#[cfg(test)]
mod speed_tests {
    use super::*;

    #[test]
    fn live_rate_gate_has_hysteresis_and_uses_the_configured_reserve() {
        gst::init().unwrap();
        let sink = gst::ElementFactory::make("fakesink").build().unwrap();
        let mut control =
            Controller::new(&sink, StartPosition::LiveEdge(LiveBuffer::default())).unwrap();
        let position = gst::ClockTime::from_seconds(10);
        control.snapshot.position = Some(position);
        let update = |control: &mut Controller, delay| {
            let range = Range::new(
                gst::ClockTime::ZERO,
                position + gst::ClockTime::from_mseconds(delay),
            )
            .unwrap();
            control.snapshot.range = Some(range);
            control.observe_live(range, true);
            control.speed()
        };
        // Reconfigure the existing session, including reducing the reserve.
        for milliseconds in [
            LiveBuffer::MIN_MS,
            LiveBuffer::DEFAULT_MS,
            LiveBuffer::MAX_MS,
            LiveBuffer::MIN_MS,
        ] {
            let buffer = LiveBuffer::checked(milliseconds).unwrap();
            control.configure_live_buffer(buffer);
            let target_delay = milliseconds as u64;
            let enable_delay = target_delay + LIVE_RATE_HYSTERESIS.mseconds();
            assert_eq!(
                update(&mut control, enable_delay - 1).availability,
                Availability::LiveOnly
            );
            assert_eq!(
                update(&mut control, enable_delay).availability,
                Availability::Variable
            );
            let approaching = update(&mut control, target_delay + 1);
            assert_eq!(approaching.availability, Availability::Variable);
            assert!(
                !approaching.at_live_edge,
                "caught up before {milliseconds} ms"
            );
            let caught_up = update(&mut control, target_delay);
            assert_eq!(caught_up.availability, Availability::LiveOnly);
            assert!(caught_up.at_live_edge);
            assert_eq!(
                update(&mut control, target_delay + 1).availability,
                Availability::LiveOnly
            );
            assert!(update(&mut control, target_delay - 1).at_live_edge);
        }
    }

    #[test]
    fn old_or_wrong_rate_output_cannot_confirm_a_rate_change() {
        gst::init().unwrap();
        let sink = gst::ElementFactory::make("fakesink").build().unwrap();
        let mut control = Controller::new(&sink, StartPosition::Beginning).unwrap();
        let sequence = gst::Seqnum::next();
        control.state = State::Seeking(Seek {
            sequence,
            target: gst::ClockTime::SECOND,
            resume: Resume::Paused,
            rate: Rate::checked(15).unwrap(),
            next: None,
            completion: Completion::Position,
            deadline: Instant::now() + SEEK_TIMEOUT,
        });
        for (observed, rate) in [(gst::Seqnum::next(), 1.5), (sequence, 1.0)] {
            *control.output.lock().unwrap() = Output {
                segment: Some(observed),
                buffered: Some(observed),
                rate: Some(rate),
                eos: None,
            };
            control.poll(&sink).unwrap();
            assert!(matches!(control.phase(), Phase::Seeking(_)));
            assert_eq!(control.speed().applied, Rate::NORMAL);
        }
        if let State::Seeking(seek) = &mut control.state {
            seek.deadline = Instant::now();
        }
        assert!(matches!(control.poll(&sink), Err(Error::RateTimedOut)));
        assert_eq!(control.phase(), Phase::Paused);
        assert_eq!(control.speed().availability, Availability::Preparing);
        control.snapshot.position = Some(gst::ClockTime::SECOND);
        assert!(control.pause(&sink, Resume::Playing).is_err());
        assert_eq!(
            control.phase(),
            Phase::Paused,
            "unconfirmed recovery cannot resume"
        );
        sink.set_state(gst::State::Null).unwrap();
    }
}
