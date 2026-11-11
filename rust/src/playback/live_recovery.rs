//! Bounded live-output recovery policy. Time and observations are supplied by the owner.
use std::time::{Duration, Instant};

pub const STALL_TIMEOUT: Duration = Duration::from_secs(8);
pub const INPUT_TIMEOUT: Duration = Duration::from_secs(2);
pub const RESTART_INTERVAL: Duration = Duration::from_secs(30);
pub const RECOVERY_TIMEOUT: Duration = Duration::from_secs(20);
pub const MAX_RESTARTS: u8 = 3;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    #[default]
    Idle,
    Recovering,
    Failed,
}
impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Recovering => "recovering",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Sample {
    pub generation: u64,
    pub received_bytes: u64,
    pub video_expected: bool,
    pub audio_expected: bool,
    pub video_input: u64,
    pub audio_input: u64,
    pub video_output: Option<u64>,
    pub audio_output: Option<u64>,
    pub input_time_ns: Option<u64>,
    pub output_time_ns: Option<u64>,
}

impl Sample {
    fn media_input(self) -> u64 {
        self.video_input.wrapping_add(self.audio_input)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Observation {
    /// User pause/seek/history or an inactive/non-live source cancels pending recovery.
    Suspended,
    /// Preroll and native buffering cannot establish a stall, but recovery has a deadline.
    Buffering,
    Live(Sample),
}

impl Observation {
    pub fn times(self) -> Option<(Option<u64>, Option<u64>)> {
        match self {
            Self::Live(sample) => Some((sample.input_time_ns, sample.output_time_ns)),
            Self::Suspended | Self::Buffering => None,
        }
    }
}

#[derive(Debug)]
pub enum Action {
    None,
    Restart(Restart),
    Failed,
}

/// Only this policy can authorize an automatic pipeline replacement. Not cloneable.
#[derive(Debug)]
pub struct Restart {
    attempt: u8,
}
impl Restart {
    pub fn attempt(&self) -> u8 {
        self.attempt
    }
}

struct Track {
    input: u64,
    output: Option<u64>,
    input_at: Instant,
    output_at: Instant,
}
impl Track {
    fn new(input: u64, output: Option<u64>, now: Instant) -> Self {
        Self {
            input,
            output,
            input_at: now,
            output_at: now,
        }
    }
    fn observe(&mut self, input: u64, expected: bool, output: Option<u64>, now: Instant) -> bool {
        if input != self.input {
            self.input = input;
            self.input_at = now;
        }
        if output != self.output {
            self.output = output;
            self.output_at = now;
        }
        // Empty/null-only reception and a removed ES cannot be an output stall.
        if !expected || input == 0 || now.saturating_duration_since(self.input_at) >= INPUT_TIMEOUT
        {
            self.output_at = now;
            return false;
        }
        now.saturating_duration_since(self.output_at) >= STALL_TIMEOUT
    }
}

struct Watching {
    generation: u64,
    video: Track,
    audio: Track,
}
impl Watching {
    fn new(sample: Sample, now: Instant) -> Self {
        Self {
            generation: sample.generation,
            video: Track::new(sample.media_input(), sample.video_output, now),
            audio: Track::new(sample.media_input(), sample.audio_output, now),
        }
    }
}

enum State {
    Watching(Option<Watching>),
    Waiting {
        due: Instant,
        baseline: Sample,
        watch: Watching,
    },
    Starting {
        deadline: Instant,
        baseline: Option<Sample>,
    },
    Failed,
}
impl Default for State {
    fn default() -> Self {
        Self::Watching(None)
    }
}

#[derive(Default)]
pub struct Recovery {
    state: State,
    attempts: u8,
    last_restart: Option<Instant>,
}
impl Recovery {
    pub fn status(&self) -> Status {
        match self.state {
            State::Watching(_) => Status::Idle,
            State::Waiting { .. } | State::Starting { .. } => Status::Recovering,
            State::Failed => Status::Failed,
        }
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn fail(&mut self) {
        self.state = State::Failed;
    }
    pub fn observe(&mut self, now: Instant, observation: Observation) -> Action {
        if matches!(self.state, State::Failed) {
            return Action::None;
        }
        if matches!(observation, Observation::Suspended) {
            self.state = State::Watching(None);
            return Action::None;
        }
        match &mut self.state {
            State::Watching(watching) => {
                let Observation::Live(sample) = observation else {
                    *watching = None;
                    return Action::None;
                };
                let Some(watch) = watching
                    .as_mut()
                    .filter(|w| w.generation == sample.generation)
                else {
                    *watching = Some(Watching::new(sample, now));
                    return Action::None;
                };
                let video = watch.video.observe(
                    sample.media_input(),
                    sample.video_expected,
                    sample.video_output,
                    now,
                );
                let audio = watch.audio.observe(
                    sample.media_input(),
                    sample.audio_expected,
                    sample.audio_output,
                    now,
                );
                if !video && !audio {
                    return Action::None;
                }
                if self.attempts >= MAX_RESTARTS {
                    self.fail();
                    return Action::Failed;
                }
                if let Some(due) = self.last_restart.map(|last| last + RESTART_INTERVAL)
                    && now < due
                {
                    self.state = State::Waiting {
                        due,
                        baseline: sample,
                        watch: Watching::new(sample, now),
                    };
                    return Action::None;
                }
                self.restart(now)
            }
            State::Waiting {
                due,
                baseline,
                watch,
            } => {
                if let Observation::Live(sample) = observation {
                    watch.video.observe(
                        sample.media_input(),
                        sample.video_expected,
                        sample.video_output,
                        now,
                    );
                    watch.audio.observe(
                        sample.media_input(),
                        sample.audio_expected,
                        sample.audio_output,
                        now,
                    );
                    if sample.media_input() == 0
                        || now.saturating_duration_since(watch.video.input_at) >= INPUT_TIMEOUT
                    {
                        self.state = State::Watching(None);
                        return Action::None;
                    }
                    if restored(*baseline, sample) {
                        self.state = State::Watching(Some(Watching::new(sample, now)));
                        return Action::None;
                    }
                    if baseline_changed(*baseline, sample) {
                        *baseline = sample;
                    }
                } else {
                    self.state = State::Watching(None);
                    return Action::None;
                }
                if now >= *due {
                    self.restart(now)
                } else {
                    Action::None
                }
            }
            State::Starting { deadline, baseline } => {
                if let Observation::Live(sample) = observation {
                    if let Some(before) = baseline
                        && restored(*before, sample)
                    {
                        self.state = State::Watching(Some(Watching::new(sample, now)));
                        return Action::None;
                    }
                    if baseline.is_none_or(|before| baseline_changed(before, sample)) {
                        *baseline = Some(sample);
                    }
                }
                if now >= *deadline {
                    self.fail();
                    return Action::Failed;
                }
                Action::None
            }
            State::Failed => Action::None,
        }
    }
    fn restart(&mut self, now: Instant) -> Action {
        self.attempts += 1;
        self.last_restart = Some(now);
        self.state = State::Starting {
            deadline: now + RECOVERY_TIMEOUT,
            baseline: None,
        };
        Action::Restart(Restart {
            attempt: self.attempts,
        })
    }
}

fn baseline_changed(before: Sample, sample: Sample) -> bool {
    before.generation != sample.generation
        || (sample.video_expected && before.video_output.is_none())
        || (sample.audio_expected && before.audio_output.is_none())
        || matches!((before.video_output, sample.video_output), (Some(b), Some(a)) if a < b)
        || matches!((before.audio_output, sample.audio_output), (Some(b), Some(a)) if a < b)
}

fn restored(before: Sample, sample: Sample) -> bool {
    let advanced = |input: bool, before: Option<u64>, after: Option<u64>| {
        !input || matches!((before, after), (Some(b), Some(a)) if a > b)
    };
    before.generation == sample.generation
        && sample.received_bytes > before.received_bytes
        && (sample.video_input > 0 || sample.audio_input > 0)
        && advanced(
            sample.video_expected,
            before.video_output,
            sample.video_output,
        )
        && advanced(
            sample.audio_expected,
            before.audio_output,
            sample.audio_output,
        )
}

#[cfg(test)]
mod tests;
