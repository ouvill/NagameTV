use super::{Change, Error};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

pub(super) const MAX_ITEMS: usize = 10_000;
const MAX_EXACT_ID: u64 = (1 << 53) - 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct Key {
    pub id: u64,
    pub channel: u64,
    pub start: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Program {
    pub(super) key: Key,
    pub(super) end: u64,
}
impl Program {
    pub fn new(id: u64, channel: u64, start: u64, end: u64) -> Option<Self> {
        (id <= MAX_EXACT_ID && channel <= MAX_EXACT_ID && start < end && end <= MAX_EXACT_ID)
            .then_some(Self {
                key: Key { id, channel, start },
                end,
            })
    }
    pub(super) fn ongoing(self, now: u64) -> bool {
        self.end > now
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    Overlap,
    Skipped,
    Reserved,
    Conflict,
    Recording,
}
impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Overlap => "overlap",
            Self::Skipped => "skipped",
            Self::Reserved => "reserved",
            Self::Conflict => "conflict",
            Self::Recording => "recording",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    status: Status,
    reserve: Option<Reserve>,
    count: usize,
}
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    entries: HashMap<Key, Entry>,
}
impl Snapshot {
    pub fn status(&self, program: Program) -> Option<Status> {
        self.entries.get(&program.key).map(|entry| entry.status)
    }
    pub(super) fn cancellation(&self, program: Program, now: u64) -> Option<u64> {
        self.target(program, now, Change::Cancel)
            .map(|target| target.id)
    }
    pub(super) fn target(&self, program: Program, now: u64, change: Change) -> Option<Target> {
        let entry = self.entries.get(&program.key)?;
        if entry.count != 1 || (change != Change::Unexclude && entry.status == Status::Recording) {
            return None;
        }
        let reserve = entry.reserve.as_ref()?;
        reserve.permits(program, now, change).then_some(Target {
            id: reserve.id,
            rule_id: reserve.rule_id,
        })
    }
    pub(super) fn from_rows(
        reserves: Vec<Reserve>,
        recording: Vec<Recording>,
    ) -> Result<Self, Error> {
        if reserves.len() + recording.len() > MAX_ITEMS {
            return Err(Error::Limit);
        }
        let mut result = Self::default();
        let mut ids = HashSet::new();
        for row in reserves {
            if row.id > MAX_EXACT_ID || !ids.insert(row.id) || row.end_at <= row.start_at {
                return Err(Error::Invalid);
            }
            let Some(key) = row.key() else { continue }; // Time-specified reservations have no program identity.
            let status = if row.is_conflict {
                Status::Conflict
            } else if row.is_skip {
                Status::Skipped
            } else if row.is_overlap {
                Status::Overlap
            } else {
                Status::Reserved
            };
            result
                .entries
                .entry(key)
                .and_modify(|entry| {
                    entry.status = entry.status.max(status);
                    entry.reserve = None;
                    entry.count += 1;
                })
                .or_insert(Entry {
                    status,
                    reserve: Some(row),
                    count: 1,
                });
        }
        ids.clear();
        for row in recording {
            if !ids.insert(row.id) || !row.is_recording {
                return Err(Error::Invalid);
            }
            let Some(key) = row.key() else { continue }; // Recordings from time-specified reservations are not guide actions.
            result
                .entries
                .entry(key)
                .and_modify(|entry| {
                    entry.status = Status::Recording;
                })
                .or_insert(Entry {
                    status: Status::Recording,
                    reserve: None,
                    count: 0,
                });
        }
        Ok(result)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Reserve {
    pub id: u64,
    pub program_id: Option<u64>,
    pub channel_id: u64,
    pub start_at: u64,
    pub end_at: u64,
    pub rule_id: Option<u64>,
    pub is_event_relay: Option<bool>,
    pub is_skip: bool,
    pub is_conflict: bool,
    pub is_overlap: bool,
    pub is_time_specified: bool,
}
impl Reserve {
    fn key(&self) -> Option<Key> {
        Some(Key {
            id: self.program_id?,
            channel: self.channel_id,
            start: self.start_at,
        })
    }
    fn permits(&self, program: Program, now: u64, change: Change) -> bool {
        if self.key() != Some(program.key) || self.start_at <= now {
            return false;
        }
        match change {
            Change::Cancel => self.rule_id.is_none() && !self.is_time_specified,
            Change::Exclude => self.is_rule() && !self.is_skip && !self.is_overlap,
            Change::Unexclude => self.is_rule() && self.is_skip,
        }
    }
    fn is_rule(&self) -> bool {
        // The pinned API omits isEventRelay, just as its own guide uses ruleId.
        // Explicit relay metadata must still prevent a destructive exclusion.
        self.rule_id.is_some() && self.is_event_relay != Some(true)
    }
}
#[derive(Clone, Copy)]
pub(super) struct Target {
    pub id: u64,
    rule_id: Option<u64>,
}
impl Target {
    pub(super) fn matches(
        self,
        reserve: &Reserve,
        program: Program,
        now: u64,
        change: Change,
    ) -> bool {
        reserve.id == self.id
            && reserve.rule_id == self.rule_id
            && reserve.permits(program, now, change)
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Recording {
    id: u64,
    program_id: Option<u64>,
    channel_id: u64,
    start_at: u64,
    is_recording: bool,
}
impl Recording {
    fn key(&self) -> Option<Key> {
        Some(Key {
            id: self.program_id?,
            channel: self.channel_id,
            start: self.start_at,
        })
    }
    pub(super) fn matches(&self, program: Program) -> bool {
        self.key() == Some(program.key)
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Schedule {
    id: u64,
    channel_id: u64,
    start_at: u64,
    end_at: u64,
    #[serde(default)]
    is_duration_undefined: bool,
}
impl Schedule {
    pub(super) fn matches(&self, program: Program) -> bool {
        !self.is_duration_undefined
            && self.id == program.key.id
            && self.channel_id == program.key.channel
            && self.start_at == program.key.start
            && self.end_at == program.end
    }
}
