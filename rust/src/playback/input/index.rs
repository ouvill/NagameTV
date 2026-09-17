//! Sparse raw-byte/PCR index. Every entry owns the tables needed to start there.
use crate::transport::programs::{Collector, Observation, Timeline};
use crate::transport::{
    Pat, Sections,
    wire::{Pid, PsiSection, STUFFING_BYTE, SYNC_BYTE, TS_PACKET_SIZE, TransportPacket},
};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
};

const PCR_HZ: u64 = 90_000;
const PCR_WRAP: u64 = 1 << 33;
const DISCONTINUITY_TICKS: u64 = PCR_HZ * 10;
const INDEX_INTERVAL_TICKS: u64 = PCR_HZ / 4;
const INITIAL_PCR_STEP: u64 = PCR_HZ / 25;
const MAX_FILE_ENTRIES: usize = 65_536;
const MAX_METADATA_BYTES: usize = 16 * 1024 * 1024;
const TS_HEADER_BYTES: usize = 4;
const PAYLOAD_START_FLAG: u8 = 0x40;
const PAYLOAD_ONLY: u8 = 0x10;
const PID_HIGH_MASK: u8 = 0x1f;
const CONTINUITY_MASK: u8 = 0x0f;
const NANOSECONDS_PER_SECOND: u64 = 1_000_000_000;
const NANOSECONDS_PER_MILLISECOND: i128 = 1_000_000;
const RATE_WINDOW_NS: u64 = 30 * NANOSECONDS_PER_SECOND;
const MIN_RATE_SPAN_NS: u64 = NANOSECONDS_PER_SECOND;

#[derive(Clone, Debug)]
pub(super) struct Anchor {
    pub offset: u64,
    pub time_ns: u64,
    pub epoch: u64,
    pub bootstrap: Arc<Vec<u8>>,
    pcr: u64,
    programs: Option<Arc<Observation>>,
}

impl Anchor {
    pub fn relative_to(&self, origin: &Self) -> Self {
        let mut anchor = self.clone();
        anchor.time_ns =
            origin.time_ns + ticks_to_ns((self.pcr + PCR_WRAP - origin.pcr) % PCR_WRAP);
        anchor.epoch = origin.epoch;
        if let Some(observation) = &self.programs {
            let shift = |ns: u64| {
                (i128::from(ns) + i128::from(anchor.time_ns) - i128::from(self.time_ns)).max(0)
                    as u64
            };
            let mut observation = (**observation).clone();
            observation.pcr = shift(observation.pcr);
            observation.information.time = observation
                .information
                .time
                .map(|(pcr, utc)| (shift(pcr), utc));
            anchor.programs = Some(Arc::new(observation));
        }
        anchor
    }
    pub fn program(&self, position_ns: u64) -> Option<(String, f64)> {
        let observation = self.programs.as_ref()?;
        let mut timeline = Timeline::default();
        timeline.push((**observation).clone());
        Some(
            timeline
                .poll(Some(position_ns), |ns| Some(i128::from(ns)))
                .serialize(),
        )
    }
    pub fn charge(&self) -> usize {
        // Count shared snapshots for every anchor as a conservative upper bound.
        std::mem::size_of::<Self>()
            + self.bootstrap.capacity()
            + self.programs.as_ref().map_or(0, |observation| {
                let information = &observation.information;
                std::mem::size_of::<Observation>()
                    + information.station.capacity()
                    + information.provider.capacity()
                    + information
                        .current
                        .iter()
                        .chain(information.next.iter())
                        .map(|program| {
                            program.name.capacity()
                                + program.description.capacity()
                                + program.extended.capacity()
                                + program.genres.capacity() * std::mem::size_of::<(u8, u8)>()
                        })
                        .sum::<usize>()
            })
    }
}

