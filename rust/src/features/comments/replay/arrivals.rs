//! Temporary display times for posts received while watching the live edge.
//! Persistence always receives the original comments and posting timestamps.
use super::{ClockReading, Comment, Context, FORWARD_SECONDS, Record, RecordOrigin, Source};
use std::{
    collections::{BTreeMap, HashMap},
    time::Duration,
};
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
    serial: u64,
    records: BTreeMap<u64, Arrival>,
    identities: HashMap<String, u64>,
}

/// A short-lived right to display this poll's posts as live arrivals.
pub(super) struct LiveReception<'a> {
    arrivals: &'a mut LiveArrivals,
    clock: ClockReading,
    position: u64,
}

impl LiveReception<'_> {
    pub(super) fn receive(self, comments: &[(Comment, bool)]) -> bool {
        self.arrivals.receive(comments, self.clock, self.position)
    }
}

fn identity(comment: &Comment) -> String {
    serde_json::to_string(&(
        comment.source_id,
        comment.timestamp_micros,
        comment.origin,
        &comment.text,
        comment.style,
    ))
    .expect("live comment identity")
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
        self.identities.clear();
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
        self.identities
            .retain(|_, id| self.records.contains_key(id));
        resolved || before != self.records.len()
    }

    fn receive(
        &mut self,
        comments: &[(Comment, bool)],
        clock: ClockReading,
        position: u64,
    ) -> bool {
        let before = self.records.len();
        let mut bytes: usize = self.identities.keys().map(String::len).sum::<usize>()
            + self
                .records
                .values()
                .map(|a| a.record.comment.text.len())
                .sum::<usize>();
        for (comment, own) in comments {
            let Some(micros) = comment.timestamp_micros else {
                continue;
            };
            if comment.phase != Phase::Live {
                continue;
            }
            let key = identity(comment);
            let size = key.len() + comment.text.len();
            if self.identities.contains_key(&key)
                || self.records.len() >= MAX_TIMELINE_COMMENTS
                || bytes.saturating_add(size) > MAX_TIMELINE_BYTES
            {
                continue;
            }
            // SQLite assigns positive signed IDs. The upper unsigned half is
            // reserved here for transient arrivals, never written to the DB.
            let Some(id) = u64::MAX
                .checked_sub(self.serial)
                .filter(|id| *id > i64::MAX as u64)
            else {
                break;
            };
            self.serial += 1;
            self.records.insert(
                id,
                Arrival {
                    record: Record {
                        id,
                        comment: comment.clone(),
                        own: *own,
                        origin: RecordOrigin::Live,
                        media_ms: None,
                    },
                    // Do not extrapolate beyond the verified video clock.
                    timing: ArrivalTime::resolve(clock, position, micros),
                },
            );
            self.identities.insert(key, id);
            bytes += size;
        }
        before != self.records.len()
    }

    pub(super) fn contains(&self, comment: &Comment) -> bool {
        !self.is_empty()
            && self
                .identities
                .get(&identity(comment))
                .is_some_and(|id| self.start(*id).is_some())
    }

    pub(super) fn records(&self) -> Vec<&Record> {
        self.records
            .values()
            .rev()
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
