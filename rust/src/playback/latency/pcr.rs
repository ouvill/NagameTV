//! Map normalized PCR time to the local monotonic clock. This estimates a
//! presentation schedule at our receive point, not a broadcaster's wall clock.
use std::time::{Duration, Instant};

const WINDOW: Duration = Duration::from_secs(30);

#[derive(Debug, Default)]
pub(super) enum Clock {
    #[default]
    Waiting,
    Tracking(Tracking),
}

#[derive(Debug)]
pub(super) struct Tracking {
    window_start: Instant,
    current: Anchor,
    previous: Option<Anchor>,
    last_received: Instant,
}

#[derive(Clone, Copy, Debug)]
struct Anchor {
    position_ns: u64,
    received: Instant,
}

impl Anchor {
    fn deviation_ns(self, position_ns: u64, presented: Instant) -> i128 {
        let elapsed = if presented >= self.received {
            presented.duration_since(self.received).as_nanos() as i128
        } else {
            -(self.received.duration_since(presented).as_nanos() as i128)
        };
        elapsed - (i128::from(position_ns) - i128::from(self.position_ns))
    }
}

impl Clock {
    pub fn observe(&mut self, position_ns: u64, received: Instant) {
        let candidate = Anchor {
            position_ns,
            received,
        };
        if !self.is_fresh(received) {
            *self = Self::Tracking(Tracking {
                window_start: received,
                current: candidate,
                previous: None,
                last_received: received,
            });
            return;
        }
        let Self::Tracking(clock) = self else {
            unreachable!("fresh clock")
        };
        let elapsed = received.saturating_duration_since(clock.window_start);
        if elapsed >= WINDOW {
            clock.previous = (elapsed < 2 * WINDOW).then_some(clock.current);
            clock.current = candidate;
            clock.window_start = received;
        } else if clock.current.deviation_ns(position_ns, received) < 0 {
            // The least delayed observation reduces chunking/read jitter.
            // No unbounded history and no civil/server clock synchronization.
            clock.current = candidate;
        }
        clock.last_received = received;
    }

    pub fn is_fresh(&self, now: Instant) -> bool {
        match self {
            Self::Waiting => false,
            Self::Tracking(clock) => {
                now.saturating_duration_since(clock.last_received) <= super::STALE_AFTER
            }
        }
    }

    pub fn deviation_ns(&self, position_ns: u64, presented: Instant) -> Option<i128> {
        if !self.is_fresh(presented) {
            return None;
        }
        let Self::Tracking(clock) = self else {
            return None;
        };
        let current = clock.current.deviation_ns(position_ns, presented);
        // Minimum receive-time offset means maximum measured lateness.
        Some(clock.previous.map_or(current, |previous| {
            current.max(previous.deviation_ns(position_ns, presented))
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    const NS_PER_MS: u64 = 1_000_000;

    proptest! {
        #[test]
        fn minimum_arrival_offset_is_independent_of_media_time_origin(
            origin in 0_u64..(u64::MAX / 2),
            jitter in prop::collection::vec(0_u64..20, 1..40),
        ) {
            const INTERVAL_MS: u64 = 40;
            const PRESENTATION_LAG_MS: u64 = 80;
            let start = Instant::now();
            let mut clock = Clock::default();
            for (index, lag) in jitter.iter().enumerate() {
                let media_ms = index as u64 * INTERVAL_MS;
                clock.observe(origin + media_ms * NS_PER_MS, start + Duration::from_millis(media_ms + lag));
            }
            let last_ms = (jitter.len() - 1) as u64 * INTERVAL_MS;
            let expected = (PRESENTATION_LAG_MS - jitter.iter().min().unwrap()) * NS_PER_MS;
            prop_assert_eq!(clock.deviation_ns(origin + last_ms * NS_PER_MS,
                start + Duration::from_millis(last_ms + PRESENTATION_LAG_MS)), Some(i128::from(expected)));
        }
    }

    #[test]
    fn chunk_jitter_is_removed_and_early_presentation_stays_negative() {
        let start = Instant::now();
        let mut clock = Clock::default();
        assert_eq!(clock.deviation_ns(0, start), None);
        clock.observe(0, start);
        clock.observe(100 * NS_PER_MS, start + Duration::from_millis(150));
        assert_eq!(
            clock.deviation_ns(400 * NS_PER_MS, start + Duration::from_millis(600)),
            Some(200_000_000)
        );
        // A chunk can contain multiple PCRs. The latest has least arrival lag.
        clock.observe(200 * NS_PER_MS, start + Duration::from_millis(150));
        assert_eq!(
            clock.deviation_ns(400 * NS_PER_MS, start + Duration::from_millis(600)),
            Some(250_000_000)
        );
        assert_eq!(
            clock.deviation_ns(400 * NS_PER_MS, start + Duration::from_millis(250)),
            Some(-100_000_000)
        );
    }

    #[test]
    fn old_minimum_expires_while_continuous_reception_follows_clock_drift() {
        let start = Instant::now();
        let mut clock = Clock::default();
        clock.observe(0, start);
        for second in 1..=61 {
            clock.observe(
                second * 1_000 * NS_PER_MS,
                start + Duration::from_secs(second) + Duration::from_millis(10),
            );
            let expected = if second < 60 { 10_000_000 } else { 0 };
            assert_eq!(
                clock.deviation_ns(
                    second * 1_000 * NS_PER_MS,
                    start + Duration::from_secs(second) + Duration::from_millis(10)
                ),
                Some(expected)
            );
        }
    }

    #[test]
    fn stale_clock_is_not_extrapolated_and_resume_starts_a_new_mapping() {
        let start = Instant::now();
        let mut clock = Clock::default();
        clock.observe(0, start);
        assert!(
            clock
                .deviation_ns(0, start + super::super::STALE_AFTER)
                .is_some()
        );
        let resumed = start + super::super::STALE_AFTER + Duration::from_nanos(1);
        assert_eq!(clock.deviation_ns(0, resumed), None);
        clock.observe(NS_PER_MS, resumed);
        assert_eq!(clock.deviation_ns(NS_PER_MS, resumed), Some(0));
    }
}
