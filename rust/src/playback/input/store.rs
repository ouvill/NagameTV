//! Bounded raw TS retention. Logical offsets never refer to reused physical bytes.
use super::index::Index;
use std::{
    collections::VecDeque,
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    time::Duration,
};

const SEGMENT_BYTES: usize = 1024 * 1024;
pub(super) const READ_BYTES: usize = super::TS_PACKET_SIZE * 256;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Retention {
    Off,
    #[default]
    Memory,
    Filesystem,
}

enum Storage {
    Memory,
    Filesystem(super::filesystem::Directory),
}
enum Bytes {
    Memory(Vec<u8>),
    File(std::path::PathBuf),
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
    byte_limit: u64,
    time_limit: Duration,
    pub index: Index,
    pub history: super::super::live_timeline::History,
    pub status: Status,
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
            Retention::Filesystem => Storage::Filesystem(super::filesystem::Directory::new()?),
        };
        let (byte_limit, time_limit) = policy.budget();
        Ok(Self {
            storage,
            segments: VecDeque::new(),
            boundaries: VecDeque::new(),
            end: 0,
            byte_limit,
            time_limit,
            index: Index::new(service, programs),
            history: Default::default(),
            status: Status::Receiving,
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
                        && (self.segments.len() as u64 + 1) * SEGMENT_BYTES as u64 > self.byte_limit
                    {
                        self.segments.pop_front();
                    }
                }
                let bytes = match &self.storage {
                    Storage::Memory => Bytes::Memory(Vec::with_capacity(SEGMENT_BYTES)),
                    Storage::Filesystem(directory) => {
                        let path = directory.path().join(self.end.to_string());
                        File::options().create_new(true).write(true).open(&path)?;
                        Bytes::File(path)
                    }
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
                Bytes::File(path) => {
                    File::options()
                        .append(true)
                        .open(path)?
                        .write_all(packets)?;
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
        }
        let earliest = self
            .index
            .end_ns()
            .unwrap_or_default()
            .saturating_sub(self.time_limit.as_nanos() as u64);
        while self.segments.len() > 1
            && self.segments.front().is_some_and(|first| {
                (match self.storage {
                    Storage::Memory => self.segments.len() as u64 * SEGMENT_BYTES as u64,
                    Storage::Filesystem(_) => self.end - first.start,
                }) > self.byte_limit
                    || first.end_ns < earliest
            })
        {
            let removed = self.segments.pop_front().expect("nonempty store");
            if let Storage::Filesystem(directory) = &self.storage {
                let path = directory.path().join(removed.start.to_string());
                drop(removed); // Close before removal, including on Windows.
                std::fs::remove_file(path)?;
            }
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
            Bytes::File(path) => {
                let mut file = File::open(path)?;
                file.seek(SeekFrom::Start(within as u64))?;
                let mut bytes = vec![0; count];
                file.read_exact(&mut bytes)?;
                bytes
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
                    Bytes::File(_) => unreachable!(),
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
        const TEST_BYTES: u64 = (SEGMENT_BYTES * 2) as u64;
        const TEST_SEGMENTS: usize = 5;
        let mut packet = [0; super::super::TS_PACKET_SIZE];
        packet[0] = crate::transport::wire::SYNC_BYTE;
        let block = packet.repeat(READ_BYTES / packet.len());
        for backend in [Retention::Memory, Retention::Filesystem] {
            let root = tempfile::tempdir()?;
            let mut store = Store::new(Retention::Memory, 1, true)?;
            if backend == Retention::Filesystem {
                store.storage =
                    Storage::Filesystem(super::super::filesystem::Directory::in_root(root.path())?);
            }
            store.byte_limit = TEST_BYTES;
            let directory = match &store.storage {
                Storage::Filesystem(path) => Some(path.path().to_owned()),
                Storage::Memory => None,
            };
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
            assert!(directory.is_none_or(|directory| !directory.exists()));
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

#[cfg(test)]
mod failure_tests {
    use super::*;
    #[test]
    fn failed_write_does_not_publish_unwritten_bytes_or_timestamps() -> std::io::Result<()> {
        let root = tempfile::tempdir()?;
        let mut store = Store::new(Retention::Memory, 1, true)?;
        store.storage =
            Storage::Filesystem(super::super::filesystem::Directory::in_root(root.path())?);
        let ts = include_bytes!("../../../../tests/fixtures/recording.ts");
        store.append(ts)?;
        let end = store.end;
        let duration = store.index.end_ns();
        let Bytes::File(path) = &store.segments.back().unwrap().bytes else {
            unreachable!()
        };
        // A real filesystem error, without a device or a replaced production IO implementation.
        std::fs::remove_file(path)?;
        std::fs::create_dir(path)?;
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
    fn received_fixture_populates_catalog_without_scanning_pcr_entries() {
        let fixture = include_bytes!("../../../../tests/fixtures/recording-seek.ts");
        let mut store = Store::new(Retention::Memory, 1, true).unwrap();
        store.append(fixture).unwrap();
        let end = store.index.end_ns().unwrap();
        let mut presenter = Presenter::new();
        let position = Duration::from_secs(10);
        let snapshot = presenter.project(
            &store.history,
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
