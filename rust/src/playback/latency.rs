//! Bounded receive delay and PCR-relative presentation estimates. No Qt or pixels.
mod pcr;
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque, btree_map::Entry},
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};

const RECEIPT_LIFETIME: Duration = Duration::from_secs(30);
const SAMPLE_WINDOW: Duration = Duration::from_secs(10);
const STALE_AFTER: Duration = Duration::from_secs(2);
const MAX_RECEIPTS: usize = 4096;
const MAX_SAMPLES: usize = 1200;
// PCR/PTS conversion and seek seeding can each lose a 90 kHz tick.
// Allow two ticks (22.23 us), still far below a broadcast frame interval.
pub(super) const TIMESTAMP_TOLERANCE_NS: u64 = 25_000;

#[derive(Clone, Debug, Default)]
pub(crate) struct Tracker(Arc<Mutex<State>>);
#[derive(Debug, Default)]
struct State {
    generation: u64,
    // PCR epochs can overlap the previous epoch's decode-ahead PTS range.
    // Do not let late output from that range match a new epoch's receipts.
    previous_end: Option<u64>,
    received_end: Option<u64>,
    receipts: BTreeMap<u64, Receipt>,
    // Only waiting receipts need expiration. Arrival order differs from PTS
    // order, so index their deadlines separately instead of scanning all PTS.
    waiting: BTreeSet<(Instant, u64)>,
    samples: VecDeque<Sample>,
    clock: pcr::Clock,
}
#[derive(Debug)]
enum Receipt {
    Waiting(Instant),
    Presented,
    Ambiguous,
}
#[derive(Clone, Debug)]
struct Sample {
    at: Instant,
    delay: Duration,
    pcr_deviation_ns: Option<i128>,
}

/// Only a matched receipt can produce a measurement, in its original generation.
#[derive(Clone, Debug)]
pub(crate) struct Pending {
    tracker: Tracker,
    generation: u64,
    position: u64,
    received: Instant,
}
#[derive(Debug, Default)]
pub(crate) struct Measurements {
    pub receive: Snapshot,
    pub pcr: Snapshot,
}

