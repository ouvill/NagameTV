//! Owned EPG data and borrowed schedule queries; independent of Qt and clocks.
use super::schedule::{End, Resolution, Schedule, Segment};
use super::{Error, MAX_PROGRAMS};
use crate::channels::BroadcastService;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Program {
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<u16>,
    #[serde(flatten)]
    service: BroadcastService,
    pub start_at: u64,
    pub duration: u64,
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extended: Option<super::details::Extended>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video: Option<super::details::Video>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series: Option<super::details::Series>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_free: Option<bool>,
    #[serde(default, rename(deserialize = "genres"))]
    pub genre: super::genre::Genre,
    #[serde(default)]
    pub audios: Box<[crate::audio::Descriptor]>,
}
impl Program {
    pub(super) fn service(&self) -> BroadcastService {
        self.service
    }
    pub fn end(&self) -> End {
        // Mirakurun encodes EIT's unknown duration as 1 ms. Preserve the wire
        // duration for identity/details, but never assert that this is a real end.
        if self.duration == 1 {
            End::Unknown
        } else {
            End::Known(self.start_at.saturating_add(self.duration))
        }
    }
    pub fn progress(&self, now: u64) -> f64 {
        if self.duration <= 1 {
            return 0.0;
        }
        (now.saturating_sub(self.start_at) as f64 / self.duration as f64).clamp(0.0, 1.0)
    }
}

/// Owned allocation capacities, excluding allocator metadata and HTTP/Qt copies.
#[derive(Debug, Default, Clone, Copy)]
pub struct Storage {
    pub records: usize,
    pub strings: usize,
    pub audio: usize,
    pub details: usize,
    pub index: usize,
}
impl Storage {
    pub fn total(self) -> usize {
        self.records + self.strings + self.audio + self.details + self.index
    }
}

#[derive(Clone, Default)]
pub struct Snapshot {
    programs: Arc<[Program]>,
    schedules: Arc<BTreeMap<BroadcastService, Schedule>>,
}

