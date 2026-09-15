//! The active attempt owns its input; stopped state retains only the replay target.
use crate::{
    channels::{BroadcastService, Channel},
    playback::recording::Recording,
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
    pub(super) fn new(channel: &Channel) -> Self {
        Self::Live(LiveAttempt {
            service: channel.id,
            name: channel.name.clone(),
            broadcast: channel.broadcast,
            retry: Retry::Available,
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
    Playing(Attempt),
    // Failed cleanup retains input identity and native subscriptions for retry.
    StopFailed(Attempt),
}
impl Default for State {
    fn default() -> Self {
        Self::Stopped(Selection::Live)
    }
}
impl State {
    pub(super) fn connecting(&self) -> bool {
        matches!(self, Self::Connecting(_))
    }
    pub(super) fn playing(&self) -> bool {
        matches!(self, Self::Playing(_))
    }
    pub(super) fn recording(&self) -> Option<&Recording> {
        match self {
            Self::Stopped(Selection::File(file))
            | Self::Connecting(Attempt::File(file))
            | Self::Playing(Attempt::File(file))
            | Self::StopFailed(Attempt::File(file)) => Some(file),
            Self::Stopped(Selection::Live)
            | Self::Connecting(Attempt::Live(_))
            | Self::Playing(Attempt::Live(_))
            | Self::StopFailed(Attempt::Live(_)) => None,
        }
    }
    pub(super) fn active_service(&self) -> Option<u64> {
        match self {
            Self::Stopped(_) => None,
            Self::Connecting(attempt) | Self::Playing(attempt) | Self::StopFailed(attempt) => {
                attempt.service()
            }
        }
    }
    pub(super) fn requested(&self, service: u64) -> bool {
        match self {
            Self::Connecting(attempt) | Self::Playing(attempt) => {
                attempt.service() == Some(service)
            }
            Self::Stopped(_) | Self::StopFailed(_) => false,
        }
    }
    /// Consume the attempt; late PLAYING messages cannot resurrect stopped input.
    pub(super) fn started(self) -> Self {
        match self {
            Self::Connecting(attempt) => Self::Playing(attempt),
            Self::Stopped(_) | Self::Playing(_) | Self::StopFailed(_) => self,
        }
    }
    pub(super) fn stopped(self) -> Self {
        match self {
            Self::Stopped(_) => self,
            Self::Connecting(attempt) | Self::Playing(attempt) | Self::StopFailed(attempt) => {
                attempt.stopped()
            }
        }
    }
    pub(super) fn stop_failed(self) -> Self {
        match self {
            Self::Stopped(_) => self,
            Self::Connecting(attempt) | Self::Playing(attempt) | Self::StopFailed(attempt) => {
                Self::StopFailed(attempt)
            }
        }
    }
    /// Taking a retry spends the original allowance, even before native cleanup.
    pub(super) fn take_retry(&mut self) -> Option<Attempt> {
        match self {
            Self::Connecting(Attempt::Live(live)) | Self::Playing(Attempt::Live(live)) => {
                live.take_retry().map(Attempt::Live)
            }
            Self::Connecting(Attempt::File(_))
            | Self::Playing(Attempt::File(_))
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