pub(super) struct Index {
    service: u16,
    collector: Option<Collector>,
    programs: Option<Arc<Observation>>,
    pat: Pat,
    pat_sections: Sections,
    pat_bytes: BTreeMap<u8, Vec<u8>>,
    pmt_pid: Option<Pid>,
    pmt_sections: Sections,
    pcr_pid: Option<Pid>,
    tables: Arc<Vec<u8>>,
    pat_packets: Vec<u8>,
    last_pmt: Vec<u8>,
    clock: Option<(u64, u64)>,
    step: u64,
    epoch: u64,
    interval: u64,
    last_pts: BTreeMap<Pid, u64>,
    presentation_end: u64,
    entries: VecDeque<Anchor>,
    metadata_bytes: usize,
}
impl Index {
    pub fn new(service: u16, programs: bool) -> Self {
        Self {
            service,
            collector: programs.then(|| Collector::new(service)),
            programs: None,
            pat: Pat::default(),
            pat_sections: Sections::default(),
            pat_bytes: BTreeMap::new(),
            pmt_pid: None,
            pmt_sections: Sections::default(),
            pcr_pid: None,
            tables: Arc::default(),
            pat_packets: Vec::new(),
            last_pmt: Vec::new(),
            clock: None,
            step: INITIAL_PCR_STEP,
            epoch: 0,
            interval: INDEX_INTERVAL_TICKS,
            last_pts: BTreeMap::new(),
            presentation_end: 0,
            entries: VecDeque::new(),
            metadata_bytes: 0,
        }
    }
    pub fn entries(&self) -> &VecDeque<Anchor> {
        &self.entries
    }
    pub fn bytes_per_second(&self) -> Option<f64> {
        let last = self.entries.back()?;
        let first = self.entries.iter().find(|entry| {
            entry.epoch == last.epoch
                && entry.time_ns >= last.time_ns.saturating_sub(RATE_WINDOW_NS)
        })?;
        let span = last.time_ns.checked_sub(first.time_ns)?;
        (span >= MIN_RATE_SPAN_NS).then(|| {
            last.offset.saturating_sub(first.offset) as f64 * NANOSECONDS_PER_SECOND as f64
                / span as f64
        })
    }
    pub fn clear_entries(&mut self) {
        self.entries.clear();
        self.metadata_bytes = 0;
    }
    fn discard_first(&mut self) {
        if let Some(anchor) = self.entries.pop_front() {
            self.metadata_bytes = self.metadata_bytes.saturating_sub(anchor.charge());
        }
    }
    pub fn expire_time(&mut self, earliest: u64) {
        while self
            .entries
            .front()
            .is_some_and(|anchor| anchor.time_ns < earliest)
            || self.metadata_bytes > MAX_METADATA_BYTES
        {
            self.discard_first();
        }
    }
    pub fn service(&self) -> u16 {
        self.service
    }
    pub fn seed(&mut self, anchor: &Anchor) {
        self.clock = Some((
            anchor.pcr,
            (u128::from(anchor.time_ns) * u128::from(PCR_HZ) / u128::from(NANOSECONDS_PER_SECOND))
                as u64,
        ));
        self.epoch = anchor.epoch;
    }
    pub fn discontinuity(&mut self) {
        self.epoch += 1;
        if let Some(collector) = &mut self.collector {
            collector.reset();
        }
        self.programs = None;
        self.pat = Pat::default();
        self.pat_sections = Sections::default();
        self.pmt_sections = Sections::default();
        self.pcr_pid = None;
        self.tables = Arc::default();
        self.last_pmt.clear();
    }
    pub fn packet(&mut self, offset: u64, bytes: &[u8]) -> Option<Anchor> {
        let packet = TransportPacket::parse(bytes).ok()?;
        if packet.pid == Pid::PAT {
            for data in self.pat_sections.push(packet.start, packet.payload) {
                let Ok(section) = PsiSection::parse(&data) else {
                    continue;
                };
                self.pat_bytes.insert(section.section_number, data.clone());
                if let Some(programs) = self.pat.push(&section) {
                    if self.service == 0 {
                        self.service = *programs.keys().next()?;
                        if let Some(collector) = &mut self.collector {
                            *collector = Collector::new(self.service);
                        }
                    }
                    if let Some(collector) = &mut self.collector {
                        collector.transport(section.extension);
                    }
                    let pid = programs.get(&self.service).copied();
                    if self.pmt_pid != pid {
                        self.pmt_pid = pid;
                        self.pmt_sections = Sections::default();
                        self.pcr_pid = None;
                        self.last_pmt.clear();
                    }
                    self.pat_packets = self
                        .pat_bytes
                        .range(..=section.last_section_number)
                        .flat_map(|(_, data)| packetize(Pid::PAT, data))
                        .collect();
                }
            }
        } else if Some(packet.pid) == self.pmt_pid {
            for data in self.pmt_sections.push(packet.start, packet.payload) {
                let Ok(section) = PsiSection::parse(&data) else {
                    continue;
                };
                let Ok(map) = section.program_map() else {
                    continue;
                };
                if map.service != self.service {
                    continue;
                }
                self.pcr_pid = Some(map.pcr_pid);
                if let Some(collector) = &mut self.collector {
                    collector.pcr_pid(map.pcr_pid);
                }
                if self.last_pmt != data {
                    // tsreadex follows PMT revisions itself. A descriptor or
                    // stream-list change is not a clock discontinuity; resetting
                    // its PES/continuity state here can stall ongoing decoding.
                    let mut tables = self.pat_packets.clone();
                    tables.extend(packetize(packet.pid, &data));
                    self.tables = Arc::new(tables);
                    self.last_pmt = data;
                }
            }
        }
        if packet.start
            && Some(packet.pid) == self.pcr_pid
            && let Some(pts) = crate::transport::pes::PesHeader::parse(packet.payload)
                .and_then(|header| header.pts_ticks)
            && let Some((pcr, ticks)) = self.clock
        {
            let distance = (pts + PCR_WRAP - pcr) % PCR_WRAP;
            if distance < DISCONTINUITY_TICKS {
                let mapped = ticks + distance;
                let step = self
                    .last_pts
                    .insert(packet.pid, mapped)
                    .and_then(|previous| mapped.checked_sub(previous))
                    .filter(|step| *step > 0 && *step < PCR_HZ)
                    .unwrap_or(self.step);
                self.presentation_end = self.presentation_end.max(ticks_to_ns(mapped + step));
            }
        }
        if let Some(collector) = &mut self.collector
            && let Ok(bytes) = bytes.try_into()
        {
            collector.packet(&packet, bytes);
            if let Some(mut observation) = collector.take()
                && let Some((pcr, ticks)) = self.clock
            {
                let map = |raw: u64| {
                    let distance = (raw + PCR_WRAP - pcr) % PCR_WRAP;
                    let signed = if distance > PCR_WRAP / 2 {
                        i128::from(distance) - i128::from(PCR_WRAP)
                    } else {
                        i128::from(distance)
                    };
                    let global = (i128::from(ticks) + signed).max(0) as u64;
                    ticks_to_ns(global)
                };
                observation.pcr = map(observation.pcr);
                observation.information.time = observation
                    .information
                    .time
                    .map(|(pcr, utc)| (map(pcr), utc));
                self.programs = Some(Arc::new(observation));
            }
        }
        if Some(packet.pid) != self.pcr_pid || self.tables.is_empty() {
            return None;
        }
        let pcr = packet.pcr?;
        let ticks = match self.clock {
            None => 0,
            Some((previous, time)) => {
                let delta = (pcr + PCR_WRAP - previous) % PCR_WRAP;
                if delta > DISCONTINUITY_TICKS || packet.discontinuity {
                    self.epoch += 1;
                    time + self.step
                } else {
                    if delta != 0 {
                        self.step = delta;
                    }
                    time + delta
                }
            }
        };
        self.clock = Some((pcr, ticks));
        let anchor = Anchor {
            offset,
            time_ns: ticks_to_ns(ticks),
            epoch: self.epoch,
            pcr,
            programs: self.programs.clone(),
            bootstrap: self.tables.clone(),
        };
        if self.entries.back().is_none_or(|last| {
            last.epoch != anchor.epoch
                || !Arc::ptr_eq(&last.bootstrap, &anchor.bootstrap)
                || anchor.time_ns.saturating_sub(last.time_ns)
                    >= self.interval * NANOSECONDS_PER_SECOND / PCR_HZ
        }) {
            self.metadata_bytes += anchor.charge();
            self.entries.push_back(anchor.clone());
        }
        Some(anchor)
    }
    pub fn program(&self, position_ns: u64) -> Option<(String, f64)> {
        self.entries
            .iter()
            .rev()
            .find(|entry| entry.time_ns <= position_ns)?
            .program(position_ns)
    }

