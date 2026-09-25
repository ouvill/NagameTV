//! ARIB STD-B10 SI shared by recording, live and time-shift playback.
pub(crate) mod catalog;
pub(crate) mod presentation;
mod syntax;
use super::{
    Sections,
    wire::{Pid, TS_PACKET_SIZE, TransportPacket, same_payload_packet},
};
use serde::Serialize;
use std::collections::HashMap;
pub(super) use syntax::section_size;

const SDT_PID: Pid = Pid(0x11);
const TIME_TABLE_PID: Pid = Pid(0x14);
const EIT_PIDS: [Pid; 3] = [Pid(0x12), Pid(0x26), Pid(0x27)];

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Program {
    pub event_id: u16,
    pub service_id: u16,
    pub network_id: u16,
    pub transport_stream_id: u16,
    pub start_at: Option<i64>,
    pub duration: Option<u64>,
    pub name: String,
    pub description: String,
    pub extended: String,
    pub genres: Vec<(u8, u8)>,
    #[serde(skip_serializing)]
    pub audios: Box<[crate::audio::Descriptor]>,
}
impl Program {
    pub fn audio_heap_bytes(&self) -> usize {
        std::mem::size_of_val(self.audios.as_ref())
            + self
                .audios
                .iter()
                .map(crate::audio::Descriptor::heap_bytes)
                .sum::<usize>()
    }
}

