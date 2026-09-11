//! Collapse changes to at most one refresh every five seconds, including quiet tails.
use std::time::{Duration, Instant};
pub(crate) const INTERVAL: Duration = Duration::from_secs(5);

pub struct RefreshGate {
    dirty: bool,
    next: Instant,
    interval: Duration,
}
impl RefreshGate {
    pub fn new(now: Instant) -> Self {
        Self::with_interval(now, INTERVAL)
    }
    pub(crate) fn with_interval(now: Instant, interval: Duration) -> Self {
        Self {
            dirty: false,
            next: now + interval,
            interval,
        }
    }
    /// Also call after a disconnected stream: events may have been missed.
    pub fn changed(&mut self) {
        self.dirty = true;
    }
    /// The receiver must poll this even when no subsequent event arrives.
    pub fn take_due(&mut self, now: Instant) -> bool {
        if !self.dirty || now < self.next {
            return false;
        }
        self.dirty = false;
        self.next = now + self.interval;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bursts_and_a_quiet_tail_produce_one_refresh_per_interval() {
        let now = Instant::now();
        let mut gate = RefreshGate::new(now);
        assert!(!gate.take_due(now));
        for _ in 0..10_000 {
            gate.changed();
        }
        assert!(!gate.take_due(now + INTERVAL - Duration::from_nanos(1)));
        assert!(gate.take_due(now + INTERVAL));
        assert!(!gate.take_due(now + INTERVAL * 2));
        gate.changed();
        assert!(gate.take_due(now + INTERVAL * 2));
        gate.changed();
        assert!(!gate.take_due(now + INTERVAL * 2));
        assert!(gate.take_due(now + INTERVAL * 3));
    }

    #[test]
    fn continuing_changes_do_not_postpone_refresh_and_last_change_is_flushed() {
        let now = Instant::now();
        let mut gate = RefreshGate::new(now);
        for second in 0..=12 {
            gate.changed();
            assert_eq!(
                gate.take_due(now + Duration::from_secs(second)),
                second == 5 || second == 10
            );
        }
        assert!(!gate.take_due(now + Duration::from_secs(14)));
        assert!(gate.take_due(now + Duration::from_secs(15)));
        assert!(!gate.take_due(now + Duration::from_secs(20)));
    }
}