    /// Derive markers from the bounded retained SI snapshots, independently of
    /// the playhead. Use the newest schedule/clock for each event; adjoining
    /// events share one UTC boundary. Never project a schedule across PCR epochs.
    pub fn program_boundaries_ms(&self) -> Vec<i64> {
        if self.collector.is_none() {
            return Vec::new();
        }
        let Some(end) = self.end_ns() else {
            return Vec::new();
        };
        let mut epochs = BTreeMap::<u64, (u64, u64)>::new();
        for anchor in &self.entries {
            epochs
                .entry(anchor.epoch)
                .and_modify(|range| range.1 = anchor.time_ns)
                .or_insert((anchor.time_ns, anchor.time_ns));
        }
        if let Some(mut entry) = epochs.last_entry() {
            entry.get_mut().1 = end;
        }
        let mut seen = BTreeSet::new();
        let mut boundaries = BTreeMap::new();
        let mut previous = None;
        for anchor in self.entries.iter().rev() {
            let Some(observation) = &anchor.programs else {
                continue;
            };
            if previous.is_some_and(|(epoch, snapshot)| {
                epoch == anchor.epoch && Arc::ptr_eq(snapshot, observation)
            }) {
                continue;
            }
            previous = Some((anchor.epoch, observation));
            let info = &observation.information;
            let Some((clock_ns, unix_ms)) = info.time else {
                continue;
            };
            let (epoch_start, epoch_end) = epochs[&anchor.epoch];
            for program in info.current.iter().chain(info.next.iter()) {
                let Some((start, duration)) = program.start_at.zip(program.duration) else {
                    continue;
                };
                if duration == 0
                    || !seen.insert((
                        anchor.epoch,
                        program.network_id,
                        program.transport_stream_id,
                        program.service_id,
                        program.event_id,
                    ))
                {
                    continue;
                }
                for utc in [i128::from(start), i128::from(start) + i128::from(duration)] {
                    let mapped = i128::from(clock_ns)
                        + (utc - i128::from(unix_ms)) * NANOSECONDS_PER_MILLISECOND;
                    if mapped > i128::from(epoch_start)
                        && mapped < i128::from(epoch_end)
                        && let Ok(ms) = i64::try_from(mapped / NANOSECONDS_PER_MILLISECOND)
                    {
                        boundaries.entry((anchor.epoch, utc)).or_insert(ms);
                    }
                }
            }
        }
        let mut result: Vec<_> = boundaries.into_values().collect();
        result.sort_unstable();
        result.dedup();
        result
    }