/// No section received is not an empty, valid present section.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) enum Present {
    #[default]
    Unknown,
    Empty,
    Event(Program),
}
impl Present {
    pub fn as_ref(&self) -> Option<&Program> {
        match self {
            Self::Event(program) => Some(program),
            Self::Unknown | Self::Empty => None,
        }
    }
    pub fn iter(&self) -> impl Iterator<Item = &Program> {
        self.as_ref().into_iter()
    }
    #[cfg(test)]
    pub fn is_none(&self) -> bool {
        self.as_ref().is_none()
    }
}
impl From<Option<Program>> for Present {
    fn from(program: Option<Program>) -> Self {
        match program {
            Some(program) => Self::Event(program),
            None => Self::Empty,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Information {
    pub station: String,
    pub provider: String,
    pub current: Present,
    pub next: Option<Program>,
    pub time: Option<(u64, i64)>, // PCR ticks, UTC epoch milliseconds (wire time is JST).
    pub service: Option<crate::channels::BroadcastService>,
}
#[derive(Clone, Debug)]
pub(crate) struct Observation {
    pub pcr: u64,
    pub information: Information,
}

struct Assembly {
    sections: Sections,
    previous: Option<(u8, [u8; TS_PACKET_SIZE])>,
}
impl Default for Assembly {
    fn default() -> Self {
        Self {
            sections: Sections::si(),
            previous: None,
        }
    }
}

pub(crate) struct Collector {
    service: u16,
    transport: Option<u16>,
    network: Option<u16>,
    pcr_pid: Option<Pid>,
    pcr: Option<u64>,
    assemblies: HashMap<Pid, Assembly>,
    information: Information,
    changed: bool,
}
impl Collector {
    pub fn new(service: u16) -> Self {
        Self {
            service,
            transport: None,
            network: None,
            pcr_pid: None,
            pcr: None,
            assemblies: HashMap::new(),
            information: Information::default(),
            changed: false,
        }
    }
    pub fn reset(&mut self) {
        *self = Self::new(self.service);
    }
    pub fn transport(&mut self, transport: u16) {
        if self.transport != Some(transport) {
            self.reset();
            self.transport = Some(transport);
        }
    }
    pub fn unselect(&mut self) {
        self.changed |= self.information != Information::default();
        self.information = Information::default();
        self.pcr_pid = None;
        self.assemblies.clear();
    }
    pub fn pcr_pid(&mut self, pid: Pid) {
        if self.pcr_pid != Some(pid) {
            self.pcr_pid = Some(pid);
            self.pcr = None;
        }
    }
    pub fn reset_clock(&mut self) {
        self.information = Information::default();
        self.pcr = None;
        self.changed = true;
        self.assemblies.clear();
    }
    pub fn packet(&mut self, packet: &TransportPacket<'_>, bytes: &[u8; TS_PACKET_SIZE]) {
        if self.pcr_pid == Some(packet.pid) {
            if packet.discontinuity {
                self.information = Information::default();
                self.pcr = None;
                self.changed = true;
            }
            if let Some(pcr) = packet.pcr {
                self.pcr = Some(pcr);
            }
        }
        if self.pcr_pid.is_none() {
            return;
        }
        if ![SDT_PID, TIME_TABLE_PID].contains(&packet.pid) && !EIT_PIDS.contains(&packet.pid) {
            return;
        }
        let assembly = self.assemblies.entry(packet.pid).or_default();
        if !packet.payload.is_empty()
            && assembly.previous.as_ref().is_some_and(|(counter, old)| {
                *counter == packet.continuity_counter && same_payload_packet(old, bytes)
            })
        {
            return;
        }
        if packet.discontinuity
            || (!packet.payload.is_empty()
                && assembly
                    .previous
                    .as_ref()
                    .is_some_and(|(counter, _)| packet.continuity_counter != (counter + 1) % 16))
        {
            *assembly = Assembly::default();
        }
        if packet.payload.is_empty() {
            return;
        }
        assembly.previous = Some((packet.continuity_counter, *bytes));
        let sections = assembly.sections.push(packet.start, packet.payload);
        for section in sections {
            self.section(packet.pid, &section);
        }
    }
    fn section(&mut self, pid: Pid, data: &[u8]) {
        let Some(transport) = self.transport else {
            return;
        };
        match (pid, data.first().copied()) {
            (TIME_TABLE_PID, Some(syntax::TDT_TABLE_ID | syntax::TOT_TABLE_ID)) => {
                if let (Some(pcr), Some(time)) = (self.pcr, syntax::time_table(data)) {
                    self.changed |= replace_changed(&mut self.information.time, Some((pcr, time)));
                }
            }
            (SDT_PID, Some(syntax::SDT_ACTUAL_TABLE_ID)) => {
                if let Some((network, provider, name)) =
                    syntax::station(data, transport, self.service)
                    && self.network.is_none_or(|old| old == network)
                {
                    self.network = Some(network);
                    self.changed |= replace_changed(
                        &mut self.information.service,
                        Some(crate::channels::BroadcastService {
                            network_id: network,
                            service_id: self.service,
                        }),
                    );
                    self.changed |= replace_changed(&mut self.information.station, name);
                    self.changed |= replace_changed(&mut self.information.provider, provider);
                }
            }
            (pid, Some(syntax::EIT_ACTUAL_PF_TABLE_ID)) if EIT_PIDS.contains(&pid) => {
                if let Some(event) = syntax::event(data, transport, self.service)
                    && self.network.is_none_or(|network| network == event.network)
                {
                    self.network = Some(event.network);
                    self.changed |= replace_changed(
                        &mut self.information.service,
                        Some(crate::channels::BroadcastService {
                            network_id: event.network,
                            service_id: self.service,
                        }),
                    );
                    let slot = usize::from(event.number);
                    if slot == 0 {
                        self.changed |=
                            replace_changed(&mut self.information.current, event.program.into());
                    } else {
                        self.changed |= replace_changed(&mut self.information.next, event.program);
                    }
                }
            }
            _ => {}
        }
    }
    pub fn take(&mut self) -> Option<Observation> {
        let pcr = self.pcr?;
        if !std::mem::take(&mut self.changed) {
            return None;
        }
        Some(Observation {
            pcr,
            information: self.information.clone(),
        })
    }
}

// Compare only the fields a section can change. Repeated SI must not clone the
// current/next program text, audio descriptors and station just to detect changes.
fn replace_changed<T: PartialEq>(current: &mut T, next: T) -> bool {
    if *current == next {
        return false;
    }
    *current = next;
    true
}

#[cfg(test)]
mod audio_tests;

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_section(pid: Pid) -> Vec<u8> {
        let bytes = include_bytes!("../../../tests/fixtures/recording-seek.ts");
        let mut assembly = Sections::si();
        bytes
            .as_chunks::<TS_PACKET_SIZE>()
            .0
            .iter()
            .find_map(|bytes| {
                let packet = TransportPacket::parse(bytes).expect("valid fixture packet");
                if packet.pid != pid {
                    return None;
                }
                assembly
                    .push(packet.start, packet.payload)
                    .into_iter()
                    .next()
            })
            .expect("fixture section")
    }

    #[test]
    fn repeated_sections_do_not_publish_changes_and_clock_preserves_programs() {
        const PCR_TICKS_PER_SECOND: u64 = 90_000;
        let mut collector = Collector::new(1);
        collector.transport(1);
        collector.pcr = Some(PCR_TICKS_PER_SECOND);
        let event = fixture_section(EIT_PIDS[0]);
        let station = fixture_section(SDT_PID);
        let clock = fixture_section(TIME_TABLE_PID);
        for (pid, section) in [
            (EIT_PIDS[0], &event),
            (SDT_PID, &station),
            (TIME_TABLE_PID, &clock),
        ] {
            collector.section(pid, section);
            assert!(collector.take().is_some());
            collector.section(pid, section);
            assert!(collector.take().is_none());
        }
        let accepted = collector.information.clone();
        collector.pcr = Some(2 * PCR_TICKS_PER_SECOND);
        collector.section(TIME_TABLE_PID, &clock);
        // An unchanged section must not erase an earlier change before take().
        collector.section(EIT_PIDS[0], &event);
        let update = collector
            .take()
            .expect("new PCR/UTC correlation")
            .information;
        assert_ne!(update.time, accepted.time);
        assert_eq!(update.current, accepted.current);
        assert_eq!(update.next, accepted.next);
        assert_eq!(update.station, accepted.station);
        assert_eq!(update.provider, accepted.provider);
        assert!(collector.take().is_none());
        collector.section(TIME_TABLE_PID, &clock[..1]);
        assert!(collector.take().is_none());
    }

    #[test]
    fn generated_ts_contains_distinct_japanese_events_and_a_broadcast_clock() {
        // Exercise the same packet/section/PAT/PMT/descriptor path as playback,
        // including arbitrarily split input, without devices or GStreamer.
        let mut parser = crate::features::subtitles::transport::TransportParser::new(false);
        parser.select_service(1);
        parser.enable_programs(1);
        let bytes = include_bytes!("../../../tests/fixtures/recording-seek.ts");
        let mut observations = Vec::new();
        for chunk in bytes.chunks(113) {
            parser.push(chunk);
            if let Some(value) = parser.take_programs() {
                observations.push(value);
            }
        }
        for id in [1, 2] {
            let information = observations
                .iter()
                .map(|o| &o.information)
                .find(|i| i.current.as_ref().is_some_and(|p| p.event_id == id) && i.time.is_some())
                .expect("complete event");
            let event = information.current.as_ref().unwrap();
            assert_eq!(information.station, "日本語 TV");
            assert_eq!(event.name, format!("日本語 {id}"));
            assert_eq!(event.duration, Some(30_000));
            assert_eq!(event.extended, "Extended description");
            assert_eq!(event.genres, vec![(1, 0)]);
        }
        parser.discontinuity();
        assert!(parser.take_programs().is_none());
        let mut foreign = Collector::new(99);
        foreign.transport(1);
        foreign.pcr_pid(Pid(0x41));
        for data in bytes.as_chunks::<188>().0 {
            if let Ok(packet) = TransportPacket::parse(data) {
                foreign.packet(&packet, data);
            }
        }
        assert!(foreign.information.current.is_none());
    }
    #[test]
    fn corrupt_and_noncurrent_si_never_replaces_the_accepted_event() {
        let bytes = include_bytes!("../../../tests/fixtures/recording-seek.ts");
        let mut assembly = Sections::si();
        let section = bytes
            .as_chunks::<188>()
            .0
            .iter()
            .find_map(|bytes| {
                let packet = TransportPacket::parse(bytes).ok()?;
                if packet.pid != Pid(0x12) {
                    return None;
                }
                assembly
                    .push(packet.start, packet.payload)
                    .into_iter()
                    .next()
            })
            .unwrap();
        let mut collector = Collector::new(1);
        collector.transport(1);
        collector.section(Pid(0x12), &section);
        assert_eq!(collector.information.current.as_ref().unwrap().event_id, 1);
        let mut corrupt = section.clone();
        corrupt[14] ^= 1;
        collector.section(Pid(0x12), &corrupt);
        assert_eq!(collector.information.current.as_ref().unwrap().event_id, 1);
        let mut future = section[..section.len() - 4].to_vec();
        future[5] &= !1;
        let crc = super::super::wire::crc32_mpeg(&future);
        future.extend_from_slice(&crc.to_be_bytes());
        collector.section(Pid(0x12), &future);
        assert_eq!(collector.information.current.as_ref().unwrap().event_id, 1);
        for end in 0..section.len() {
            collector.section(Pid(0x12), &section[..end]);
        }
        assert_eq!(collector.information.current.as_ref().unwrap().event_id, 1);
    }
}
