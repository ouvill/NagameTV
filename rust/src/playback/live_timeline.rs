//! Receive-time program history and one atomic live timeline projection.
//! Media coordinates are monotonic TS time, never UTC seek offsets.
use super::timeline::{Phase, Resume};
use crate::transport::programs::{Observation, Program};
use serde::Serialize;
use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

const NS_PER_MS: u64 = 1_000_000;
const CLOCK_TOLERANCE_MS: i64 = 1_500; // TDT has whole-second precision.
const MAX_EPOCHS: usize = 128;
const MAX_PROGRAMS: usize = 512;
const MAX_PROGRAM_BYTES: usize = 1024 * 1024;
const MIN_AXIS_MS: i64 = 1_000;
static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
struct MediaMs(i64);
impl MediaMs {
    fn from_ns(ns: u64) -> Self {
        Self((ns / NS_PER_MS) as i64)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
struct UtcMs(i64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
struct Span {
    start: MediaMs,
    end: MediaMs,
}
impl Span {
    fn contains(self, point: MediaMs) -> bool {
        point >= self.start && point < self.end
    }
    fn intersects(self, other: Self) -> bool {
        self.start < other.end && self.end > other.start
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
struct Clock {
    media: MediaMs,
    utc: UtcMs,
}
impl Clock {
    fn utc(self, media: MediaMs) -> UtcMs {
        UtcMs(self.utc.0.saturating_add(media.0 - self.media.0))
    }
    fn media(self, utc: i64) -> MediaMs {
        MediaMs(self.media.0.saturating_add(utc.saturating_sub(self.utc.0)))
    }
}
enum Announcement {
    Following,
    Current { first_seen: MediaMs },
}
struct Record {
    program: Program,
    station: String,
    provider: String,
    announcement: Announcement,
}
impl Record {
    fn id(&self, epoch: u64) -> String {
        format!(
            "{}:{}:{}:{}:{}",
            epoch,
            self.program.network_id,
            self.program.transport_stream_id,
            self.program.service_id,
            self.program.event_id
        )
    }
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.program.name.len()
            + self.program.description.len()
            + self.program.extended.len()
            + self.program.genres.len() * std::mem::size_of::<(u8, u8)>()
            + self.station.len()
            + self.provider.len()
    }
    fn span(&self, clock: Option<Clock>) -> Option<Span> {
        let start = clock?.media(self.program.start_at?);
        let duration = i64::try_from(self.program.duration?).ok()?;
        (duration > 0).then_some(Span {
            start,
            end: MediaMs(start.0.checked_add(duration)?),
        })
    }
    fn matches(&self, program: &Program) -> bool {
        let old = &self.program;
        (
            old.network_id,
            old.transport_stream_id,
            old.service_id,
            old.event_id,
        ) == (
            program.network_id,
            program.transport_stream_id,
            program.service_id,
            program.event_id,
        )
    }
}
struct Epoch {
    pcr_epoch: u64,
    serial: u64,
    span: Span,
    clock: Option<Clock>,
    records: VecDeque<Record>,
}
impl Epoch {
    fn record(&self, position: MediaMs) -> Option<&Record> {
        self.records
            .iter()
            .rev()
            .find(|record| match record.span(self.clock) {
                Some(span) => span.contains(position),
                None => match record.announcement {
                    Announcement::Following => false,
                    Announcement::Current { first_seen } => first_seen <= position,
                },
            })
    }
}

#[derive(Default)]
pub(super) struct History {
    epochs: VecDeque<Epoch>,
    // Independent of the smaller SI/clock catalog. Each disjoint span requires
    // a retained PCR anchor; Store expires both using the bounded TS index.
    coverage: VecDeque<Span>,
    observed: Option<Arc<Observation>>,
    serial: u64,
}
impl History {
    /// Called at reception, not from a UI tick or from the playback reader.
    pub fn observe(
        &mut self,
        epoch: u64,
        time_ns: u64,
        end_ns: u64,
        observation: Option<&Arc<Observation>>,
    ) {
        let position = MediaMs::from_ns(time_ns);
        let end = MediaMs::from_ns(end_ns).max(MediaMs(position.0 + 1));
        let mut clock = observation
            .and_then(|o| o.information.time)
            .map(|(ns, utc)| Clock {
                media: MediaMs::from_ns(ns),
                utc: UtcMs(utc),
            });
        let changed = match (&self.observed, observation) {
            (Some(old), Some(new)) => !Arc::ptr_eq(old, new),
            (None, None) => false,
            _ => true,
        };
        let new_pcr_epoch = self
            .epochs
            .back()
            .is_some_and(|last| last.pcr_epoch != epoch);
        if self.coverage.is_empty()
            || (new_pcr_epoch && self.coverage.back().is_some_and(|span| span.end < position))
        {
            self.coverage.push_back(Span {
                start: position,
                end,
            });
        } else if let Some(last) = self.coverage.back_mut() {
            last.end = end.max(last.end);
        }
        // An old TDT must not supply a clock for a replacement PCR epoch.
        let epoch_start = if new_pcr_epoch {
            position
        } else {
            self.epochs
                .back()
                .map_or(position, |epoch| epoch.span.start)
        };
        if self.serial > 0 && clock.is_some_and(|clock| clock.media < epoch_start) {
            clock = None;
        }
        let discontinuity = self.epochs.back().is_none_or(|last| {
            last.pcr_epoch != epoch
                || (changed
                    && last.clock.zip(clock).is_some_and(|(old, new)| {
                        old.utc(position).0.abs_diff(new.utc(position).0)
                            > CLOCK_TOLERANCE_MS as u64
                    }))
        });
        if discontinuity {
            if let Some(last) = self.epochs.back_mut() {
                last.span.end = last.span.end.min(position);
            }
            self.serial += 1;
            self.epochs.push_back(Epoch {
                pcr_epoch: epoch,
                serial: self.serial,
                span: Span {
                    start: position,
                    end,
                },
                clock,
                records: VecDeque::new(),
            });
        }
        let last = self.epochs.back_mut().expect("epoch created before update");
        last.span.end = end;
        if last.clock.is_none() {
            last.clock = clock;
        }
        if (changed || discontinuity)
            && let Some(observation) = observation
        {
            let info = &observation.information;
            // Missing SI is not a deletion of a known schedule.
            for (program, current) in info
                .current
                .iter()
                .map(|p| (p, true))
                .chain(info.next.iter().map(|p| (p, false)))
            {
                if let Some(previous) = last.records.iter_mut().find(|entry| entry.matches(program))
                {
                    previous.program = program.clone();
                    previous.station.clone_from(&info.station);
                    previous.provider.clone_from(&info.provider);
                    if current && matches!(previous.announcement, Announcement::Following) {
                        previous.announcement = Announcement::Current {
                            first_seen: position,
                        };
                    }
                } else {
                    last.records.push_back(Record {
                        program: program.clone(),
                        station: info.station.clone(),
                        provider: info.provider.clone(),
                        announcement: if current {
                            Announcement::Current {
                                first_seen: position,
                            }
                        } else {
                            Announcement::Following
                        },
                    });
                }
            }
        }
        self.observed = observation.cloned();
        if changed || discontinuity {
            self.limit();
        }
    }
    fn limit(&mut self) {
        while self.epochs.len() > MAX_EPOCHS {
            self.epochs.pop_front();
        }
        let mut count: usize = self.epochs.iter().map(|epoch| epoch.records.len()).sum();
        let mut bytes: usize = self
            .epochs
            .iter()
            .flat_map(|epoch| &epoch.records)
            .map(Record::bytes)
            .sum();
        for epoch in &mut self.epochs {
            while count > MAX_PROGRAMS || bytes > MAX_PROGRAM_BYTES {
                let Some(record) = epoch.records.pop_front() else {
                    break;
                };
                bytes -= record.bytes();
                count -= 1;
            }
        }
    }
    pub fn advance_end(&mut self, end_ns: u64) {
        if let Some(last) = self.coverage.back_mut() {
            last.end = MediaMs::from_ns(end_ns).max(last.end);
        }
        if let Some(last) = self.epochs.back_mut() {
            last.span.end = MediaMs::from_ns(end_ns).max(last.span.end);
        }
    }
    pub fn expire(&mut self, start_ns: u64) {
        let start = MediaMs::from_ns(start_ns);
        while self.coverage.front().is_some_and(|span| span.end <= start) {
            self.coverage.pop_front();
        }
        if let Some(first) = self.coverage.front_mut() {
            first.start = first.start.max(start);
        }
        while self.epochs.len() > 1 && self.epochs.front().is_some_and(|e| e.span.end <= start) {
            self.epochs.pop_front();
        }
        for epoch in &mut self.epochs {
            epoch
                .records
                .retain(|record| record.span(epoch.clock).is_none_or(|span| span.end > start));
        }
    }
    pub fn seek_target(
        &self,
        start_ns: u64,
        end_ns: u64,
        milliseconds: f64,
    ) -> Result<SeekTarget, super::timeline::Error> {
        if !milliseconds.is_finite() {
            return Err(super::timeline::Error::InvalidPosition);
        }
        let range = Span {
            start: MediaMs::from_ns(start_ns),
            end: MediaMs::from_ns(end_ns),
        };
        let available = self.retained(Some(range));
        let requested = MediaMs(milliseconds.round() as i64);
        if available.iter().any(|span| span.contains(requested)) {
            return Ok(SeekTarget {
                milliseconds,
                correction: Correction::Unchanged,
            });
        }
        // A drag can outlive its retained bytes. Gaps choose the following interval;
        // future positions choose the last received frame. Never turn UTC into a seek.
        let span = available
            .iter()
            .find(|span| span.start > requested)
            .or_else(|| available.last())
            .ok_or(super::timeline::Error::Unavailable)?;
        let target = if requested < span.start {
            super::timeline::Range::new(
                gstreamer::ClockTime::from_mseconds(span.start.0 as u64),
                gstreamer::ClockTime::from_mseconds(span.end.0 as u64),
            )
            .ok_or(super::timeline::Error::Unavailable)?
            .recovery_target()
            .mseconds() as f64
        } else {
            (span.end.0 - 1) as f64
        };
        Ok(SeekTarget {
            milliseconds: target,
            correction: Correction::Adjusted,
        })
    }
    fn epoch(&self, position: MediaMs) -> Option<&Epoch> {
        self.epochs
            .iter()
            .rev()
            .find(|epoch| epoch.span.contains(position))
    }
    fn utc(&self, position: MediaMs) -> Option<UtcMs> {
        self.epoch(position)?.clock.map(|clock| clock.utc(position))
    }
    fn selection(&self, position: MediaMs) -> Option<Selection> {
        let epoch = self.epoch(position)?;
        Selection::new(epoch, epoch.record(position)?, position)
    }
    fn retained(&self, range: Option<Span>) -> Vec<Span> {
        let Some(range) = range else {
            return Vec::new();
        };
        self.coverage
            .iter()
            .filter_map(|span| {
                let start = span.start.max(range.start);
                let end = span.end.min(range.end);
                (end > start).then_some(Span { start, end })
            })
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct Selection {
    id: String,
    title: String,
    span: Option<Span>,
    elapsed: Option<i64>,
    duration: Option<i64>,
    progress: f64,
    data: serde_json::Value,
}
impl Selection {
    fn new(epoch: &Epoch, record: &Record, position: MediaMs) -> Option<Self> {
        let program = &record.program;
        let span = record.span(epoch.clock);
        let elapsed =
            span.map(|span| (position.0 - span.start.0).clamp(0, span.end.0 - span.start.0));
        let duration = span.map(|span| span.end.0 - span.start.0);
        let mut data = serde_json::to_value(program).ok()?;
        data["source"] = "broadcast_ts".into();
        data["station"] = record.station.clone().into();
        data["provider"] = record.provider.clone().into();
        data["playbackStartMs"] = span.map(|span| span.start.0).into();
        data["playbackEndMs"] = span.map(|span| span.end.0).into();
        if !program.extended.is_empty() {
            data["description"] = format!("{}\n\n{}", program.description, program.extended)
                .trim()
                .into();
        }
        Some(Self {
            id: record.id(epoch.serial),
            title: program.name.clone(),
            span,
            elapsed,
            duration,
            progress: elapsed
                .zip(duration)
                .map_or(0.0, |(elapsed, duration)| elapsed as f64 / duration as f64),
            data,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Serialize)]
struct ProgramSpan {
    id: String,
    title: String,
    start: MediaMs,
    end: MediaMs,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ClockMode {
    Elapsed,
    Broadcast,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct Axis {
    start: MediaMs,
    end: MediaMs,
    clock: ClockMode,
    start_utc: Option<UtcMs>,
    end_utc: Option<UtcMs>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Availability {
    Disabled,
    Available,
    Expired,
    Unavailable,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum PlaybackState {
    Playing,
    Paused,
    Seeking,
    Ended,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
struct Viewing {
    position: MediaMs,
    utc: Option<UtcMs>,
    program: Option<Selection>,
    availability: Availability,
    offscreen: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
struct Live {
    position: MediaMs,
    utc: Option<UtcMs>,
    program: Option<Selection>,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Snapshot {
    session: String,
    revision: u64,
    kind: &'static str,
    axis: Axis,
    available: Vec<Span>,
    state: PlaybackState,
    viewing: Option<Viewing>,
    live: Live,
    seek_target: Option<MediaMs>,
    programs: Vec<ProgramSpan>,
    boundaries: Vec<MediaMs>,
    #[serde(skip)]
    clocks: Vec<(Span, Clock)>,
}
impl Snapshot {
    pub fn viewing_program(&self) -> (String, f64) {
        self.viewing
            .as_ref()
            .and_then(|viewing| viewing.program.as_ref())
            .map_or_else(
                || ("null".into(), 0.0),
                |program| (program.data.to_string(), program.progress),
            )
    }
    pub fn serialize(&self) -> String {
        serde_json::to_string(self).expect("finite timeline projection")
    }
}

enum Correction {
    Unchanged,
    Adjusted,
}
/// Constructed only from the current receive-side availability, consumed by Ready.
pub(super) struct SeekTarget {
    milliseconds: f64,
    correction: Correction,
}
impl SeekTarget {
    pub fn seek(self, ready: super::timeline::Ready<'_>) -> Result<(), super::timeline::Error> {
        ready.seek_live(
            self.milliseconds,
            matches!(self.correction, Correction::Adjusted),
        )
    }
}

pub(super) struct Reading {
    pub phase: Phase,
    pub position_ns: Option<u64>,
    pub target_ns: Option<u64>,
}
pub(super) struct Presenter {
    session: String,
    previous: Option<Snapshot>,
}
impl Presenter {
    pub fn new() -> Self {
        Self {
            session: NEXT_SESSION.fetch_add(1, Ordering::Relaxed).to_string(),
            previous: None,
        }
    }
    pub fn owns(&self, session: &str) -> bool {
        self.session == session
    }
    pub fn project(
        &mut self,
        history: &History,
        start_ns: u64,
        end_ns: u64,
        enabled: bool,
        reading: Reading,
    ) -> Snapshot {
        let start = MediaMs::from_ns(start_ns);
        let edge = MediaMs::from_ns(end_ns);
        let live_position = MediaMs((edge.0 - 1).max(0));
        let live = Live {
            position: edge,
            utc: history.utc(live_position).map(|utc| UtcMs(utc.0 + 1)),
            program: history.selection(live_position),
        };
        let available = history.retained(enabled.then_some(Span { start, end: edge }));
        let oldest = if enabled { start } else { live_position };
        let left = history
            .selection(oldest)
            .and_then(|selection| selection.span)
            .map_or(oldest, |span| span.start);
        let previous_end = self
            .previous
            .as_ref()
            .map_or(edge, |snapshot| snapshot.axis.end);
        let right = live
            .program
            .as_ref()
            .and_then(|program| program.span)
            .map_or(previous_end, |span| span.end)
            .max(edge)
            .max(MediaMs(left.0 + MIN_AXIS_MS));
        let first_clock = history.epoch(oldest).and_then(|epoch| epoch.clock);
        let last_clock = history.epoch(live_position).and_then(|epoch| epoch.clock);
        let axis = Axis {
            start: left,
            end: right,
            clock: if last_clock.is_some() {
                ClockMode::Broadcast
            } else {
                ClockMode::Elapsed
            },
            start_utc: first_clock.map(|clock| clock.utc(left)),
            end_utc: last_clock.map(|clock| clock.utc(right)),
        };
        let state = match reading.phase {
            Phase::Playing => PlaybackState::Playing,
            Phase::Paused => PlaybackState::Paused,
            Phase::Seeking(Resume::Playing | Resume::Paused) => PlaybackState::Seeking,
            Phase::Ended => PlaybackState::Ended,
        };
        let previous_view = self
            .previous
            .as_ref()
            .and_then(|snapshot| snapshot.viewing.as_ref());
        let frozen = state == PlaybackState::Seeking
            || self.previous.as_ref().is_some_and(|previous| {
                matches!(state, PlaybackState::Paused | PlaybackState::Ended)
                    && previous.state == state
            });
        let mut viewing = if frozen && previous_view.is_some() {
            previous_view.cloned()
        } else {
            reading
                .position_ns
                .map(MediaMs::from_ns)
                .map(|position| Viewing {
                    position,
                    utc: history.utc(position),
                    program: history.selection(position),
                    availability: Availability::Unavailable,
                    offscreen: false,
                })
        };
        if let Some(viewing) = &mut viewing {
            viewing.availability = if !enabled {
                Availability::Disabled
            } else if available.iter().any(|span| span.contains(viewing.position)) {
                Availability::Available
            } else if viewing.position < start {
                Availability::Expired
            } else {
                Availability::Unavailable
            };
            viewing.offscreen = viewing.position < left || viewing.position > right;
        }
        let mut programs = Vec::new();
        let display = Span {
            start: left,
            end: right,
        };
        for (index, epoch) in history.epochs.iter().enumerate() {
            for record in &epoch.records {
                let Some(span) = record.span(epoch.clock) else {
                    continue;
                };
                let clipped = Span {
                    start: if index == 0 {
                        span.start
                    } else {
                        span.start.max(epoch.span.start)
                    },
                    end: if index + 1 == history.epochs.len() {
                        span.end
                    } else {
                        span.end.min(epoch.span.end)
                    },
                };
                if clipped.end <= clipped.start || !clipped.intersects(display) {
                    continue;
                }
                programs.push(ProgramSpan {
                    id: record.id(epoch.serial),
                    title: record.program.name.clone(),
                    start: clipped.start,
                    end: clipped.end,
                });
            }
        }
        programs.sort_by_key(|program| program.start);
        let mut boundaries: Vec<_> = programs
            .iter()
            .flat_map(|program| [program.start, program.end])
            .filter(|position| *position > left && *position < right)
            .collect();
        boundaries.sort_unstable();
        boundaries.dedup();
        let mut snapshot = Snapshot {
            session: self.session.clone(),
            revision: self.previous.as_ref().map_or(0, |s| s.revision),
            kind: "live",
            axis,
            available,
            state,
            viewing,
            live,
            seek_target: reading.target_ns.map(MediaMs::from_ns),
            programs,
            boundaries,
            clocks: history
                .epochs
                .iter()
                .enumerate()
                .filter_map(|(index, epoch)| {
                    epoch.clock.map(|clock| {
                        (
                            Span {
                                start: if index == 0 {
                                    epoch.span.start.min(left)
                                } else {
                                    epoch.span.start
                                },
                                end: if index + 1 == history.epochs.len() {
                                    epoch.span.end.max(right)
                                } else {
                                    epoch.span.end
                                },
                            },
                            clock,
                        )
                    })
                })
                .collect(),
        };
        if self.previous.as_ref() != Some(&snapshot) {
            snapshot.revision += 1;
        }
        self.previous = Some(snapshot.clone());
        snapshot
    }
    pub fn preview(&self, session: &str, milliseconds: f64) -> String {
        if !self.owns(session) || !milliseconds.is_finite() {
            return "null".into();
        }
        let Some(snapshot) = &self.previous else {
            return "null".into();
        };
        let position = MediaMs(milliseconds.round() as i64);
        let program = snapshot
            .programs
            .iter()
            .rev()
            .find(|program| position >= program.start && position < program.end);
        // Map within the published epoch intervals, not by subtracting a global UTC.
        serde_json::json!({"position": position, "title": program.map(|p| &p.title),
            "utc": snapshot.clocks.iter().find(|(span, _)| span.contains(position)).map(|(_, clock)| clock.utc(position)),
            "available": snapshot.available.iter().any(|span| span.contains(position))})
        .to_string()
    }
}

#[cfg(test)]
mod tests;
