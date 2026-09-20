//! Bounded raw TS retention. Logical offsets never refer to reused physical bytes.
use super::index::Index;
use std::{collections::VecDeque, time::Duration};

pub(super) const SEGMENT_BYTES: usize = 1024 * 1024;
pub(super) const READ_BYTES: usize = super::TS_PACKET_SIZE * 256;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Retention {
    #[default]
    Off,
    Memory,
    Filesystem,
}

enum Storage {
    Memory,
    Filesystem(super::filesystem::Buffer),
}
enum Bytes {
    Memory(Vec<u8>),
    File,
}
struct Segment {
    start: u64,
    size: usize,
    end_ns: u64,
    bytes: Bytes,
}
#[derive(Debug)]
pub(super) enum ReadResult {
    Data { bytes: Vec<u8>, reconnected: bool },
    Awaiting,
    Expired,
    End,
}
pub(super) enum Status {
    Receiving,
    Ended,
    Failed(String),
}
pub(super) struct Store {
    storage: Storage,
    segments: VecDeque<Segment>,
    boundaries: VecDeque<u64>,
    end: u64,
    policy: super::Policy,
    pub index: Index,
    pub history: super::super::live_timeline::History,
    pub status: Status,
}

// Preparing owns the replacement resources and exclusively borrows their target.
// A failed preparation cannot discard the current history or change its policy.
pub(super) struct Prepared<'a> {
    store: &'a mut Store,
    policy: super::Policy,
    replacement: Option<(Storage, Segment)>,
}
impl Prepared<'_> {
    pub fn commit(self) -> std::io::Result<Option<super::super::timeline::RetentionChange>> {
        use super::super::timeline::RetentionChange;
        let (bytes, time) = self.policy.budget();
        let (previous_bytes, previous_time) = self.store.policy.budget();
        let change = if let Some((storage, segment)) = self.replacement {
            self.store.segments.clear();
            self.store.storage = storage;
            self.store.segments.push_back(segment);
            self.store.boundaries.clear();
            self.store.index.clear_entries();
            if let Some(end) = self.store.index.end_ns() {
                self.store.history.expire(end);
            }
            Some(RetentionChange::ReturnToLive)
        } else if bytes < previous_bytes || time < previous_time {
            Some(RetentionChange::ClampPosition)
        } else {
            None
        };
        self.store.trim(bytes, time)?;
        self.store.policy = self.policy;
        Ok(change)
    }
}
impl Store {
    pub fn new(
        policy: impl Into<super::Policy>,
        service: u16,
        programs: bool,
    ) -> std::io::Result<Self> {
        let policy = policy.into();
        let storage = match policy.storage() {
            Retention::Off | Retention::Memory => Storage::Memory,
            Retention::Filesystem => Storage::Filesystem(super::filesystem::Buffer::new()?),
        };
        let index = Index::new(service, programs);
        let history = super::super::live_timeline::History::from_catalog(index.catalog());
        Ok(Self {
            storage,
            segments: VecDeque::new(),
            boundaries: VecDeque::new(),
            end: 0,
            policy,
            index,
            history,
            status: Status::Receiving,
        })
    }
    pub fn policy(&self) -> super::Policy {
        self.policy
    }
    pub fn prepare(&mut self, policy: super::Policy) -> std::io::Result<Prepared<'_>> {
        self.prepare_with(policy, super::filesystem::Buffer::new)
    }
    fn prepare_with(
        &mut self,
        policy: super::Policy,
        buffer: impl FnOnce() -> std::io::Result<super::filesystem::Buffer>,
    ) -> std::io::Result<Prepared<'_>> {
        let reset = match (self.policy.storage(), policy.storage()) {
            (Retention::Memory, Retention::Memory)
            | (Retention::Filesystem, Retention::Filesystem)
            | (Retention::Off, Retention::Off)
            | (Retention::Off, Retention::Memory) => false,
            (Retention::Memory | Retention::Filesystem, Retention::Off)
            | (Retention::Off | Retention::Memory, Retention::Filesystem)
            | (Retention::Filesystem, Retention::Memory) => true,
        };
        let replacement = if reset {
            let (storage, bytes) = match policy.storage() {
                Retention::Off | Retention::Memory => (
                    Storage::Memory,
                    Bytes::Memory(Vec::with_capacity(SEGMENT_BYTES)),
                ),
                Retention::Filesystem => (Storage::Filesystem(buffer()?), Bytes::File),
            };
            Some((
                storage,
                Segment {
                    start: self.end,
                    size: 0,
                    end_ns: 0,
                    bytes,
                },
            ))
        } else {
            None
        };
        Ok(Prepared {
            store: self,
            policy,
            replacement,
        })
    }
    pub fn reconnect(&mut self) {
        self.boundaries.push_back(self.end);
        self.index.discontinuity();
    }
    pub fn append(&mut self, packets: &[u8]) -> std::io::Result<()> {
        if !packets.len().is_multiple_of(super::TS_PACKET_SIZE) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "unaligned TS write",
            ));
        }
        let (byte_limit, time_limit) = self.policy.budget();
        let mut remaining = packets;
        while !remaining.is_empty() {
            if self
                .segments
                .back()
                .is_none_or(|segment| segment.size + super::TS_PACKET_SIZE > SEGMENT_BYTES)
            {
                // Free an old allocation before reserving another chunk. The
                // configured TS capacity also bounds transient ring growth.
                if matches!(self.storage, Storage::Memory) {
                    while !self.segments.is_empty()
                        && (self.segments.len() as u64 + 1) * SEGMENT_BYTES as u64 > byte_limit
                    {
                        self.segments.pop_front();
                    }
                }
                let bytes = match &self.storage {
                    Storage::Memory => Bytes::Memory(Vec::with_capacity(SEGMENT_BYTES)),
                    Storage::Filesystem(_) => Bytes::File,
                };
                self.segments.push_back(Segment {
                    start: self.end,
                    size: 0,
                    end_ns: 0,
                    bytes,
                });
            }
            let segment = self.segments.back_mut().expect("segment inserted above");
            let count = remaining.len().min(
                (SEGMENT_BYTES - segment.size) / super::TS_PACKET_SIZE * super::TS_PACKET_SIZE,
            );
            let packets = &remaining[..count];
            match &mut segment.bytes {
                Bytes::Memory(bytes) => bytes.extend_from_slice(packets),
                Bytes::File => {
                    let Storage::Filesystem(buffer) = &mut self.storage else {
                        unreachable!("file segment belongs to filesystem storage")
                    };
                    buffer.write(segment.start, segment.size, packets)?;
                }
            }
            for (number, packet) in packets
                .as_chunks::<{ super::TS_PACKET_SIZE }>()
                .0
                .iter()
                .enumerate()
            {
                if let Some(anchor) = self
                    .index
                    .packet(self.end + (number * super::TS_PACKET_SIZE) as u64, packet)
                {
                    self.history.observe(
                        anchor.epoch,
                        anchor.time_ns,
                        self.index.end_ns().unwrap_or(anchor.time_ns),
                        anchor.observation(),
                    );
                }
            }
            segment.size += count;
            segment.end_ns = self.index.end_ns().unwrap_or_default();
            self.end += count as u64;
            remaining = &remaining[count..];
            // Reclaim between chunks even when a caller supplies a large batch.
            if !remaining.is_empty() {
                self.trim(byte_limit, time_limit)?;
            }
        }
        self.trim(byte_limit, time_limit)
    }
    fn trim(&mut self, byte_limit: u64, time_limit: Duration) -> std::io::Result<()> {
        let earliest = self
            .index
            .end_ns()
            .unwrap_or_default()
            .saturating_sub(time_limit.as_nanos() as u64);
        while self.segments.len() > 1
            && self.segments.front().is_some_and(|first| {
                (match self.storage {
                    Storage::Memory => self.segments.len() as u64 * SEGMENT_BYTES as u64,
                    Storage::Filesystem(_) => self.end - first.start,
                }) > byte_limit
                    || first.end_ns < earliest
            })
        {
            if let Storage::Filesystem(buffer) = &mut self.storage {
                buffer.remove(self.segments.front().expect("nonempty store").start)?;
            }
            self.segments.pop_front();
        }
        self.index.expire(self.start());
        while self
            .boundaries
            .front()
            .is_some_and(|offset| *offset < self.start())
        {
            self.boundaries.pop_front();
        }
        self.index.expire_time(earliest);
        if let Some(first) = self.index.entries().front() {
            self.history.expire(first.time_ns);
        }
        if let Some(end) = self.index.end_ns() {
            self.history.advance_end(end);
        }
        Ok(())
    }
    pub fn start(&self) -> u64 {
        self.segments
            .front()
            .map_or(self.end, |segment| segment.start)
    }
    pub fn read(&mut self, offset: u64) -> std::io::Result<ReadResult> {
        let readable_start = self
            .index
            .entries()
            .front()
            .filter(|entry| entry.time_ns > 0)
            .map_or(self.start(), |entry| entry.offset.max(self.start()));
        if offset < readable_start {
            return Ok(ReadResult::Expired);
        }
        if offset >= self.end {
            return match &self.status {
                Status::Receiving => Ok(ReadResult::Awaiting),
                Status::Ended => Ok(ReadResult::End),
                Status::Failed(message) => Err(std::io::Error::other(message.clone())),
            };
        }
        let segment = self
            .segments
            .iter_mut()
            .find(|segment| offset >= segment.start && offset < segment.start + segment.size as u64)
            .expect("offset in retained window");
        let within = (offset - segment.start) as usize;
        let until_boundary = self
            .boundaries
            .iter()
            .find(|boundary| **boundary > offset)
            .map_or(READ_BYTES, |boundary| (*boundary - offset) as usize);
        let count = READ_BYTES.min(segment.size - within).min(until_boundary);
        let bytes = match &mut segment.bytes {
            Bytes::Memory(bytes) => bytes[within..within + count].to_vec(),
            Bytes::File => {
                let Storage::Filesystem(buffer) = &mut self.storage else {
                    unreachable!("file segment belongs to filesystem storage")
                };
                buffer.read(segment.start, within, count)?
            }
        };
        Ok(ReadResult::Data {
            bytes,
            reconnected: self.boundaries.contains(&offset),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resizing_live_history_preserves_offsets_and_trims_both_budgets() -> std::io::Result<()> {
        use super::super::{Limits, Policy};
        const SMALL_MIB: u32 = super::super::limits::MIN_CAPACITY_MIB;
        const LARGE_MIB: u32 = SMALL_MIB * 2;
        const INITIAL_MINUTES: u32 = 3;
        const REDUCED_MINUTES: u32 = 1;
        let root = tempfile::tempdir()?;
        for storage in [Retention::Memory, Retention::Filesystem] {
            let initial = Limits::new(LARGE_MIB, LARGE_MIB, INITIAL_MINUTES).unwrap();
            let mut store = Store::new(Policy::new(Retention::Memory, initial), 1, true)?;
            store
                .prepare_with(Policy::new(storage, initial), || {
                    super::super::filesystem::Buffer::in_root(root.path())
                })?
                .commit()?;
            let packet = [0; super::super::TS_PACKET_SIZE];
            let block = packet.repeat(READ_BYTES / packet.len());
            let received_bytes = u64::from(SMALL_MIB + SMALL_MIB / 2) * SEGMENT_BYTES as u64;
            for _ in 0..received_bytes.div_ceil(block.len() as u64) {
                store.append(&block)?;
            }
            let end = store.end;
            let reduced = Policy::new(
                storage,
                Limits::new(SMALL_MIB, SMALL_MIB, INITIAL_MINUTES).unwrap(),
            );
            store.prepare(reduced)?.commit()?;
            assert_eq!(store.end, end);
            assert!(matches!(store.read(0)?, ReadResult::Expired));
            let start = store.start();
            assert!(matches!(store.read(start)?, ReadResult::Data { .. }));
            store.prepare(Policy::new(storage, initial))?.commit()?;
            assert_eq!(store.start(), start);
            assert_eq!(store.end, end);
            let fixture = include_bytes!("../../../../tests/fixtures/recording-seek.ts");
            store.append(fixture)?;
            store.reconnect();
            store.append(fixture)?;
            let end = store.end;
            let first = store.index.entries().front().unwrap().time_ns;
            store
                .prepare(Policy::new(
                    storage,
                    Limits::new(LARGE_MIB, LARGE_MIB, REDUCED_MINUTES).unwrap(),
                ))?
                .commit()?;
            assert_eq!(store.end, end);
            assert!(store.index.entries().front().unwrap().time_ns > first);
            assert!(
                store.index.end_ns().unwrap() - store.index.entries().front().unwrap().time_ns
                    <= Duration::from_secs(60).as_nanos() as u64
            );
        }
        Ok(())
    }

    #[test]
    fn storage_preparation_failure_and_cancellation_leave_history_untouched() -> std::io::Result<()>
    {
        use super::super::{Limits, Policy};
        let root = tempfile::tempdir()?;
        let blocked = root.path().join("not-a-directory");
        std::fs::write(&blocked, b"occupied")?;
        let mut store = Store::new(Retention::Memory, 1, true)?;
        let fixture = include_bytes!("../../../../tests/fixtures/recording.ts");
        store.append(fixture)?;
        let end = store.end;
        let first = store.index.entries().front().unwrap().time_ns;
        let policy = store.policy();
        let replacement = Policy::new(Retention::Filesystem, Limits::default());
        assert!(
            store
                .prepare_with(replacement, || super::super::filesystem::Buffer::in_root(
                    &blocked
                ))
                .is_err()
        );
        let prepared = store.prepare_with(replacement, || {
            super::super::filesystem::Buffer::in_root(root.path())
        })?;
        drop(prepared);
        assert!(std::fs::read_dir(root.path())?.all(|entry| {
            let name = entry.unwrap().file_name();
            name == "registry.lock" || name == "not-a-directory"
        }));
        assert_eq!(store.policy(), policy);
        assert_eq!(store.end, end);
        assert_eq!(store.index.entries().front().unwrap().time_ns, first);
        assert!(matches!(store.read(0)?, ReadResult::Data { .. }));
        store
            .prepare_with(replacement, || {
                super::super::filesystem::Buffer::in_root(root.path())
            })?
            .commit()?;
        assert_eq!(store.end, end);
        assert!(store.index.entries().is_empty());
        assert!(matches!(store.read(0)?, ReadResult::Expired));
        store.append(fixture)?;
        assert!(store.index.end_ns().unwrap() > first);
        let readable = store.index.entries().front().unwrap().offset;
        assert!(readable >= end);
        assert!(matches!(store.read(readable)?, ReadResult::Data { .. }));
        Ok(())
    }

    #[test]
    fn configured_memory_budget_bounds_allocations_and_discards_the_old_cursor()
    -> std::io::Result<()> {
        use super::super::{Limits, Policy, limits::MIN_CAPACITY_MIB};
        let limits = Limits::new(MIN_CAPACITY_MIB, MIN_CAPACITY_MIB, 1).unwrap();
        let mut store = Store::new(Policy::new(Retention::Memory, limits), 1, false)?;
        let packet = [0; super::super::TS_PACKET_SIZE];
        let block = packet.repeat(READ_BYTES / packet.len());
        let capacity = u64::from(MIN_CAPACITY_MIB) * SEGMENT_BYTES as u64;
        const CAPACITY_CYCLES: u64 = 3;
        for _ in 0..(capacity * CAPACITY_CYCLES).div_ceil(block.len() as u64) {
            store.append(&block)?;
            let allocated: usize = store
                .segments
                .iter()
                .map(|segment| match &segment.bytes {
                    Bytes::Memory(bytes) => bytes.capacity(),
                    Bytes::File => unreachable!(),
                })
                .sum();
            assert!(allocated as u64 <= capacity);
        }
        assert!(matches!(store.read(0)?, ReadResult::Expired));
        assert!(matches!(
            store.read(store.start())?,
            ReadResult::Data { .. }
        ));
        Ok(())
    }
    #[test]
    fn memory_and_files_have_the_same_expiry_eof_and_cleanup_contract() -> std::io::Result<()> {
        const CAPACITY_MIB: u32 = super::super::limits::MIN_CAPACITY_MIB;
        const TEST_BYTES: u64 = CAPACITY_MIB as u64 * SEGMENT_BYTES as u64;
        const TEST_SEGMENTS: usize = CAPACITY_MIB as usize + 4;
        let mut packet = [0; super::super::TS_PACKET_SIZE];
        packet[0] = crate::transport::wire::SYNC_BYTE;
        let block = packet.repeat(READ_BYTES / packet.len());
        for backend in [Retention::Memory, Retention::Filesystem] {
            let root = tempfile::tempdir()?;
            let mut store = Store::new(Retention::Memory, 1, true)?;
            if backend == Retention::Filesystem {
                store.storage =
                    Storage::Filesystem(super::super::filesystem::Buffer::in_root(root.path())?);
            }
            store.policy = super::super::Policy::new(
                backend,
                super::super::Limits::new(CAPACITY_MIB, CAPACITY_MIB, 1).unwrap(),
            );
            assert!(matches!(store.read(0)?, ReadResult::Awaiting));
            for _ in 0..(TEST_SEGMENTS * SEGMENT_BYTES).div_ceil(block.len()) {
                store.append(&block)?;
            }
            assert!(store.end - store.start() <= TEST_BYTES);
            assert!(matches!(store.read(0)?, ReadResult::Expired));
            assert!(
                matches!(store.read(store.start())?, ReadResult::Data { bytes, .. } if bytes == block)
            );
            assert!(matches!(store.read(store.end)?, ReadResult::Awaiting));
            store.status = Status::Ended;
            assert!(matches!(store.read(store.end)?, ReadResult::End));
            store.status = Status::Failed("disk full".into());
            assert!(store.read(store.end).is_err());
            drop(store);
            assert!(
                std::fs::read_dir(root.path())?
                    .all(|entry| entry.unwrap().file_name() == "registry.lock")
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod reconnect_tests {
    use super::*;
    #[test]
    fn reconnect_is_a_read_boundary_and_never_discards_old_data() -> std::io::Result<()> {
        let ts = include_bytes!("../../../../tests/fixtures/recording.ts");
        let mut store = Store::new(Retention::Memory, 1, true)?;
        store.append(ts)?;
        let boundary = store.end;
        store.reconnect();
        store.append(ts)?;
        let packet_bytes = super::super::TS_PACKET_SIZE as u64;
        assert!(
            matches!(store.read(boundary - packet_bytes)?, ReadResult::Data { bytes, reconnected: false } if bytes.len() as u64 == packet_bytes)
        );
        assert!(matches!(
            store.read(boundary)?,
            ReadResult::Data {
                reconnected: true,
                ..
            }
        ));
        assert!(store.index.end_ns().unwrap() > Duration::from_secs(5).as_nanos() as u64);
        Ok(())
    }
}

#[cfg(all(test, target_os = "linux", target_env = "gnu"))]
mod failure_tests {
    use super::*;
    #[test]
    fn failed_write_does_not_publish_unwritten_bytes_or_timestamps() -> std::io::Result<()> {
        let root = tempfile::tempdir()?;
        let mut store = Store::new(Retention::Memory, 1, true)?;
        store.storage =
            Storage::Filesystem(super::super::filesystem::Buffer::in_root(root.path())?);
        let ts = include_bytes!("../../../../tests/fixtures/recording.ts");
        store.append(ts)?;
        let end = store.end;
        let duration = store.index.end_ns();
        let Storage::Filesystem(buffer) = &mut store.storage else {
            unreachable!()
        };
        buffer.deny_writes_for_test()?;
        assert!(store.append(ts).is_err());
        assert_eq!(store.end, end);
        assert_eq!(store.index.end_ns(), duration);
        Ok(())
    }
}

#[cfg(test)]
mod timeline_tests {
    use super::*;
    use crate::playback::{
        live_timeline::{Presenter, Reading},
        timeline::Phase,
    };
    #[test]
    fn disabled_timeshift_keeps_metadata_until_decoded_output_passes_it() {
        let fixture = include_bytes!("../../../../tests/fixtures/recording-seek.ts");
        let mut store = Store::new(Retention::Off, 1, true).unwrap();
        store.append(fixture).unwrap();
        let start = store.index.entries().front().unwrap().time_ns;
        let end = store.index.end_ns().unwrap();
        let mut presenter = Presenter::new();
        // Decoded frames can outlive the two-second raw forward buffer. Cross
        // an EIT event boundary while both positions precede its retained TS.
        const FIRST_EVENT_SECONDS: u64 = 10;
        const SECOND_EVENT_SECONDS: u64 = 31;
        const IN_FLIGHT_COMMENT_AGE: Duration = Duration::from_secs(5);
        for (seconds, event_id) in [(FIRST_EVENT_SECONDS, 1), (SECOND_EVENT_SECONDS, 2)] {
            let position = Duration::from_secs(seconds).as_nanos() as u64;
            assert!(position < start);
            let snapshot = presenter.project(
                &mut store.history,
                start,
                end,
                false,
                Reading {
                    phase: Phase::Playing,
                    position_ns: Some(position),
                    target_ns: None,
                },
            );
            let value: serde_json::Value = serde_json::from_str(&snapshot.serialize()).unwrap();
            assert_eq!(value["viewing"]["program_status"], "available", "{value}");
            assert_eq!(value["viewing"]["program"]["data"]["eventId"], event_id);
            assert!(value["viewing"]["utc"].is_number());
            assert_eq!(value["available"], serde_json::json!([]));
            assert!(store.index.view(position).clock.is_some());
            // The next receive cycle must preserve this displayed position.
            store.append(&[]).unwrap();
            assert!(store.index.view(position).program.is_some());
            let comment_position = position - IN_FLIGHT_COMMENT_AGE.as_nanos() as u64;
            assert!(
                store
                    .index
                    .view(position)
                    .clock
                    .unwrap()
                    .utc(comment_position)
                    .is_some()
            );
        }
        let first = Duration::from_secs(FIRST_EVENT_SECONDS).as_nanos() as u64;
        assert!(store.index.view(first).program.is_none());
        assert!(matches!(store.read(0).unwrap(), ReadResult::Expired));
    }
    #[test]
    fn received_fixture_populates_catalog_without_scanning_pcr_entries() {
        let fixture = include_bytes!("../../../../tests/fixtures/recording-seek.ts");
        let mut store = Store::new(Retention::Memory, 1, true).unwrap();
        store.append(fixture).unwrap();
        let end = store.index.end_ns().unwrap();
        let mut presenter = Presenter::new();
        let position = Duration::from_secs(10);
        let snapshot = presenter.project(
            &mut store.history,
            0,
            end,
            true,
            Reading {
                phase: Phase::Playing,
                position_ns: Some(position.as_nanos() as u64),
                target_ns: None,
            },
        );
        let value: serde_json::Value = serde_json::from_str(&snapshot.serialize()).unwrap();
        assert!(value["live"]["program"].is_object());
        assert!(value["viewing"]["program"].is_object());
        // The fixture advertises the following event; the last presentation
        // timestamps reach its boundary as well as the two current events.
        const ADVERTISED_EVENTS: usize = 3;
        assert_eq!(
            value["programs"].as_array().unwrap().len(),
            ADVERTISED_EVENTS,
            "{value}"
        );
        for (index, program) in value["programs"].as_array().unwrap().iter().enumerate() {
            assert!(
                program["id"]
                    .as_str()
                    .unwrap()
                    .ends_with(&format!(":{}", index + 1)),
                "{value}"
            );
        }
        const CHANGE_MS: i64 = 30_000;
        const CLOCK_PRECISION_MS: i64 = 1_500;
        assert!(
            (value["programs"][1]["start"].as_i64().unwrap() - CHANGE_MS).abs()
                <= CLOCK_PRECISION_MS
        );
    }
}
