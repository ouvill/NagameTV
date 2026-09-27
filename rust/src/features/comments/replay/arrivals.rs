//! Temporary display times for posts received while watching the live edge.
//! Session history keeps the original comments and posting timestamps.
use super::{ClockReading, Context, FORWARD_SECONDS, Record, Source};
use std::{collections::BTreeMap, time::Duration};
use viewer_comments::{
    Phase,
    danmaku::{MAX_LIFETIME, MAX_TIMELINE_BYTES, MAX_TIMELINE_COMMENTS},
};

struct Arrival {
    record: Record,
    timing: ArrivalTime,
}

enum ArrivalTime {
    WaitingForVideo { received: u64 },
    Ready { start: u64 },
}
impl ArrivalTime {
    fn resolve(clock: ClockReading, position: u64, micros: u64) -> Self {
        let utc_ms = (micros / 1000) as i64;
        match clock
            .media(utc_ms)
            .and_then(|ns| ns.checked_add(micros % 1000 * 1000))
        {
            Some(scheduled) => Self::Ready {
                start: scheduled.max(position),
            },
            None if utc_ms < clock.utc_range().0 => Self::Ready { start: position },
            None => Self::WaitingForVideo { received: position },
        }
    }

    fn start(&self) -> Option<u64> {
        match *self {
            Self::WaitingForVideo { .. } => None,
            Self::Ready { start } => Some(start),
        }
    }
}

#[derive(Default)]
pub(super) struct LiveArrivals {
    records: BTreeMap<u64, Arrival>,
}

/// A short-lived right to display this poll's posts as live arrivals.
pub(super) struct LiveReception<'a> {
    arrivals: &'a mut LiveArrivals,
    clock: ClockReading,
    position: u64,
}

impl LiveReception<'_> {
    pub(super) fn receive(self, comments: &[Record]) -> bool {
        self.arrivals.receive(comments, self.clock, self.position)
    }
}

impl LiveArrivals {
    pub(super) fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub(super) fn at_edge(
        &mut self,
        context: &Context,
        seeking: bool,
        clock: ClockReading,
        position: u64,
    ) -> Option<LiveReception<'_>> {
        (context.enabled
            && context.display
            && !context.paused
            && !seeking
            && matches!(context.source_range, Source::Live { at_edge: true, .. }))
        .then_some(LiveReception {
            arrivals: self,
            clock,
            position,
        })
    }

    pub(super) fn clear(&mut self) {
        self.records.clear();
    }

    pub(super) fn mark_own(&mut self, ids: &[u64]) -> bool {
        let mut changed = false;
        for id in ids {
            if let Some(arrival) = self.records.get_mut(id)
                && !arrival.record.own
            {
                arrival.record.own = true;
                changed = true;
            }
        }
        changed
    }

    pub(super) fn advance(&mut self, clock: ClockReading, position: u64) -> bool {
        let before = self.records.len();
        let mut resolved = false;
        self.records.retain(|_, arrival| {
            if matches!(arrival.timing, ArrivalTime::WaitingForVideo { .. })
                && let Some(micros) = arrival.record.comment.timestamp_micros
            {
                let timing = ArrivalTime::resolve(clock, position, micros);
                if matches!(timing, ArrivalTime::Ready { .. }) {
                    arrival.timing = timing;
                    resolved = true;
                }
            }
            let (start, lifetime) = match arrival.timing {
                ArrivalTime::Ready { start } => (start, MAX_LIFETIME),
                ArrivalTime::WaitingForVideo { received } => {
                    (received, Duration::from_secs(FORWARD_SECONDS as u64))
                }
            };
            position.saturating_sub(start) < lifetime.as_nanos() as u64
        });
        resolved || before != self.records.len()
    }

    fn receive(&mut self, comments: &[Record], clock: ClockReading, position: u64) -> bool {
        let before = self.records.len();
        let mut bytes: usize = self
            .records
            .values()
            .map(|a| std::mem::size_of::<Arrival>() + a.record.comment.text.len())
            .sum();
        for record in comments {
            let comment = &record.comment;
            let Some(micros) = comment.timestamp_micros else {
                continue;
            };
            if comment.phase != Phase::Live {
                continue;
            }
            let size = std::mem::size_of::<Arrival>() + comment.text.len();
            if self.records.contains_key(&record.id)
                || self.records.len() >= MAX_TIMELINE_COMMENTS
                || bytes.saturating_add(size) > MAX_TIMELINE_BYTES
            {
                continue;
            }
            self.records.insert(
                record.id,
                Arrival {
                    record: record.clone(),
                    // Do not extrapolate beyond the verified video clock.
                    timing: ArrivalTime::resolve(clock, position, micros),
                },
            );
            bytes += size;
        }
        before != self.records.len()
    }

    pub(super) fn contains(&self, id: u64) -> bool {
        self.start(id).is_some()
    }

    pub(super) fn records(&self) -> Vec<&Record> {
        self.records
            .values()
            .filter(|arrival| arrival.timing.start().is_some())
            .map(|arrival| &arrival.record)
            .collect()
    }

    pub(super) fn start(&self, id: u64) -> Option<u64> {
        self.records
            .get(&id)
            .and_then(|arrival| arrival.timing.start())
    }
}
