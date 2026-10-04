//! Reuse the application's ARIB SI collector on the same raw TS as the carousel.
//! Broadcast time and program identity must not come from the host wall clock.
use crate::transport::{
    programs::Collector,
    wire::{Pid, TS_PACKET_SIZE, TransportPacket},
};
use arib_b24::transport::ServiceProgram;
use viewer_web_bml::{ProgramEvent, ProgramInfo};

pub(super) enum Update {
    Program(ProgramInfo),
    Time(i64),
    Clock { base: u64, extension: u16 },
}

pub(super) struct Metadata {
    collector: Collector,
    selection: Option<ServiceProgram>,
    original_network_id: Option<u16>,
    program: ProgramInfo,
    time: Option<(u64, i64)>,
}

impl Metadata {
    pub fn new(service_id: u16, original_network_id: Option<u16>) -> Self {
        Self {
            collector: Collector::new(service_id),
            selection: None,
            original_network_id,
            program: ProgramInfo {
                service_id,
                original_network_id,
                transport_stream_id: None,
                event: None,
            },
            time: None,
        }
    }

    pub fn push(
        &mut self,
        selection: Option<ServiceProgram>,
        packet: &TransportPacket<'_>,
        bytes: &[u8; TS_PACKET_SIZE],
    ) -> Vec<Update> {
        let mut updates = Vec::new();
        if self.selection != selection {
            self.selection = selection;
            self.collector.reset();
            self.time = None;
            self.program.original_network_id = self.original_network_id;
            self.program.transport_stream_id =
                selection.map(|program| program.transport_stream_id());
            self.program.event = None;
            if let Some(program) = selection {
                self.collector.transport(program.transport_stream_id());
                self.collector.pcr_pid(Pid(program.pcr_pid()));
            }
            updates.push(Update::Program(self.program.clone()));
        }
        let Some(selected) = selection else {
            return updates;
        };
        self.collector.packet(packet, bytes);
        if packet.pid.0 == selected.pcr_pid()
            && let (Some(base), Some(extension)) = (packet.pcr, packet.pcr_extension())
        {
            updates.push(Update::Clock { base, extension });
        }
        if let Some(observation) = self.collector.take() {
            let information = observation.information;
            let program = ProgramInfo {
                service_id: selected.service_id(),
                original_network_id: information
                    .service
                    .map(|service| service.network_id)
                    .or(self.original_network_id),
                transport_stream_id: Some(selected.transport_stream_id()),
                event: information.current.as_ref().map(|event| ProgramEvent {
                    id: event.event_id,
                    name: event.name.clone(),
                    start_unix_ms: event.start_at,
                    duration_seconds: event.duration.map(|ms| ms / 1000),
                }),
            };
            if program != self.program {
                updates.push(Update::Program(program.clone()));
                self.program = program;
            }
            if information.time != self.time {
                self.time = information.time;
                if let Some((_, time)) = self.time {
                    updates.push(Update::Time(time));
                }
            }
        }
        updates
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_ts_supplies_program_changes_and_broadcast_time() {
        let mut receiver = arib_b24::transport::ServiceReceiver::new(1).unwrap();
        let mut metadata = Metadata::new(1, None);
        let mut events = std::collections::BTreeMap::new();
        let mut times = Vec::new();
        let mut clocks = 0;
        for bytes in include_bytes!("../../../../tests/fixtures/recording-seek.ts")
            .as_chunks::<TS_PACKET_SIZE>()
            .0
        {
            receiver.push_updates(bytes).unwrap();
            let packet = TransportPacket::parse(bytes).unwrap();
            for update in metadata.push(receiver.program(), &packet, bytes) {
                match update {
                    Update::Program(program) => {
                        if let Some(event) = &program.event {
                            assert_eq!(program.original_network_id, Some(1));
                            assert_eq!(program.transport_stream_id, Some(1));
                            assert_eq!(event.name, format!("日本語 {}", event.id));
                            assert_eq!(event.duration_seconds, Some(30));
                            events.insert(event.id, event.clone());
                        }
                    }
                    Update::Time(time) => times.push(time),
                    Update::Clock { base, extension } => {
                        assert_eq!(Some(base), packet.pcr);
                        assert_eq!(Some(extension), packet.pcr_extension());
                        clocks += 1;
                    }
                }
            }
        }
        assert_eq!(events.keys().copied().collect::<Vec<_>>(), [1, 2]);
        assert!(clocks > 0);
        let start = events[&1].start_unix_ms.unwrap();
        assert!(
            times
                .iter()
                .all(|time| (start..start + 60_000).contains(time))
        );
        assert!(times.len() > 1);
        assert_eq!(events[&2].start_unix_ms, Some(start + 30_000));

        // Losing the selected program invalidates both event and TS identity.
        let bytes =
            &include_bytes!("../../../../tests/fixtures/recording-seek.ts")[..TS_PACKET_SIZE];
        let bytes: &[u8; TS_PACKET_SIZE] = bytes.try_into().unwrap();
        let packet = TransportPacket::parse(bytes).unwrap();
        assert!(
            matches!(metadata.push(None, &packet, bytes).as_slice(), [Update::Program(program)] if program.event.is_none() && program.transport_stream_id.is_none())
        );
    }
}
