//! Position-scoped SI and broadcast clocks, independent of sparse seek anchors.
//! Each scan owns a non-cloneable cursor. Random reads cannot bridge an unread gap.
use super::{Information, Observation, Present, Program};
use crate::channels::BroadcastService;
use serde::Serialize;
use std::{collections::BTreeMap, sync::Arc};

const NS_PER_MS: u64 = 1_000_000;
const CLOCK_TOLERANCE_MS: u64 = 1_500;
const MAX_RUNS: usize = 64;
const MAX_CLOCK_ANCHORS: usize = 1024;
const MAX_SAMPLES: usize = 16_384;
const MAX_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Accuracy {
    Provisional,
    Indexed,
}

/// Only Catalog can issue cursors; a new cursor starts a new continuity scope.
#[derive(Default)]
pub(crate) struct ScanCursor {
    run: Option<u64>,
}
#[derive(Clone, Copy, Debug)]
struct Clock {
    media_ns: u64,
    unix_ms: i64,
}
impl Clock {
    fn utc(self, media_ns: u64) -> i64 {
        (i128::from(self.unix_ms)
            + (i128::from(media_ns) - i128::from(self.media_ns)) / i128::from(NS_PER_MS))
        .clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
    }
}
#[derive(Default)]
struct BroadcastClockMap {
    anchors: Vec<Clock>,
    candidate: Option<Clock>,
}
impl BroadcastClockMap {
    fn observe(&mut self, clock: Clock) {
        let Some(last) = self.anchors.last().copied() else {
            self.anchors.push(clock);
            return;
        };
        if clock.media_ns <= last.media_ns {
            return;
        }
        if last.utc(clock.media_ns).abs_diff(clock.unix_ms) <= CLOCK_TOLERANCE_MS {
            self.candidate = None;
            return; // Same mapping, including long TOT/TDT silences. No expiry.
        }
        match self.candidate {
            Some(candidate)
                if candidate.media_ns < clock.media_ns
                    && candidate.utc(clock.media_ns).abs_diff(clock.unix_ms)
                        <= CLOCK_TOLERANCE_MS =>
            {
                self.anchors.push(candidate);
                self.candidate = None;
            }
            Some(_) | None => self.candidate = Some(clock),
        }
    }
    fn at(&self, position: u64) -> Option<Clock> {
        let index = self
            .anchors
            .partition_point(|clock| clock.media_ns <= position);
        self.anchors.get(index.saturating_sub(1)).copied()
    }
}
struct Sample {
    position: u64,
    offset: u64,
    information: Arc<Information>,
}
struct Run {
    epoch: u64,
    accuracy: Accuracy,
    start: u64,
    end: u64,
    last_offset: u64,
    samples: Vec<Sample>,
    clocks: BroadcastClockMap,
}
impl Run {
    fn contains(&self, position: u64) -> bool {
        self.start <= position && position < self.end
    }
    fn sample(&self, position: u64) -> Option<&Sample> {
        let count = self
            .samples
            .partition_point(|sample| sample.position <= position);
        count
            .checked_sub(1)
            .and_then(|index| self.samples.get(index))
    }
}
pub(crate) struct ScanPoint<'a> {
    pub accuracy: Accuracy,
    pub epoch: u64,
    pub offset: u64,
    pub position: u64,
    pub end: u64,
    pub observation: Option<&'a Observation>,
}
#[derive(Default)]
pub(crate) struct Catalog {
    serial: u64,
    runs: BTreeMap<u64, Run>,
    samples: usize,
    bytes: usize,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Status {
    Pending,
    Available,
    Unavailable,
    Failed,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Available => "available",
            Self::Unavailable => "unavailable",
            Self::Failed => "failed",
        }
    }
}
/// A clock reading carries its scope. Callers cannot invent a mapped interval.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ClockReading {
    clock: Clock,
    start: u64,
    end: u64,
    pub epoch: u64,
}
impl ClockReading {
    /// Explicit recording metadata, scoped to the demuxer's measured duration.
    /// This does not claim that an edited file still has a continuous clock.
    pub(crate) fn recording(start_ms: i64, duration_ns: u64) -> Option<Self> {
        if start_ms <= 0 || duration_ns == 0 {
            return None;
        }
        start_ms.checked_add(i64::try_from(duration_ns / NS_PER_MS).ok()?)?;
        Some(Self {
            clock: Clock {
                media_ns: 0,
                unix_ms: start_ms,
            },
            start: 0,
            end: duration_ns,
            epoch: 0,
        })
    }
    pub fn key(self) -> String {
        format!(
            "{}:{}:{}",
            self.epoch, self.clock.media_ns, self.clock.unix_ms
        )
    }
    /// The exclusive end is verified by the same scan as the last byte/frame.
    pub fn utc_range(self) -> (i64, i64) {
        (self.clock.utc(self.start), self.clock.utc(self.end))
    }
    pub fn utc(self, position: u64) -> Option<i64> {
        (self.start <= position && position < self.end).then(|| self.clock.utc(position))
    }
    pub fn media(self, utc_ms: i64) -> Option<u64> {
        let ns = i128::from(self.clock.media_ns)
            + (i128::from(utc_ms) - i128::from(self.clock.unix_ms)) * i128::from(NS_PER_MS);
        let ns = u64::try_from(ns).ok()?;
        (self.start <= ns && ns < self.end).then_some(ns)
    }
    pub fn agrees(self, other: Self, position: u64) -> bool {
        self.epoch == other.epoch
            && self.clock.utc(position).abs_diff(other.clock.utc(position)) <= CLOCK_TOLERANCE_MS
    }
    pub fn range(self) -> (u64, u64) {
        (self.start, self.end)
    }
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct BroadcastSpan {
    pub clock: ClockReading,
    pub service: Option<BroadcastService>,
}
pub(crate) struct View {
    pub status: Status,
    pub program: Option<Program>,
    pub next: Option<Program>,
    pub station: String,
    pub provider: String,
    pub service: Option<BroadcastService>,
    pub clock: Option<ClockReading>,
}
impl Default for View {
    fn default() -> Self {
        Self {
            status: Status::Pending,
            program: None,
            next: None,
            station: String::new(),
            provider: String::new(),
            service: None,
            clock: None,
        }
    }
}
impl View {
    #[cfg(test)]
    pub fn presentation(&self, position: u64) -> (String, f64) {
        let (data, progress) = self.project(position, &mut super::presentation::Cache::default());
        (
            data.as_ref()
                .map_or("null", super::presentation::Data::json)
                .to_owned(),
            progress,
        )
    }
    pub fn project(
        &self,
        position: u64,
        cache: &mut super::presentation::Cache,
    ) -> (Option<super::presentation::Data>, f64) {
        let Some(program) = &self.program else {
            return (None, 0.0);
        };
        let utc = self.clock.and_then(|clock| clock.utc(position));
        let progress = utc
            .zip(program.start_at)
            .zip(program.duration.filter(|duration| *duration > 0))
            .map(|((utc, start), duration)| {
                ((i128::from(utc) - i128::from(start)) as f64 / duration as f64).clamp(0., 1.)
            });
        let range = utc
            .zip(program.start_at)
            .zip(program.duration.filter(|duration| *duration > 0))
            .and_then(|((utc, start), duration)| {
                let start = i128::from(position / NS_PER_MS) + i128::from(start) - i128::from(utc);
                Some((
                    i64::try_from(start).ok()?,
                    i64::try_from(start + i128::from(duration)).ok()?,
                ))
            });
        let data = cache.project(
            program.clone(),
            self.station.clone(),
            self.provider.clone(),
            range,
            progress.is_some(),
        );
        (Some(data), progress.unwrap_or(0.))
    }
}
impl Catalog {
    /// Enumerate only scanned clock/service scopes. Neither an estimated file
    /// duration nor matching clocks on opposite sides can bridge an unread gap.
    pub fn broadcast_spans(&self, start: u64, end: u64) -> Vec<BroadcastSpan> {
        let mut boundaries = std::collections::BTreeSet::from([start, end]);
        for run in self
            .runs
            .values()
            .filter(|run| run.start < end && run.end > start)
        {
            boundaries.insert(run.start.max(start));
            boundaries.insert(run.end.min(end));
            for anchor in &run.clocks.anchors {
                if start < anchor.media_ns && anchor.media_ns < end {
                    boundaries.insert(anchor.media_ns);
                }
            }
            let mut previous = None;
            for sample in &run.samples {
                let service = sample.information.service;
                if service != previous && start < sample.position && sample.position < end {
                    boundaries.insert(sample.position);
                }
                previous = service;
            }
        }
        let boundaries: Vec<_> = boundaries.into_iter().collect();
        let mut spans: Vec<BroadcastSpan> = Vec::new();
        for pair in boundaries.windows(2) {
            let view = self.view(pair[0]);
            let Some(mut clock) = view.clock else {
                continue;
            };
            clock.start = clock.start.max(pair[0]);
            clock.end = clock.end.min(pair[1]);
            if clock.start >= clock.end {
                continue;
            }
            if let Some(last) = spans.last_mut()
                && last.service == view.service
                && last.clock.end == clock.start
                && last.clock.key() == clock.key()
            {
                last.clock.end = clock.end;
            } else {
                spans.push(BroadcastSpan {
                    clock,
                    service: view.service,
                });
            }
        }
        spans
    }
    pub fn observe(&mut self, cursor: &mut ScanCursor, point: ScanPoint<'_>) {
        let ScanPoint {
            accuracy,
            epoch,
            offset,
            position,
            end,
            observation,
        } = point;
        let existing = cursor
            .run
            .and_then(|id| self.runs.get(&id))
            .is_some_and(|run| {
                run.epoch == epoch && run.last_offset <= offset && position >= run.start
            });
        if !existing {
            self.serial += 1;
            cursor.run = Some(self.serial);
            self.runs.insert(
                self.serial,
                Run {
                    epoch,
                    accuracy,
                    start: position,
                    end: end.max(position.saturating_add(1)),
                    last_offset: offset,
                    samples: Vec::new(),
                    clocks: Default::default(),
                },
            );
        }
        let id = cursor.run.expect("scan created above");
        let run = self.runs.get_mut(&id).expect("scan exists");
        run.end = run.end.max(end);
        run.last_offset = offset;
        if let Some(observation) = observation {
            let mut information = observation.information.clone();
            if let Present::Event(program) = &mut information.current
                && let Some(previous) = run.samples.last().and_then(|last| {
                    last.information
                        .current
                        .iter()
                        .chain(last.information.next.iter())
                        .find(|old| {
                            (
                                old.network_id,
                                old.transport_stream_id,
                                old.service_id,
                                old.event_id,
                                old.start_at,
                            ) == (
                                program.network_id,
                                program.transport_stream_id,
                                program.service_id,
                                program.event_id,
                                program.start_at,
                            )
                        })
                })
            {
                // A valid EIT can omit optional descriptors. That is not an
                // explicit empty present section or evidence of a new event.
                if program.name.is_empty() {
                    program.name.clone_from(&previous.name);
                }
                if program.description.is_empty() {
                    program.description.clone_from(&previous.description);
                }
                if program.extended.is_empty() {
                    program.extended.clone_from(&previous.extended);
                }
                if program.genres.is_empty() {
                    program.genres.clone_from(&previous.genres);
                }
            }
            let info = &information;
            if let Some((media_ns, unix_ms)) = info.time
                && media_ns >= run.start
                && media_ns < run.end
            {
                run.clocks.observe(Clock { media_ns, unix_ms });
                if run.clocks.anchors.len() > MAX_CLOCK_ANCHORS {
                    run.clocks.anchors.remove(0);
                    run.start = run.start.max(run.clocks.anchors[0].media_ns);
                }
            }
            if run.samples.last().is_none_or(|last| {
                last.information.current != info.current
                    || last.information.next != info.next
                    || last.information.service != info.service
                    || last.information.station != info.station
                    || last.information.provider != info.provider
            }) {
                self.bytes += information_bytes(info);
                self.samples += 1;
                run.samples.push(Sample {
                    position,
                    offset,
                    information: Arc::new(info.clone()),
                });
            }
        }
        self.limit(id);
    }
    pub fn extend(&mut self, cursor: &ScanCursor, end: u64) {
        if let Some(run) = cursor.run.and_then(|id| self.runs.get_mut(&id)) {
            run.end = run.end.max(end);
        }
    }
    fn limit(&mut self, current: u64) {
        while self.runs.len() > MAX_RUNS || self.samples > MAX_SAMPLES || self.bytes > MAX_BYTES {
            if let Some(id) = self
                .runs
                .iter()
                .filter(|(id, _)| **id != current)
                .min_by_key(|(id, run)| (run.accuracy, run.end.saturating_sub(run.start), **id))
                .map(|(id, _)| *id)
            {
                if let Some(run) = self.runs.remove(&id) {
                    self.uncharge(&run);
                }
            } else {
                let run = self.runs.get_mut(&current).expect("current scan");
                if run.samples.len() <= 1 {
                    break;
                }
                let removed = run.samples.remove(0);
                self.samples -= 1;
                self.bytes = self
                    .bytes
                    .saturating_sub(information_bytes(&removed.information));
                run.start = run.samples[0].position;
                let first = run
                    .clocks
                    .anchors
                    .partition_point(|clock| clock.media_ns < run.start)
                    .saturating_sub(1);
                run.clocks.anchors.drain(..first);
            }
        }
    }
    fn uncharge(&mut self, run: &Run) {
        self.samples -= run.samples.len();
        self.bytes = self.bytes.saturating_sub(
            run.samples
                .iter()
                .map(|sample| information_bytes(&sample.information))
                .sum::<usize>(),
        );
    }
    pub fn clear_provisional(&mut self) {
        let ids: Vec<_> = self
            .runs
            .iter()
            .filter(|(_, run)| run.accuracy == Accuracy::Provisional)
            .map(|(&id, _)| id)
            .collect();
        for id in ids {
            if let Some(run) = self.runs.remove(&id) {
                self.uncharge(&run);
            }
        }
    }
    pub fn expire(&mut self, position: u64) {
        let ids: Vec<_> = self
            .runs
            .iter()
            .filter(|(_, run)| run.end <= position)
            .map(|(&id, _)| id)
            .collect();
        for id in ids {
            if let Some(run) = self.runs.remove(&id) {
                self.uncharge(&run);
            }
        }
        for run in self.runs.values_mut() {
            run.start = run.start.max(position);
            let count = run
                .samples
                .partition_point(|sample| sample.position < position)
                .saturating_sub(1);
            for sample in run.samples.drain(..count) {
                self.samples -= 1;
                self.bytes = self
                    .bytes
                    .saturating_sub(information_bytes(&sample.information));
            }
            let count = run
                .clocks
                .anchors
                .partition_point(|clock| clock.media_ns < position)
                .saturating_sub(1);
            run.clocks.anchors.drain(..count);
        }
    }
    pub fn view(&self, position: u64) -> View {
        let mut view = View::default();
        let runs: Vec<_> = self
            .runs
            .values()
            .filter(|run| run.contains(position))
            .collect();
        // Once exact indexing covers a position it invalidates any provisional
        // clock/identity there, including an apparently better populated probe.
        let accuracy = runs.iter().map(|run| run.accuracy).max();
        let runs: Vec<_> = runs
            .into_iter()
            .filter(|run| Some(run.accuracy) == accuracy)
            .collect();
        let epoch = runs.iter().map(|run| run.epoch).max();
        let runs: Vec<_> = runs
            .into_iter()
            .filter(|run| Some(run.epoch) == epoch)
            .collect();
        let best = runs
            .iter()
            .filter_map(|run| {
                let sample = run
                    .samples
                    .iter()
                    .rev()
                    .find(|sample| {
                        sample.position <= position
                            && !matches!(sample.information.current, Present::Unknown)
                    })
                    .or_else(|| {
                        run.samples
                            .iter()
                            .find(|sample| !matches!(sample.information.current, Present::Unknown))
                            .filter(|sample| {
                                let Some(program) = sample.information.current.as_ref() else {
                                    return false;
                                };
                                let Some(utc) =
                                    run.clocks.at(position).map(|clock| clock.utc(position))
                                else {
                                    return false;
                                };
                                program.start_at.zip(program.duration).is_some_and(
                                    |(start, duration)| {
                                        utc >= start
                                            && i128::from(utc)
                                                < i128::from(start) + i128::from(duration)
                                    },
                                )
                            })
                    });
                sample.map(|sample| (*run, sample))
            })
            .filter(|(_, sample)| !matches!(sample.information.current, Present::Unknown))
            .max_by_key(|(run, sample)| (run.accuracy, sample.offset));
        let selected_epoch = best.map(|(run, _)| run.epoch).or_else(|| {
            runs.iter()
                .max_by_key(|run| run.accuracy)
                .map(|run| run.epoch)
        });
        let clocks: Vec<_> = runs
            .iter()
            .filter(|run| Some(run.epoch) == selected_epoch)
            .filter_map(|run| {
                let clock = run.clocks.at(position)?;
                let first = run
                    .clocks
                    .anchors
                    .first()
                    .is_some_and(|first| first.media_ns == clock.media_ns);
                let start = if first || clock.media_ns <= run.start {
                    run.start
                } else {
                    clock.media_ns
                };
                let end = run
                    .clocks
                    .anchors
                    .iter()
                    .find(|next| next.media_ns > position)
                    .map_or(run.end, |next| next.media_ns.min(run.end));
                Some(ClockReading {
                    clock,
                    start,
                    end,
                    epoch: run.epoch,
                })
            })
            .collect();
        if let Some(newest) = clocks.iter().max_by_key(|reading| reading.clock.media_ns) {
            // A reader seeded three seconds before a seek must not hide the
            // earlier part of the same established clock (in-flight comments).
            view.clock = clocks
                .iter()
                .filter(|reading| reading.agrees(*newest, position))
                .min_by_key(|reading| (reading.start, std::cmp::Reverse(reading.end)))
                .copied();
        }
        // Service/SDT can be acquired without an EIT title, including while paused.
        for run in runs.iter().filter(|run| Some(run.epoch) == selected_epoch) {
            let sample = run.sample(position).or_else(|| run.samples.first());
            if let Some(sample) = sample {
                if sample.information.service.is_some() {
                    view.service = sample.information.service;
                }
                if !sample.information.station.is_empty() {
                    view.station.clone_from(&sample.information.station);
                }
                if !sample.information.provider.is_empty() {
                    view.provider.clone_from(&sample.information.provider);
                }
            }
        }
        if let Some((run, sample)) = best {
            view.next.clone_from(&sample.information.next);
            match &sample.information.current {
                Present::Event(program) => {
                    // Corrections to one uninterrupted present event also enrich
                    // its earlier observed interval. Stop at a different event or
                    // explicit empty section; reused event IDs are not merged.
                    let mut corrected = program;
                    for later in run
                        .samples
                        .iter()
                        .filter(|later| later.offset > sample.offset)
                    {
                        let Present::Event(candidate) = &later.information.current else {
                            break;
                        };
                        if (
                            candidate.network_id,
                            candidate.transport_stream_id,
                            candidate.service_id,
                            candidate.event_id,
                        ) != (
                            program.network_id,
                            program.transport_stream_id,
                            program.service_id,
                            program.event_id,
                        ) {
                            break;
                        }
                        if let Some((old, new)) = program.start_at.zip(candidate.start_at)
                            && old.abs_diff(new)
                                > program
                                    .duration
                                    .unwrap_or(0)
                                    .max(candidate.duration.unwrap_or(0))
                        {
                            break;
                        }
                        corrected = candidate;
                        view.next.clone_from(&later.information.next);
                    }
                    let mut result = corrected.clone();
                    // Text corrections can enrich an earlier interval. Audio format
                    // changes belong to their observed position, even within one event.
                    result.audios.clone_from(&program.audios);
                    view.program = Some(result);
                    view.status = Status::Available;
                }
                Present::Empty => view.status = Status::Unavailable,
                Present::Unknown => unreachable!("filtered above"),
            }
        }
        view
    }
}
fn information_bytes(info: &Information) -> usize {
    std::mem::size_of::<Information>()
        + info.station.len()
        + info.provider.len()
        + info
            .current
            .iter()
            .chain(info.next.iter())
            .map(|program| {
                std::mem::size_of::<Program>()
                    + program.name.len()
                    + program.description.len()
                    + program.extended.len()
                    + program.genres.len() * std::mem::size_of::<(u8, u8)>()
                    + program.audio_heap_bytes()
            })
            .sum::<usize>()
}

#[cfg(test)]
mod tests;
