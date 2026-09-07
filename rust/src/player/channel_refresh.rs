//! Monotonic catalog refresh deadline. Requests themselves remain owned by Player.
use std::time::{Duration, Instant};
const INTERVAL: Duration = Duration::from_secs(300);

#[derive(Default)]
pub(super) enum Refresh {
    #[default]
    Disabled,
    Scheduled(Instant),
}
impl Refresh {
    pub(super) fn enabled(&self) -> bool {
        matches!(self, Self::Scheduled(_))
    }
    pub(super) fn requested(&mut self, now: Instant) {
        *self = Self::Scheduled(now);
    }
    pub(super) fn due(&self, now: Instant, in_flight: bool) -> bool {
        !in_flight
            && matches!(self, Self::Scheduled(last) if now.saturating_duration_since(*last) >= INTERVAL)
    }
    pub(super) fn requested_by_user(&self, now: Instant, in_flight: bool, force: bool) -> bool {
        !in_flight
            && matches!(self, Self::Scheduled(last) if force || now.saturating_duration_since(*last) >= Duration::from_secs(60))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn coalesces_overdue_work_and_disabling_invalidates_old_deadlines() {
        let now = Instant::now();
        let mut refresh = Refresh::default();
        assert!(!refresh.due(now + INTERVAL, false));
        assert!(!refresh.requested_by_user(now, false, true));
        refresh.requested(now);
        assert!(!refresh.requested_by_user(now, true, true));
        assert!(refresh.requested_by_user(now, false, true));
        assert!(!refresh.requested_by_user(now + Duration::from_secs(59), false, false));
        assert!(refresh.requested_by_user(now + Duration::from_secs(60), false, false));
        assert!(!refresh.due(now + INTERVAL - Duration::from_nanos(1), false));
        assert!(!refresh.due(now + INTERVAL * 3, true));
        assert!(refresh.due(now + INTERVAL * 3, false));
        refresh.requested(now + INTERVAL * 3);
        assert!(!refresh.due(now + INTERVAL * 3, false));
        refresh = Refresh::Disabled;
        assert!(!refresh.due(now + INTERVAL * 10, false));
    }
}