#[derive(Debug, Default, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum Snapshot {
    #[default]
    Unavailable,
    Waiting,
    Measuring {
        latest_ms: f64,
        median_ms: f64,
        p95_ms: f64,
        samples: usize,
    },
}
impl Tracker {
    fn state(&self) -> MutexGuard<'_, State> {
        self.0.lock().unwrap_or_else(|error| {
            tracing::error!(%error, "Could not lock video latency measurements; discarding poisoned state");
            let mut state = error.into_inner();
            state.generation = state.generation.wrapping_add(1);
            state.receipts.clear();
            state.waiting.clear();
            state.samples.clear();
            state.clock = pcr::Clock::default();
            self.0.clear_poison();
            state
        })
    }
    pub fn observe_pcr(&self, position_ns: u64, received: Instant) {
        let mut state = self.state();
        if !state.clock.is_fresh(received) {
            for sample in &mut state.samples {
                sample.pcr_deviation_ns = None;
            }
        }
        state.clock.observe(position_ns, received);
    }
    pub fn receive(&self, position: u64, now: Instant) {
        let mut state = self.state();
        if state.previous_end.is_some_and(|end| position <= end) {
            return;
        }
        state.received_end = Some(state.received_end.map_or(position, |end| end.max(position)));
        // Packet timestamps can arrive out of presentation order (B frames).
        // Bound both time and count, including malformed repeated timestamps.
        state.expire_receipts(now);
        match state.receipts.entry(position) {
            Entry::Occupied(mut entry) => {
                if let Receipt::Waiting(at) = entry.insert(Receipt::Ambiguous) {
                    state.waiting.remove(&(at, position));
                }
            }
            Entry::Vacant(entry) => {
                entry.insert(Receipt::Waiting(now));
                state.waiting.insert((now, position));
            }
        }
        while state.receipts.len() > MAX_RECEIPTS {
            if let Some((position, Receipt::Waiting(at))) = state.receipts.pop_first() {
                state.waiting.remove(&(at, position));
            }
        }
    }
    pub fn match_frame(&self, position: u64) -> Option<Pending> {
        let state = self.state();
        let mut candidates = state.receipts.range(
            position.saturating_sub(TIMESTAMP_TOLERANCE_NS)
                ..=position.saturating_add(TIMESTAMP_TOLERANCE_NS),
        );
        let (&position, receipt) = candidates.next()?;
        if candidates.next().is_some() {
            return None;
        }
        let Receipt::Waiting(received) = receipt else {
            return None;
        };
        Some(Pending {
            tracker: self.clone(),
            generation: state.generation,
            position,
            received: *received,
        })
    }
    /// A flush invalidates in-flight output, but keeps original receive times.
    pub fn reset_output(&self) {
        let mut state = self.state();
        state.generation = state.generation.wrapping_add(1);
        state.samples.clear();
    }
    /// A transport discontinuity invalidates the timestamp correspondence too.
    pub fn reset(&self) {
        let mut state = self.state();
        state.generation = state.generation.wrapping_add(1);
        state.previous_end = state
            .received_end
            .map(|end| end.saturating_add(2 * TIMESTAMP_TOLERANCE_NS));
        state.receipts.clear();
        state.waiting.clear();
        state.samples.clear();
        state.clock = pcr::Clock::default();
    }
    pub fn snapshot(&self, now: Instant) -> Measurements {
        let mut state = self.state();
        state.expire_samples(now);
        let samples: Vec<_> = state.samples.iter().cloned().collect();
        let pcr_fresh = state.clock.is_fresh(now);
        // Statistics must not hold up reception or Qt's presentation callback.
        // Copy one consistent window, then select quantiles outside the lock.
        drop(state);
        Measurements {
            receive: Snapshot::summarize(&samples, now, |sample| {
                Some(sample.delay.as_nanos() as i128)
            }),
            pcr: if pcr_fresh {
                Snapshot::summarize(&samples, now, |sample| sample.pcr_deviation_ns)
            } else {
                Snapshot::Waiting
            },
        }
    }
}
impl Snapshot {
    fn summarize(
        samples: &[Sample],
        now: Instant,
        value: impl Fn(&Sample) -> Option<i128>,
    ) -> Self {
        let Some((_, latest)) = samples
            .iter()
            .rev()
            .find_map(|sample| value(sample).map(|value| (sample.at, value)))
            .filter(|(at, _)| now.saturating_duration_since(*at) <= STALE_AFTER)
        else {
            return Self::Waiting;
        };
        const NS_PER_MS: f64 = 1_000_000.0;
        let latest_ms = latest as f64 / NS_PER_MS;
        let mut measurements: Vec<_> = samples.iter().filter_map(value).collect();
        let count = measurements.len();
        let p95 = *measurements
            .select_nth_unstable((count * 95).div_ceil(100) - 1)
            .1;
        let (lower, upper_median, _) = measurements.select_nth_unstable(count / 2);
        let lower_median = if count.is_multiple_of(2) {
            *lower
                .iter()
                .max()
                .expect("even sample count has a lower half")
        } else {
            *upper_median
        };
        Self::Measuring {
            latest_ms,
            median_ms: (lower_median + *upper_median) as f64 / (2.0 * NS_PER_MS),
            p95_ms: p95 as f64 / NS_PER_MS,
            samples: count,
        }
    }
}
impl State {
    fn expire_receipts(&mut self, now: Instant) {
        while let Some(&(at, position)) = self.waiting.first() {
            if now.saturating_duration_since(at) <= RECEIPT_LIFETIME {
                break;
            }
            self.waiting.pop_first();
            self.receipts.remove(&position);
        }
    }
    fn expire_samples(&mut self, now: Instant) {
        while self
            .samples
            .front()
            .is_some_and(|sample| now.saturating_duration_since(sample.at) > SAMPLE_WINDOW)
        {
            self.samples.pop_front();
        }
    }
}
impl Pending {
    pub fn present(&self, now: Instant) {
        let mut state = self.tracker.state();
        if self.generation != state.generation {
            return;
        }
        let Some(Receipt::Waiting(received)) = state.receipts.get(&self.position) else {
            return;
        };
        if *received != self.received {
            return;
        }
        let Some(delay) = now
            .checked_duration_since(self.received)
            .filter(|delay| *delay <= RECEIPT_LIFETIME)
        else {
            return;
        };
        state.receipts.insert(self.position, Receipt::Presented);
        state.waiting.remove(&(self.received, self.position));
        state.expire_samples(now);
        if state.samples.len() == MAX_SAMPLES {
            state.samples.pop_front();
        }
        let pcr_deviation_ns = state.clock.deviation_ns(self.position, now);
        state.samples.push_back(Sample {
            at: now,
            delay,
            pcr_deviation_ns,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    const FRAME_NS: u64 = 40_000_000;

    #[test]
    fn receive_delay_and_signed_pcr_deviation_measure_different_intervals() {
        let tracker = Tracker::default();
        let start = Instant::now();
        tracker.observe_pcr(0, start);
        for (position, receive_ms) in [(10 * FRAME_NS, 0), (20 * FRAME_NS, 300)] {
            tracker.receive(position, start + Duration::from_millis(receive_ms));
            tracker
                .match_frame(position)
                .unwrap()
                .present(start + Duration::from_millis(600));
        }
        let measurements = tracker.snapshot(start + Duration::from_millis(600));
        assert!(matches!(
            measurements.receive,
            Snapshot::Measuring {
                latest_ms: 300.0,
                median_ms: 450.0,
                p95_ms: 600.0,
                samples: 2
            }
        ));
        assert!(matches!(
            measurements.pcr,
            Snapshot::Measuring {
                latest_ms: -200.0,
                median_ms: 0.0,
                p95_ms: 200.0,
                samples: 2
            }
        ));
    }

    #[test]
    fn flush_preserves_the_receive_clock_but_discontinuity_discards_it() {
        let tracker = Tracker::default();
        let start = Instant::now();
        tracker.observe_pcr(0, start);
        tracker.receive(FRAME_NS, start);
        let old = tracker.match_frame(FRAME_NS).unwrap();
        tracker.reset_output();
        old.present(start);
        assert!(matches!(tracker.snapshot(start).pcr, Snapshot::Waiting));
        tracker.match_frame(FRAME_NS).unwrap().present(start);
        assert!(matches!(
            tracker.snapshot(start).pcr,
            Snapshot::Measuring {
                latest_ms: -40.0,
                ..
            }
        ));
        tracker.reset();
        tracker.receive(2 * FRAME_NS, start);
        tracker.match_frame(2 * FRAME_NS).unwrap().present(start);
        let measurements = tracker.snapshot(start);
        assert!(matches!(measurements.receive, Snapshot::Measuring { .. }));
        assert!(matches!(measurements.pcr, Snapshot::Waiting));
    }

    #[test]
    fn missing_pcr_does_not_hide_receive_delay_or_reuse_a_stale_estimate() {
        let tracker = Tracker::default();
        let start = Instant::now();
        tracker.observe_pcr(0, start);
        tracker.receive(FRAME_NS, start);
        tracker.match_frame(FRAME_NS).unwrap().present(start);
        let resumed = start + STALE_AFTER + Duration::from_nanos(1);
        tracker.receive(2 * FRAME_NS, resumed);
        tracker.match_frame(2 * FRAME_NS).unwrap().present(resumed);
        let measurements = tracker.snapshot(resumed);
        assert!(matches!(
            measurements.receive,
            Snapshot::Measuring { samples: 2, .. }
        ));
        assert!(matches!(measurements.pcr, Snapshot::Waiting));
        tracker.observe_pcr(2 * FRAME_NS, resumed);
        assert!(matches!(tracker.snapshot(resumed).pcr, Snapshot::Waiting));
        tracker.receive(3 * FRAME_NS, resumed);
        tracker.match_frame(3 * FRAME_NS).unwrap().present(resumed);
        assert!(matches!(
            tracker.snapshot(resumed).pcr,
            Snapshot::Measuring {
                samples: 1,
                latest_ms: -40.0,
                ..
            }
        ));
    }

    #[test]
    fn timestamp_rounding_is_bounded_and_never_selects_an_ambiguous_frame() {
        let tracker = Tracker::default();
        let now = Instant::now();
        tracker.receive(FRAME_NS, now);
        for offset in [0, TIMESTAMP_TOLERANCE_NS] {
            assert!(tracker.match_frame(FRAME_NS + offset).is_some());
            assert!(tracker.match_frame(FRAME_NS - offset).is_some());
        }
        assert!(
            tracker
                .match_frame(FRAME_NS + TIMESTAMP_TOLERANCE_NS + 1)
                .is_none()
        );
        tracker.receive(FRAME_NS + TIMESTAMP_TOLERANCE_NS, now);
        assert!(tracker.match_frame(FRAME_NS).is_none());
    }

    #[test]
    fn a_clock_restart_excludes_old_output_even_with_rounding() {
        let tracker = Tracker::default();
        let now = Instant::now();
        tracker.receive(FRAME_NS, now);
        tracker.reset();
        tracker.receive(FRAME_NS + TIMESTAMP_TOLERANCE_NS, now);
        let new_position = FRAME_NS + 3 * TIMESTAMP_TOLERANCE_NS;
        tracker.receive(new_position, now);
        assert!(
            tracker
                .match_frame(FRAME_NS + TIMESTAMP_TOLERANCE_NS)
                .is_none()
        );
        assert!(tracker.match_frame(new_position).is_some());
    }

    #[test]
    fn reordered_receipts_and_repeated_presentations_count_each_frame_once() {
        let tracker = Tracker::default();
        let received = Instant::now();
        tracker.receive(2 * FRAME_NS, received);
        tracker.receive(FRAME_NS, received + Duration::from_millis(10));
        let early = tracker.match_frame(FRAME_NS + 1).unwrap();
        early.present(received + Duration::from_millis(110));
        early.present(received + Duration::from_millis(160));
        assert!(tracker.match_frame(FRAME_NS).is_none());
        tracker
            .match_frame(2 * FRAME_NS)
            .unwrap()
            .present(received + Duration::from_millis(200));
        let Snapshot::Measuring {
            latest_ms,
            median_ms,
            p95_ms,
            samples,
        } = tracker
            .snapshot(received + Duration::from_millis(201))
            .receive
        else {
            panic!("measurement");
        };
        assert_eq!(
            (latest_ms, median_ms, p95_ms, samples),
            (200.0, 150.0, 200.0, 2)
        );
    }
    #[test]
    fn ambiguous_missing_expired_and_old_generation_frames_are_not_measurements() {
        let tracker = Tracker::default();
        let now = Instant::now();
        assert!(tracker.match_frame(FRAME_NS).is_none());
        tracker.receive(FRAME_NS, now);
        let pending = tracker.match_frame(FRAME_NS).unwrap();
        tracker.receive(FRAME_NS, now);
        pending.present(now + Duration::from_millis(100));
        assert!(tracker.match_frame(FRAME_NS).is_none());
        tracker.receive(2 * FRAME_NS, now);
        let pending = tracker.match_frame(2 * FRAME_NS).unwrap();
        tracker.reset_output();
        pending.present(now + Duration::from_millis(100));
        assert!(matches!(tracker.snapshot(now).receive, Snapshot::Waiting));
        let pending = tracker.match_frame(2 * FRAME_NS).unwrap();
        tracker.reset();
        tracker.receive(2 * FRAME_NS, now);
        pending.present(now + Duration::from_millis(100));
        assert!(matches!(tracker.snapshot(now).receive, Snapshot::Waiting));
        assert!(tracker.match_frame(2 * FRAME_NS).is_none());
        tracker.receive(3 * FRAME_NS, now);
        tracker
            .match_frame(3 * FRAME_NS)
            .unwrap()
            .present(now + RECEIPT_LIFETIME + Duration::from_nanos(1));
        assert!(matches!(tracker.snapshot(now).receive, Snapshot::Waiting));
    }
    #[test]
    fn stale_output_and_rolling_window_expire_without_more_frames() {
        let tracker = Tracker::default();
        let now = Instant::now();
        tracker.receive(FRAME_NS, now);
        tracker.match_frame(FRAME_NS).unwrap().present(now);
        assert!(matches!(
            tracker.snapshot(now + STALE_AFTER).receive,
            Snapshot::Measuring { latest_ms: 0.0, .. }
        ));
        assert!(matches!(
            tracker
                .snapshot(now + STALE_AFTER + Duration::from_nanos(1))
                .receive,
            Snapshot::Waiting
        ));
        let later = now + SAMPLE_WINDOW + Duration::from_secs(1);
        tracker.receive(2 * FRAME_NS, later);
        tracker
            .match_frame(2 * FRAME_NS)
            .unwrap()
            .present(later + Duration::from_millis(300));
        assert!(matches!(
            tracker.snapshot(later + Duration::from_millis(300)).receive,
            Snapshot::Measuring {
                samples: 1,
                median_ms: 300.0,
                ..
            }
        ));
    }
    #[test]
    fn long_sessions_have_bounded_receipts_and_samples() {
        let tracker = Tracker::default();
        let now = Instant::now();
        for index in 0..MAX_RECEIPTS * 2 {
            let position = index as u64 * FRAME_NS;
            tracker.receive(position, now);
            tracker.match_frame(position).unwrap().present(now);
        }
        let state = tracker.state();
        assert_eq!(state.receipts.len(), MAX_RECEIPTS);
        assert!(state.waiting.is_empty());
        assert_eq!(state.samples.len(), MAX_SAMPLES);
    }

    #[test]
    fn receipt_expiration_uses_arrival_order_and_keeps_duplicate_guards() {
        let tracker = Tracker::default();
        let start = Instant::now();
        let later = start + Duration::from_secs(1);
        tracker.receive(4 * FRAME_NS, start);
        let expired = tracker.match_frame(4 * FRAME_NS).unwrap();
        tracker.receive(3 * FRAME_NS, later);
        tracker.receive(FRAME_NS, start);
        tracker.match_frame(FRAME_NS).unwrap().present(start);
        tracker.receive(2 * FRAME_NS, start);
        tracker.receive(2 * FRAME_NS, later);

        tracker.receive(5 * FRAME_NS, start + RECEIPT_LIFETIME);
        assert!(tracker.match_frame(4 * FRAME_NS).is_some());
        let after_expiration = start + RECEIPT_LIFETIME + Duration::from_nanos(1);
        tracker.receive(6 * FRAME_NS, after_expiration);
        assert!(tracker.match_frame(4 * FRAME_NS).is_none());
        assert!(tracker.match_frame(3 * FRAME_NS).is_some());
        // An expired receipt can be replaced, but cannot validate old output.
        tracker.receive(4 * FRAME_NS, after_expiration);
        expired.present(after_expiration);
        assert!(tracker.match_frame(4 * FRAME_NS).is_some());
        for position in [FRAME_NS, 2 * FRAME_NS] {
            tracker.receive(position, after_expiration);
            assert!(tracker.match_frame(position).is_none());
        }
    }

    #[test]
    fn count_eviction_and_reset_also_release_waiting_deadlines() {
        let tracker = Tracker::default();
        let start = Instant::now();
        // Arrival order is the reverse of PTS order, including count eviction.
        for index in (0..MAX_RECEIPTS * 2).rev() {
            tracker.receive(index as u64 * FRAME_NS, start);
        }
        {
            let state = tracker.state();
            assert_eq!(state.receipts.len(), MAX_RECEIPTS);
            assert_eq!(state.waiting.len(), MAX_RECEIPTS);
        }
        tracker.reset_output();
        assert_eq!(tracker.state().waiting.len(), MAX_RECEIPTS);
        tracker.reset();
        assert!(tracker.state().waiting.is_empty());
    }

    proptest! {
        #[test]
        fn waiting_deadlines_follow_receipt_lifecycle(
            operations in prop::collection::vec((0_u8..5, 0_u64..64, 0_u64..31_000), 1..200),
        ) {
            let tracker = Tracker::default();
            let mut now = Instant::now();
            for (operation, frame, elapsed_ms) in operations {
                let position = frame * FRAME_NS;
                match operation {
                    0 => tracker.receive(position, now),
                    1 => if let Some(pending) = tracker.match_frame(position) {
                        pending.present(now);
                    },
                    2 => now += Duration::from_millis(elapsed_ms),
                    3 => tracker.reset_output(),
                    4 => tracker.reset(),
                    _ => unreachable!("generated operation"),
                }
                let state = tracker.state();
                let expected: BTreeSet<_> = state.receipts.iter().filter_map(|(&position, receipt)| {
                    match receipt {
                        Receipt::Waiting(at) => Some((*at, position)),
                        Receipt::Presented | Receipt::Ambiguous => None,
                    }
                }).collect();
                prop_assert_eq!(&state.waiting, &expected);
            }
        }

        #[test]
        fn selected_quantiles_match_sorted_signed_samples(
            values in prop::collection::vec(prop::option::of(-1_000_000_i64..1_000_000), 1..=MAX_SAMPLES),
        ) {
            let now = Instant::now();
            let samples: Vec<_> = values.iter().map(|value| Sample {
                at: now,
                delay: Duration::ZERO,
                pcr_deviation_ns: value.map(i128::from),
            }).collect();
            let measured = Snapshot::summarize(&samples, now, |sample| sample.pcr_deviation_ns);
            let mut sorted: Vec<_> = values.iter().copied().flatten().collect();
            if sorted.is_empty() {
                prop_assert!(matches!(measured, Snapshot::Waiting));
            } else {
                let latest = *sorted.last().unwrap();
                sorted.sort_unstable();
                let count = sorted.len();
                const NS_PER_MS: f64 = 1_000_000.0;
                let expected = Snapshot::Measuring {
                    latest_ms: latest as f64 / NS_PER_MS,
                    median_ms: (sorted[(count - 1) / 2] + sorted[count / 2]) as f64 / (2.0 * NS_PER_MS),
                    p95_ms: sorted[(count * 95).div_ceil(100) - 1] as f64 / NS_PER_MS,
                    samples: count,
                };
                prop_assert_eq!(serde_json::to_value(measured).unwrap(), serde_json::to_value(expected).unwrap());
            }
        }
    }
}
