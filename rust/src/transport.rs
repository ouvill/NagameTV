//! Transport syntax and program discovery shared by playback and subtitles.
pub(crate) mod programs;
mod psi;
pub(crate) mod wire;
pub(crate) use psi::{Pat, Sections};
use wire::{Pid, PsiSection, SYNC_BYTE, TS_PACKET_SIZE, TransportPacket};

/// Choose the lowest service in the first complete, current, CRC-checked PAT.
/// Discovery neither constructs a caption decoder nor depends on a GUI/runtime.
pub(crate) fn recording_service(data: &[u8]) -> Option<u16> {
    let mut pat = Pat::default();
    let mut sections = Sections::default();
    let mut previous: Option<(u8, [u8; TS_PACKET_SIZE])> = None;
    let mut offset = 0;
    while offset < data.len() {
        offset += data[offset..].iter().position(|byte| *byte == SYNC_BYTE)?;
        let bytes: &[u8; TS_PACKET_SIZE] =
            data.get(offset..offset + TS_PACKET_SIZE)?.try_into().ok()?;
        if data
            .get(offset + TS_PACKET_SIZE)
            .is_some_and(|byte| *byte != SYNC_BYTE)
        {
            offset += 1;
            continue;
        }
        offset += TS_PACKET_SIZE;
        let Ok(packet) = TransportPacket::parse(bytes) else {
            continue;
        };
        if packet.pid != Pid::PAT {
            continue;
        }
        if !packet.payload.is_empty()
            && previous.as_ref().is_some_and(|(counter, old)| {
                *counter == packet.continuity_counter && wire::same_payload_packet(old, bytes)
            })
        {
            continue;
        }
        if packet.discontinuity
            || (!packet.payload.is_empty()
                && previous
                    .as_ref()
                    .is_some_and(|(counter, _)| packet.continuity_counter != (counter + 1) % 16))
        {
            sections = Sections::default();
            pat = Pat::default();
            previous = None;
        }
        if packet.payload.is_empty() {
            continue;
        }
        previous = Some((packet.continuity_counter, *bytes));
        for data in sections.push(packet.start, packet.payload) {
            if let Ok(section) = PsiSection::parse(&data)
                && let Some(programs) = pat.push(&section)
                && let Some(service) = programs.keys().next()
            {
                return Some(*service);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    fn packet(service: u16, number: u8, last: u8, version: u8, counter: u8) -> Vec<u8> {
        let mut section = vec![0, 0xb0, 13, 0, 1, 0xc1 | (version << 1), number, last];
        section.extend_from_slice(&service.to_be_bytes());
        section.extend_from_slice(&[0xe1, service as u8]);
        let mut crc = 0xffff_ffff_u32;
        for byte in &section {
            crc ^= u32::from(*byte) << 24;
            for _ in 0..8 {
                crc = (crc << 1)
                    ^ if crc & 0x8000_0000 != 0 {
                        0x04c1_1db7
                    } else {
                        0
                    };
            }
        }
        section.extend_from_slice(&crc.to_be_bytes());
        let mut packet = vec![SYNC_BYTE, 0x40, 0, 0x10 | counter, 0];
        packet.extend(section);
        packet.resize(TS_PACKET_SIZE, 0xff);
        packet
    }
    #[test]
    fn discovery_requires_complete_pat_and_selects_lowest_service() {
        let first = packet(42, 0, 1, 0, 0);
        assert_eq!(recording_service(&first), None);
        let mut data = vec![0, 1, 2]; // Recover packet alignment after leading junk.
        data.extend(first);
        data.extend(packet(12, 1, 1, 0, 1));
        assert_eq!(recording_service(&data), Some(12));
        let mut corrupt = packet(42, 0, 0, 0, 0);
        corrupt[16] ^= 1;
        assert_eq!(recording_service(&corrupt), None);
        assert_eq!(recording_service(&corrupt[..80]), None);
    }
    #[test]
    fn missing_packets_and_pat_versions_cannot_be_combined() {
        let mut data = packet(42, 0, 1, 0, 0);
        data.extend(packet(12, 1, 1, 0, 2)); // Lost continuity.
        assert_eq!(recording_service(&data), None);
        let mut data = packet(42, 0, 1, 0, 0);
        data.extend(packet(12, 1, 1, 1, 1)); // New version, incomplete.
        assert_eq!(recording_service(&data), None);
        data.extend(packet(43, 0, 1, 1, 2));
        assert_eq!(recording_service(&data), Some(12));
    }
}
