//! The active attempt owns its input; stopped state retains only the replay target.
use crate::{
    channels::{BroadcastService, Channel},
    playback::{
        input::{Policy, Retention},
        recording::Recording,
        timeline::{Phase, Resume},
    },
};

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Attempt {
    Live(LiveAttempt),
    File(Recording),
}

// No Clone or public fields: only a new channel request can grant a fresh retry.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct LiveAttempt {
    service: u64,
    name: String,
    broadcast: Option<BroadcastService>,
    retry: Retry,
    retention: Policy,
}
#[derive(Debug, PartialEq, Eq)]
enum Retry {
    Available,
    Used,
}

impl LiveAttempt {
    pub(super) fn service(&self) -> u64 {
        self.service
    }
    pub(super) fn retention(&self) -> Policy {
        self.retention
    }
    pub(super) fn broadcast(&self) -> Option<BroadcastService> {
        self.broadcast
    }
    fn take_retry(&mut self) -> Option<Self> {
        match std::mem::replace(&mut self.retry, Retry::Used) {
            Retry::Available => Some(Self {
                service: self.service,
                name: self.name.clone(),
                broadcast: self.broadcast,
                retry: Retry::Used,
                retention: self.retention,
            }),
            Retry::Used => None,
        }
    }
}

#[derive(Default, Debug, PartialEq, Eq)]
pub(super) enum Selection {
    #[default]
    Live,
    File(Recording),
}