pub(crate) fn parse(bytes: &[u8]) -> Result<Snapshot, Error> {
    let mut entries: Vec<Program> = crate::json::from_slice(bytes)?;
    if entries.len() > MAX_PROGRAMS {
        return Err(Error::TooManyPrograms {
            actual: entries.len(),
            limit: MAX_PROGRAMS,
        });
    }
    entries.sort_unstable_by_key(|p| (p.service(), p.start_at, p.id));
    // Equal identities can contain conflicting revisions. Only discard equal
    // decoded records, keeping all distinct candidates without quadratic scans.
    let mut previous = None;
    let mut seen = BTreeMap::new();
    let mut programs = Vec::with_capacity(entries.len());
    for program in entries {
        let key = (program.service(), program.start_at, program.id);
        if previous != Some(key) {
            programs.extend(std::mem::take(&mut seen).into_values());
            previous = Some(key);
        }
        seen.entry(serde_json::to_vec(&program).map_err(Error::Projection)?)
            .or_insert(program);
    }
    programs.extend(seen.into_values());
    let mut schedules = BTreeMap::new();
    let mut start = 0;
    while start < programs.len() {
        let service = programs[start].service();
        let end = start + programs[start..].partition_point(|p| p.service() == service);
        schedules.insert(service, Schedule::build(&programs, start..end));
        start = end;
    }
    Ok(Snapshot {
        programs: programs.into(),
        schedules: Arc::new(schedules),
    })
}
impl Snapshot {
    pub fn len(&self) -> usize {
        self.programs.len()
    }
    pub(super) fn schedule(&self, service: Option<BroadcastService>) -> &[Program] {
        service
            .and_then(|s| self.schedules.get(&s))
            .map_or(&[], |s| &self.programs[s.range.clone()])
    }
    pub fn program(&self, index: usize) -> &Program {
        &self.programs[index]
    }
    pub fn segments(&self, service: Option<BroadcastService>) -> &[Segment] {
        service
            .and_then(|s| self.schedules.get(&s))
            .map_or(&[], |s| s.segments.as_slice())
    }
    pub fn segment(&self, service: Option<BroadcastService>, now: u64) -> Option<&Segment> {
        service
            .and_then(|s| self.schedules.get(&s))
            .and_then(|s| s.at(now))
    }
    pub fn resolution(&self, service: Option<BroadcastService>, now: u64) -> Resolution {
        self.segment(service, now)
            .map_or(Resolution::Gap, |s| s.resolution)
    }
    pub fn candidates(&self, service: Option<BroadcastService>, now: u64) -> Vec<usize> {
        service
            .and_then(|s| self.schedules.get(&s))
            .map_or_else(Vec::new, |s| s.candidates(&self.programs, now).collect())
    }
    pub fn current(&self, service: Option<BroadcastService>, now: u64) -> Option<&Program> {
        match self.resolution(service, now) {
            Resolution::Single(index) => {
                let program = self.program(index);
                matches!(program.end(), End::Known(_)).then_some(program)
            }
            Resolution::Gap | Resolution::Conflict(_) => None,
        }
    }
    pub fn next(&self, service: Option<BroadcastService>, now: u64) -> Option<&Program> {
        let start = self
            .schedule(service)
            .iter()
            .find(|p| p.start_at > now && p.duration > 0)?
            .start_at;
        self.current(service, start)
    }
    pub fn exact(&self, service: BroadcastService, event: u16, start: u64) -> Option<&Program> {
        let mut matches = self
            .schedule(Some(service))
            .iter()
            .filter(|p| p.event_id == Some(event) && p.start_at == start);
        let result = matches.next()?;
        matches.next().is_none().then_some(result)
    }
    pub fn conflict_services(&self) -> usize {
        self.schedules
            .values()
            .filter(|s| {
                s.segments
                    .iter()
                    .any(|s| matches!(s.resolution, Resolution::Conflict(_)))
            })
            .count()
    }
    pub fn unknown_ends(&self) -> usize {
        self.programs
            .iter()
            .filter(|p| p.end() == End::Unknown)
            .count()
    }
    #[cfg(test)]
    pub fn view(
        &self,
        service: Option<BroadcastService>,
        window: super::guide::DayWindow,
    ) -> Result<String, serde_json::Error> {
        let programs: Vec<_> = self
            .schedule(service)
            .iter()
            .filter(|p| window.overlaps(p.start_at, p.duration))
            .collect();
        serde_json::to_string(&programs)
    }
    pub fn storage(&self) -> Storage {
        let record_bytes = self.programs.len() * std::mem::size_of::<Program>();
        let string_bytes: usize = self
            .programs
            .iter()
            .map(|p| {
                p.name.as_ref().map_or(0, String::capacity)
                    + p.description.as_ref().map_or(0, String::capacity)
                    + p.extended
                        .as_ref()
                        .map_or(0, super::details::Extended::string_bytes)
                    + p.video
                        .as_ref()
                        .map_or(0, super::details::Video::string_bytes)
                    + p.series
                        .as_ref()
                        .map_or(0, super::details::Series::string_bytes)
            })
            .sum();
        let audio_bytes: usize = self
            .programs
            .iter()
            .map(|program| {
                program.audios.len() * std::mem::size_of::<crate::audio::Descriptor>()
                    + program
                        .audios
                        .iter()
                        .map(crate::audio::Descriptor::heap_bytes)
                        .sum::<usize>()
            })
            .sum();
        Storage {
            records: record_bytes,
            index: self.schedules.values().map(Schedule::bytes).sum(),
            strings: string_bytes,
            audio: audio_bytes,
            details: self
                .programs
                .iter()
                .filter_map(|program| program.extended.as_ref())
                .map(super::details::Extended::record_bytes)
                .sum(),
        }
    }
}
