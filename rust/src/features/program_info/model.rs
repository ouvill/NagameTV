//! Owned EPG data and borrowed schedule queries; independent of Qt and clocks.
use super::{Error, MAX_PROGRAMS};
use crate::channels::BroadcastService;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
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
    #[serde(default, rename(deserialize = "genres"))]
    pub genre: super::genre::Genre,
    #[serde(default, skip_serializing)]
    pub audios: Box<[crate::audio::Descriptor]>,
}
impl Program {
    fn service(&self) -> BroadcastService {
        self.service
    }
    fn contains(&self, now: u64) -> bool {
        self.start_at <= now && now < self.start_at.saturating_add(self.duration)
    }
    pub fn progress(&self, now: u64) -> f64 {
        if self.duration == 0 {
            return 0.0;
        }
        (now.saturating_sub(self.start_at) as f64 / self.duration as f64).clamp(0.0, 1.0)
    }
}

#[derive(Default)]
pub struct Snapshot(Vec<Program>);

pub(super) fn parse(bytes: &[u8]) -> Result<Snapshot, Error> {
    let mut entries: Vec<Program> = serde_json::from_slice(bytes)?;
    if entries.len() > MAX_PROGRAMS {
        return Err(Error::TooManyPrograms {
            actual: entries.len(),
            limit: MAX_PROGRAMS,
        });
    }
    entries.sort_unstable_by_key(|p| (p.service(), p.start_at, p.id));
    entries.dedup_by_key(|p| (p.service(), p.start_at, p.id));
    Ok(Snapshot(entries))
}
impl Snapshot {
    pub fn len(&self) -> usize {
        self.0.len()
    }
    fn schedule(&self, service: Option<BroadcastService>) -> &[Program] {
        let Some(service) = service else {
            return &[];
        };
        let start = self.0.partition_point(|p| p.service() < service);
        let end = self.0.partition_point(|p| p.service() <= service);
        &self.0[start..end]
    }
    pub fn current(&self, service: Option<BroadcastService>, now: u64) -> Option<&Program> {
        let schedule = self.schedule(service);
        // Match main: newest start wins for overlapping entries; do not resurrect
        // an older entry after a newer one has finished.
        let index = schedule
            .partition_point(|p| p.start_at <= now)
            .checked_sub(1)?;
        schedule.get(index).filter(|p| p.contains(now))
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
    pub fn grid_view(
        &self,
        channels: &[crate::channels::Channel],
        window: super::guide::DayWindow,
    ) -> Result<String, serde_json::Error> {
        #[derive(Serialize)]
        struct Column<'a> {
            index: usize,
            programs: Vec<&'a Program>,
        }
        // Borrow records from the one snapshot; only the selected calendar day crosses Qt.
        let columns: Vec<_> = channels
            .iter()
            .enumerate()
            .map(|(index, channel)| Column {
                index,
                programs: self
                    .schedule(channel.broadcast)
                    .iter()
                    .filter(|p| window.overlaps(p.start_at, p.duration))
                    .collect(),
            })
            .collect();
        serde_json::to_string(&columns)
    }
    pub fn record_storage(&self) {
        let record_bytes = self.0.capacity() * std::mem::size_of::<Program>();
        let string_bytes: usize = self
            .0
            .iter()
            .map(|p| {
                p.name.as_ref().map_or(0, String::capacity)
                    + p.description.as_ref().map_or(0, String::capacity)
            })
            .sum();
        let audio_bytes: usize = self
            .0
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
        eprintln!(
            "EPG_MEMORY programs={} record_capacity_bytes={record_bytes} string_capacity_bytes={string_bytes} audio_heap_bytes={audio_bytes} snapshot_capacity_bytes={}",
            self.len(),
            record_bytes + string_bytes + audio_bytes
        );
    }
}
