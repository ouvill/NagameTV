//! Owned EPG data and borrowed schedule queries; independent of Qt and clocks.
use super::Error;
use super::schedule::{End, Resolution, Schedule, Segment};
use crate::channels::BroadcastService;
use serde::Serialize;
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Program {
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<u16>,
    #[serde(flatten)]
    pub(super) service: BroadcastService,
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
    pub source: usize,
}
impl Storage {
    pub fn total(self) -> usize {
        self.records + self.strings + self.audio + self.details + self.index + self.source
    }
}

#[derive(Clone, Default)]
pub struct Snapshot {
    source: Arc<[super::wire::Record]>,
    programs: Arc<[Program]>,
    schedules: Arc<BTreeMap<BroadcastService, Schedule>>,
}

pub(crate) fn parse(bytes: &[u8]) -> Result<Snapshot, Error> {
    Snapshot::from_records(super::wire::records(bytes)?)
}

pub(super) fn parse_service(bytes: &[u8], service: BroadcastService) -> Result<Snapshot, Error> {
    Snapshot::from_records(super::wire::records_for(bytes, Some(service))?)
}
impl Snapshot {
    fn from_records(records: Vec<super::wire::Record>) -> Result<Self, Error> {
        if records.len() > super::MAX_PROGRAMS {
            return Err(Error::TooManyPrograms {
                actual: records.len(),
                limit: super::MAX_PROGRAMS,
            });
        }
        let mut entries = super::wire::resolve(&records)?;
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
        let snapshot = Snapshot {
            source: records.into(),
            programs: programs.into(),
            schedules: Arc::new(schedules),
        };
        let actual = snapshot.storage().total();
        if actual > super::MAX_SNAPSHOT_BYTES {
            return Err(Error::TooMuchMemory {
                actual,
                limit: super::MAX_SNAPSHOT_BYTES,
            });
        }
        Ok(snapshot)
    }
    pub(super) fn same_programs(&self, other: &Self) -> bool {
        self.programs == other.programs
    }
    pub(super) fn apply(
        &self,
        changes: impl Iterator<Item = viewer_epg_events::ProgramChange>,
    ) -> Result<Option<Self>, Error> {
        use viewer_epg_events::ProgramChange;
        let mut updates = BTreeMap::new();
        for change in changes {
            let id = change.id();
            let record = match change {
                ProgramChange::Upsert { data, .. } => {
                    super::wire::parse_one(data.get().as_bytes(), id)?
                }
                ProgramChange::Remove(_) => None,
            };
            let mut previous = self.source.iter().filter(|record| record.program.id == id);
            if previous.next() != record.as_ref() || previous.next().is_some() {
                updates.insert(id, record);
            }
        }
        if updates.is_empty() {
            return Ok(None);
        }
        let mut records: Vec<_> = self
            .source
            .iter()
            .filter(|record| !updates.contains_key(&record.program.id))
            .cloned()
            .collect();
        records.extend(updates.into_values().flatten());
        Self::from_records(records).map(Some)
    }
    pub(super) fn replace_service(
        &self,
        service: BroadcastService,
        incoming: Self,
    ) -> Result<Self, Error> {
        if incoming
            .source
            .iter()
            .any(|record| record.program.service != service)
        {
            return Err(Error::WrongService);
        }
        let mut records: Vec<_> = self
            .source
            .iter()
            .filter(|record| record.program.service != service)
            .cloned()
            .collect();
        records.extend(incoming.source.iter().cloned());
        Self::from_records(records)
    }

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
    pub(super) fn source_string_bytes(&self) -> usize {
        self.source
            .iter()
            .map(|record| record.program.storage().strings)
            .sum()
    }
    pub fn storage(&self) -> Storage {
        let mut storage = Storage {
            index: self.schedules.values().map(Schedule::bytes).sum(),
            source: self
                .source
                .iter()
                .map(super::wire::Record::storage_bytes)
                .sum(),
            ..Storage::default()
        };
        for program in self.programs.iter() {
            let item = program.storage();
            storage.records += item.records;
            storage.strings += item.strings;
            storage.audio += item.audio;
            storage.details += item.details;
        }
        storage
    }
}
impl Program {
    pub(super) fn storage(&self) -> Storage {
        Storage {
            records: std::mem::size_of::<Self>(),
            strings: self.name.as_ref().map_or(0, String::capacity)
                + self.description.as_ref().map_or(0, String::capacity)
                + self
                    .extended
                    .as_ref()
                    .map_or(0, super::details::Extended::string_bytes)
                + self
                    .video
                    .as_ref()
                    .map_or(0, super::details::Video::string_bytes)
                + self
                    .series
                    .as_ref()
                    .map_or(0, super::details::Series::string_bytes),
            audio: self.audios.len() * std::mem::size_of::<crate::audio::Descriptor>()
                + self
                    .audios
                    .iter()
                    .map(crate::audio::Descriptor::heap_bytes)
                    .sum::<usize>(),
            details: self
                .extended
                .as_ref()
                .map_or(0, super::details::Extended::record_bytes),
            ..Storage::default()
        }
    }
}
