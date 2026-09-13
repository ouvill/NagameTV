//! One source of truth for a stream's identity, activity and retry allowance.
use crate::channels::{BroadcastService, Channel};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Attempt {
    pub service: u64,
    pub name: String,
    pub broadcast: Option<BroadcastService>,
    retry: Retry,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Retry {
    Available,
    Used,
}

impl Attempt {
    pub(super) fn new(channel: &Channel) -> Self {
        Self {
            service: channel.id,
            name: channel.name.clone(),
            broadcast: channel.broadcast,
            retry: Retry::Available,
        }
    }
}

#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub(super) enum State {
    #[default]
    Stopped,
    Connecting(Attempt),
    Playing(Attempt),
    // Native stop failed: retain the identity and subscriptions until cleanup
    // succeeds, but do not advertise the stream as playing or connecting.
    StopFailed(Attempt),
}

impl State {
    pub(super) fn connecting(&self) -> bool {
        matches!(self, Self::Connecting(_))
    }

    pub(super) fn playing(&self) -> bool {
        matches!(self, Self::Playing(_))
    }

    pub(super) fn active_service(&self) -> Option<u64> {
        match self {
            Self::Stopped => None,
            Self::Connecting(attempt) | Self::Playing(attempt) | Self::StopFailed(attempt) => {
                Some(attempt.service)
            }
        }
    }

    pub(super) fn requested(&self, service: u64) -> bool {
        match self {
            Self::Connecting(attempt) | Self::Playing(attempt) => attempt.service == service,
            Self::Stopped | Self::StopFailed(_) => false,
        }
    }

    /// A late PLAYING message after stop must not resurrect a stopped stream.
    pub(super) fn started(&self) -> Option<Attempt> {
        match self {
            Self::Connecting(attempt) => Some(attempt.clone()),
            Self::Stopped | Self::Playing(_) | Self::StopFailed(_) => None,
        }
    }

    pub(super) fn stop_failed(&self) -> Self {
        match self {
            Self::Stopped => Self::Stopped,
            Self::Connecting(attempt) | Self::Playing(attempt) | Self::StopFailed(attempt) => {
                Self::StopFailed(attempt.clone())
            }
        }
    }

    /// Capture the original channel before cleanup or a catalog update. The
    /// retry token travels with the attempt, so a no-op Play cannot replenish it.
    pub(super) fn resume_retry(&self) -> Option<Attempt> {
        match self {
            Self::Connecting(attempt) | Self::Playing(attempt) => match attempt.retry {
                Retry::Available => Some(Attempt {
                    retry: Retry::Used,
                    ..attempt.clone()
                }),
                Retry::Used => None,
            },
            Self::Stopped | Self::StopFailed(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attempt() -> Attempt {
        Attempt {
            service: 42,
            name: "Original channel".into(),
            broadcast: Some(BroadcastService {
                network_id: 1,
                service_id: 2,
            }),
            retry: Retry::Available,
        }
    }

    #[test]
    fn one_retry_preserves_identity_and_only_a_new_attempt_restores_allowance() {
        let original = attempt();
        let state = State::Playing(State::Connecting(original.clone()).started().unwrap());
        let retry = state.resume_retry().unwrap();
        assert_eq!(retry.service, original.service);
        assert_eq!(retry.name, original.name);
        assert_eq!(retry.broadcast, original.broadcast);
        let state = State::Playing(State::Connecting(retry).started().unwrap());
        assert!(state.requested(original.service));
        assert!(state.resume_retry().is_none());
        assert!(State::Connecting(original).resume_retry().is_some());
    }

    #[test]
    fn stopped_and_failed_cleanup_reject_late_start_events() {
        assert!(State::Stopped.started().is_none());
        assert!(State::Stopped.resume_retry().is_none());
        let failed = State::Connecting(attempt()).stop_failed();
        assert_eq!(failed.active_service(), Some(42));
        assert!(!failed.playing());
        assert!(!failed.connecting());
        assert!(!failed.requested(42), "explicit Play must retry cleanup");
        assert!(failed.started().is_none());
        assert!(failed.resume_retry().is_none());
    }
}