    pub fn compact_file(&mut self) {
        if self.entries.len() > MAX_FILE_ENTRIES || self.metadata_bytes > MAX_METADATA_BYTES {
            let mut position = 0;
            self.entries.retain(|_| {
                let keep = position % 2 == 0;
                position += 1;
                keep
            });
            self.metadata_bytes = self.entries.iter().map(Anchor::charge).sum();
            self.interval *= 2;
        }
    }
    pub fn expire(&mut self, first_offset: u64) {
        while self
            .entries
            .front()
            .is_some_and(|entry| entry.offset < first_offset)
        {
            self.discard_first();
        }
    }
    pub fn end_ns(&self) -> Option<u64> {
        self.clock
            .map(|(_, time)| ticks_to_ns(time + self.step).max(self.presentation_end))
    }
}

fn packetize(pid: Pid, section: &[u8]) -> Vec<u8> {
    let mut data = Vec::with_capacity(section.len() + 1);
    data.push(0); // pointer_field: section starts immediately.
    data.extend_from_slice(section);
    let mut output = Vec::new();
    for (counter, payload) in data.chunks(TS_PACKET_SIZE - TS_HEADER_BYTES).enumerate() {
        let mut packet = [STUFFING_BYTE; TS_PACKET_SIZE];
        packet[0] = SYNC_BYTE;
        packet[1] = ((pid.0 >> 8) as u8 & PID_HIGH_MASK)
            | if counter == 0 { PAYLOAD_START_FLAG } else { 0 };
        packet[2] = pid.0 as u8;
        packet[3] = PAYLOAD_ONLY | (counter as u8 & CONTINUITY_MASK);
        packet[TS_HEADER_BYTES..TS_HEADER_BYTES + payload.len()].copy_from_slice(payload);
        output.extend(packet);
    }
    output
}