impl Attempt {
    pub(super) fn new(channel: &Channel, retention: Policy) -> Self {
        Self::Live(LiveAttempt {
            service: channel.id,
            name: channel.name.clone(),
            broadcast: channel.broadcast,
            retry: Retry::Available,
            retention,
        })
    }
    pub(super) fn service(&self) -> Option<u64> {
        match self {
            Self::Live(live) => Some(live.service),
            Self::File(_) => None,
        }
    }
    pub(super) fn name(&self) -> &str {
        match self {
            Self::Live(live) => &live.name,
            Self::File(file) => file.name(),
        }
    }
    pub(super) fn stopped(self) -> State {
        State::Stopped(match self {
            Self::Live(_) => Selection::Live,
            Self::File(file) => Selection::File(file),
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum State {
    Stopped(Selection),
    Connecting(Attempt),
    Playing(LiveAttempt, Phase),
    Recording(Recording, Phase),
    // Failed cleanup retains input identity and native subscriptions for retry.
    StopFailed(Attempt),
}
impl Default for State {
    fn default() -> Self {
        Self::Stopped(Selection::Live)
    }
}
impl State {
    pub(super) fn retain_subtitle(&mut self, script: Option<crate::media_subtitles::Script>) {
        match self {
            Self::Recording(file, _)
            | Self::Connecting(Attempt::File(file))
            | Self::StopFailed(Attempt::File(file))
            | Self::Stopped(Selection::File(file)) => file.retain_subtitle(script),
            Self::Playing(_, _)
            | Self::Connecting(Attempt::Live(_))
            | Self::StopFailed(Attempt::Live(_))
            | Self::Stopped(Selection::Live) => {}
        }
    }

    pub(super) fn reconfigured_live(mut self, retention: Policy) -> Self {
        match &mut self {
            Self::Playing(live, _)
            | Self::Connecting(Attempt::Live(live))
            | Self::StopFailed(Attempt::Live(live)) => live.retention = retention,
            Self::Stopped(_)
            | Self::Recording(_, _)
            | Self::Connecting(Attempt::File(_))
            | Self::StopFailed(Attempt::File(_)) => {}
        }
        self
    }

    pub(super) fn connecting(&self) -> bool {
        matches!(self, Self::Connecting(_))
    }
    pub(super) fn playing(&self) -> bool {
        matches!(
            self,
            Self::Playing(_, Phase::Playing | Phase::Seeking(Resume::Playing))
                | Self::Recording(_, Phase::Playing | Phase::Seeking(Resume::Playing))
        )
    }
    pub(super) fn paused(&self) -> bool {
        matches!(
            self,
            Self::Playing(_, Phase::Paused | Phase::Seeking(Resume::Paused))
                | Self::Recording(_, Phase::Paused | Phase::Seeking(Resume::Paused))
        )
    }
    pub(super) fn seeking(&self) -> bool {
        matches!(
            self,
            Self::Playing(_, Phase::Seeking(_)) | Self::Recording(_, Phase::Seeking(_))
        )
    }
    pub(super) fn ended(&self) -> bool {
        matches!(self, Self::Recording(_, Phase::Ended))
    }
    pub(super) fn pausable(&self) -> bool {
        matches!(self, Self::Playing(live, _) if live.retention.storage() != Retention::Off)
    }
    pub(super) fn timeshift(&self, has_history: bool) -> bool {
        matches!(self, Self::Playing(live, _) if live.retention.storage() != Retention::Off
            && (live.retention.activation() == crate::playback::input::Activation::Always || has_history))
    }
    pub(super) fn active(&self) -> bool {
        matches!(self, Self::Playing(_, _) | Self::Recording(_, _))
    }
    pub(super) fn transport(self, phase: Phase) -> Self {
        match self {
            Self::Recording(file, _) => Self::Recording(file, phase),
            Self::Playing(live, _) => Self::Playing(live, phase),
            Self::Stopped(_) | Self::Connecting(_) | Self::StopFailed(_) => self,
        }
    }
    pub(super) fn recording(&self) -> Option<&Recording> {
        match self {
            Self::Stopped(Selection::File(file))
            | Self::Connecting(Attempt::File(file))
            | Self::Recording(file, _)
            | Self::StopFailed(Attempt::File(file)) => Some(file),
            Self::Stopped(Selection::Live)
            | Self::Connecting(Attempt::Live(_))
            | Self::Playing(_, _)
            | Self::StopFailed(Attempt::Live(_)) => None,
        }
    }
    pub(super) fn active_service(&self) -> Option<u64> {
        match self {
            Self::Stopped(_) | Self::Recording(_, _) => None,
            Self::Playing(live, _) => Some(live.service()),
            Self::Connecting(attempt) | Self::StopFailed(attempt) => attempt.service(),
        }
    }
    pub(super) fn requested(&self, service: u64) -> bool {
        match self {
            Self::Connecting(attempt) => attempt.service() == Some(service),
            Self::Playing(live, _) => live.service() == service,
            Self::Stopped(_) | Self::StopFailed(_) | Self::Recording(_, _) => false,
        }
    }
    /// Consume the attempt; late PLAYING messages cannot resurrect stopped input.
    pub(super) fn started(self) -> Self {
        match self {
            Self::Connecting(Attempt::Live(live)) => Self::Playing(live, Phase::Playing),
            Self::Connecting(Attempt::File(file)) => Self::Recording(file, Phase::Playing),
            Self::Stopped(_)
            | Self::Playing(_, _)
            | Self::StopFailed(_)
            | Self::Recording(_, _) => self,
        }
    }
    pub(super) fn stopped(self) -> Self {
        match self {
            Self::Stopped(_) => self,
            Self::Playing(live, _) => Attempt::Live(live).stopped(),
            Self::Recording(file, _) => Attempt::File(file).stopped(),
            Self::Connecting(attempt) | Self::StopFailed(attempt) => attempt.stopped(),
        }
    }
    pub(super) fn stop_failed(self) -> Self {
        match self {
            Self::Stopped(_) => self,
            Self::Playing(live, _) => Self::StopFailed(Attempt::Live(live)),
            Self::Recording(file, _) => Self::StopFailed(Attempt::File(file)),
            Self::Connecting(attempt) | Self::StopFailed(attempt) => Self::StopFailed(attempt),
        }
    }
    /// Taking a retry spends the original allowance, even before native cleanup.
    pub(super) fn take_retry(&mut self) -> Option<Attempt> {
        match self {
            Self::Connecting(Attempt::Live(live)) | Self::Playing(live, _) => {
                live.take_retry().map(Attempt::Live)
            }
            Self::Connecting(Attempt::File(_))
            | Self::Recording(_, _)
            | Self::Stopped(_)
            | Self::StopFailed(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn attempt() -> Attempt {
        Attempt::Live(LiveAttempt {
            service: 42,
            name: "Original channel".into(),
            broadcast: Some(BroadcastService {
                network_id: 1,
                service_id: 2,
            }),
            retry: Retry::Available,
            retention: Retention::Memory.into(),
        })
    }
    #[test]
    fn retry_is_spent_at_extraction_and_cannot_be_duplicated() {
        let mut state = State::Connecting(attempt()).started();
        let retry = state.take_retry().unwrap();
        assert_eq!(retry.service(), Some(42));
        assert_eq!(retry.name(), "Original channel");
        assert!(
            state.take_retry().is_none(),
            "taking twice must not duplicate the allowance"
        );
        let mut state = State::Connecting(retry).started();
        assert!(state.requested(42));
        assert!(state.take_retry().is_none());
        assert!(State::Connecting(attempt()).take_retry().is_some());
    }
    #[test]
    fn stopped_and_failed_cleanup_reject_late_start_events() {
        assert_eq!(State::default().started(), State::default());
        let mut failed = State::Connecting(attempt()).stop_failed().started();
        assert_eq!(failed.active_service(), Some(42));
        assert!(!failed.playing() && !failed.connecting() && !failed.requested(42));
        assert!(failed.take_retry().is_none());
    }
    #[test]
    fn recording_identity_survives_stop_failure_and_normal_eof() {
        let file = Recording::open(std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../tests/fixtures/subtitle-clock.ts"
        )))
        .unwrap();
        let expected = file.clone();
        let mut state = State::Connecting(Attempt::File(file));
        assert_eq!(state.recording(), Some(&expected));
        assert!(state.active_service().is_none());
        assert!(state.take_retry().is_none());
        state = state.started();
        assert!(state.playing());
        state = state.stop_failed().started();
        assert!(matches!(state, State::StopFailed(Attempt::File(_))));
        assert_eq!(state.recording(), Some(&expected));
        state = state.stopped().started();
        assert!(!state.playing() && !state.connecting());
        assert_eq!(state.recording(), Some(&expected));
    }
}