fn ticks_to_ns(ticks: u64) -> u64 {
    (u128::from(ticks) * u128::from(NANOSECONDS_PER_SECOND) / u128::from(PCR_HZ))
        .min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod rate_tests {
    use super::*;
    const BYTES_PER_SECOND: u64 = 2_000_000;
    fn anchor(seconds: u64, epoch: u64) -> Anchor {
        Anchor {
            offset: seconds * BYTES_PER_SECOND,
            time_ns: seconds * NANOSECONDS_PER_SECOND,
            epoch,
            bootstrap: Arc::default(),
            pcr: seconds * PCR_HZ,
            programs: None,
        }
    }
    #[test]
    fn rate_uses_recent_bytes_and_waits_for_a_new_clock_epoch() {
        let mut index = Index::new(0, false);
        assert_eq!(index.bytes_per_second(), None);
        let history_seconds = 60;
        index.entries = (0..=history_seconds).map(|s| anchor(s, 0)).collect();
        // Old, higher-rate data must not affect the recent 30-second estimate.
        index.entries.front_mut().unwrap().offset = 0;
        for entry in index.entries.iter_mut().skip(1) {
            entry.offset += BYTES_PER_SECOND * history_seconds;
        }
        assert_eq!(index.bytes_per_second(), Some(BYTES_PER_SECOND as f64));
        index.entries.push_back(anchor(history_seconds + 1, 1));
        assert_eq!(index.bytes_per_second(), None);
        index.entries.push_back(anchor(history_seconds + 2, 1));
        assert_eq!(index.bytes_per_second(), Some(BYTES_PER_SECOND as f64));
        index.clear_entries();
        assert_eq!(index.bytes_per_second(), None);
    }

    #[test]
    fn retained_si_provides_one_shared_program_boundary_and_discards_expired_markers() {
        let fixture = include_bytes!("../../../../tests/fixtures/recording-seek.ts");
        let mut index = Index::new(1, true);
        for (number, packet) in fixture.as_chunks::<TS_PACKET_SIZE>().0.iter().enumerate() {
            index.packet((number * TS_PACKET_SIZE) as u64, packet);
        }
        const CHANGE_MS: i64 = 30_000;
        const BROADCAST_CLOCK_TOLERANCE_MS: i64 = 1_000;
        let markers = index.program_boundaries_ms();
        assert_eq!(
            markers
                .iter()
                .filter(|ms| (**ms - CHANGE_MS).abs() <= BROADCAST_CLOCK_TOLERANCE_MS)
                .count(),
            1,
            "shared start/end and repeated SI: {markers:?}"
        );
        assert!(markers.windows(2).all(|pair| pair[0] < pair[1]));
        let after_boundary =
            (CHANGE_MS + BROADCAST_CLOCK_TOLERANCE_MS) as u64 * NANOSECONDS_PER_MILLISECOND as u64;
        index.expire_time(after_boundary);
        assert!(
            index
                .program_boundaries_ms()
                .iter()
                .all(|ms| *ms > CHANGE_MS + BROADCAST_CLOCK_TOLERANCE_MS)
        );
        for anchor in &mut index.entries {
            if let Some(observation) = &mut anchor.programs {
                Arc::make_mut(observation).information.time = None;
            }
        }
        assert!(index.program_boundaries_ms().is_empty());
    }
}
