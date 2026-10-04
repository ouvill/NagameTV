//! Broadcaster Information Table (BIT) on PID 0x0024.

use super::{Error, Reader};
use std::collections::BTreeMap;

pub const PID: u16 = 0x0024;
const TABLE_ID: u8 = 0xc4;
const SERVICE_LIST_DESCRIPTOR: u8 = 0x41;
const EXTENDED_BROADCASTER_DESCRIPTOR: u8 = 0xce;
const TERRESTRIAL_BROADCASTER: u8 = 1;
const DESCRIPTOR_LENGTH_MASK: u16 = 0x0fff;
const BIT_HEADER_LENGTH: usize = 8;
const CRC_LENGTH: usize = 4;
const MAX_BIT_SECTION_LENGTH: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BroadcasterInformation {
    pub original_network_id: u16,
    pub broadcasters: Vec<Broadcaster>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Broadcaster {
    pub id: u8,
    pub services: Vec<BroadcastService>,
    pub affiliations: Vec<u8>,
    pub affiliation_broadcasters: Vec<AffiliationBroadcaster>,
    pub terrestrial_broadcaster_id: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BroadcastService {
    pub service_type: u8,
    pub service_id: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AffiliationBroadcaster {
    pub original_network_id: u16,
    pub broadcaster_id: u8,
}

fn descriptors(bytes: &[u8], broadcaster: &mut Broadcaster) -> Result<(), Error> {
    let mut reader = Reader::new(bytes);
    while !reader.bytes.is_empty() {
        let tag = reader.u8()?;
        let length = usize::from(reader.u8()?);
        let mut value = Reader::new(reader.take(length)?);
        match tag {
            SERVICE_LIST_DESCRIPTOR => {
                if value.bytes.len() % 3 != 0 {
                    return Err(Error::Invalid("BIT service list length"));
                }
                while !value.bytes.is_empty() {
                    broadcaster.services.push(BroadcastService {
                        service_id: value.u16()?,
                        service_type: value.u8()?,
                    });
                }
            }
            EXTENDED_BROADCASTER_DESCRIPTOR => {
                if value.u8()? >> 4 != TERRESTRIAL_BROADCASTER {
                    continue;
                }
                broadcaster.terrestrial_broadcaster_id = Some(value.u16()?);
                let counts = value.u8()?;
                for _ in 0..counts >> 4 {
                    broadcaster.affiliations.push(value.u8()?);
                }
                for _ in 0..counts & 0x0f {
                    broadcaster
                        .affiliation_broadcasters
                        .push(AffiliationBroadcaster {
                            original_network_id: value.u16()?,
                            broadcaster_id: value.u8()?,
                        });
                }
                // Remaining private data is broadcaster-defined.
            }
            _ => {}
        }
    }
    Ok(())
}

fn parse_broadcasters(body: &[u8]) -> Result<Vec<Broadcaster>, Error> {
    let mut reader = Reader::new(body);
    let first_length = usize::from(reader.u16()? & DESCRIPTOR_LENGTH_MASK);
    reader.take(first_length)?; // First descriptors describe the table, not a broadcaster.
    let mut broadcasters = Vec::new();
    while !reader.bytes.is_empty() {
        let id = reader.u8()?;
        let length = usize::from(reader.u16()? & DESCRIPTOR_LENGTH_MASK);
        let mut broadcaster = Broadcaster {
            id,
            services: Vec::new(),
            affiliations: Vec::new(),
            affiliation_broadcasters: Vec::new(),
            terrestrial_broadcaster_id: None,
        };
        descriptors(reader.take(length)?, &mut broadcaster)?;
        broadcasters.push(broadcaster);
    }
    Ok(broadcasters)
}

#[derive(Default)]
pub(super) struct Sections {
    current: Option<Version>,
    published: Option<BroadcasterInformation>,
}

struct Version {
    network_id: u16,
    number: u8,
    last_section: u8,
    sections: BTreeMap<u8, Vec<Broadcaster>>,
}

impl Sections {
    pub fn push(&mut self, raw: &[u8]) -> Result<Option<BroadcasterInformation>, Error> {
        if raw.first() != Some(&TABLE_ID) {
            return Ok(None);
        }
        let size = viewer_mpegts::private_section_size(raw)
            .map_err(|error| super::psi_error(error, "BIT section"))?;
        if raw.len() != size
            || size > MAX_BIT_SECTION_LENGTH
            || size < BIT_HEADER_LENGTH + CRC_LENGTH
        {
            return Err(Error::Invalid("BIT section length"));
        }
        if raw[1] & 0xf0 != 0xf0 || raw[5] & 0xc1 != 0xc1 {
            return Err(Error::Invalid("BIT section syntax"));
        }
        if viewer_mpegts::crc32_mpeg(raw) != 0 {
            return Err(Error::Invalid("BIT CRC-32"));
        }
        let network_id = u16::from_be_bytes([raw[3], raw[4]]);
        let version = (raw[5] >> 1) & 0x1f;
        let section_number = raw[6];
        let last_section_number = raw[7];
        if section_number > last_section_number {
            return Err(Error::Invalid("BIT section numbering"));
        }
        let reset = !matches!(&self.current, Some(current)
            if current.network_id == network_id
                && current.number == version
                && current.last_section == last_section_number);
        if reset {
            self.current = Some(Version {
                network_id,
                number: version,
                last_section: last_section_number,
                sections: BTreeMap::new(),
            });
        }
        let broadcasters = parse_broadcasters(&raw[BIT_HEADER_LENGTH..size - CRC_LENGTH])?;
        let current = self.current.as_mut().ok_or(Error::Invalid("BIT version"))?;
        current.sections.insert(section_number, broadcasters);
        if current.sections.len() != usize::from(current.last_section) + 1 {
            return Ok(None);
        }
        let information = BroadcasterInformation {
            original_network_id: current.network_id,
            broadcasters: current.sections.values().flatten().cloned().collect(),
        };
        if self.published.as_ref() == Some(&information) {
            return Ok(None);
        }
        self.published = Some(information.clone());
        Ok(Some(information))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // CRC-checked BIT section captured from service 2088 (ONID 0x7fd5).
    const LIVE_BIT: [u8; 89] = [
        0xc4, 0xf0, 0x56, 0x7f, 0xd5, 0xc3, 0x00, 0x00, 0xf0, 0x24, 0xd7, 0x22, 0xff, 0xe3, 0xdb,
        0x40, 0x01, 0x01, 0xc4, 0x01, 0x01, 0x42, 0x01, 0x02, 0x4e, 0x04, 0x01, 0x01, 0x01, 0x22,
        0x50, 0x0e, 0x4f, 0x08, 0x06, 0x0e, 0x03, 0x03, 0x13, 0x10, 0xcf, 0x02, 0x06, 0x0d, 0x00,
        0x03, 0xff, 0xf0, 0x24, 0xce, 0x05, 0x1f, 0x7f, 0xd5, 0x10, 0x02, 0xd7, 0x1b, 0xff, 0xe3,
        0xdb, 0x4e, 0x04, 0xff, 0x00, 0x05, 0x05, 0x58, 0x0c, 0x6f, 0x08, 0x06, 0x0d, 0x03, 0x10,
        0xef, 0x02, 0x06, 0x0d, 0x00, 0x20, 0xc8, 0x02, 0x06, 0x00, 0xb7, 0x83, 0x3e, 0x78,
    ];

    #[test]
    fn live_bit_supplies_terrestrial_nvram_affiliation_once() {
        let mut sections = Sections::default();
        let information = sections.push(&LIVE_BIT).unwrap().unwrap();
        assert_eq!(information.original_network_id, 0x7fd5);
        assert_eq!(information.broadcasters.len(), 1);
        assert_eq!(information.broadcasters[0].id, 255);
        assert_eq!(information.broadcasters[0].affiliations, [2]);
        assert_eq!(
            information.broadcasters[0].terrestrial_broadcaster_id,
            Some(0x7fd5)
        );
        assert_eq!(sections.push(&LIVE_BIT).unwrap(), None);
    }

    #[test]
    fn bit_waits_for_every_section_before_publishing() {
        fn section(number: u8) -> Vec<u8> {
            let mut bytes = LIVE_BIT[..LIVE_BIT.len() - CRC_LENGTH].to_vec();
            bytes[6] = number;
            bytes[7] = 1;
            let crc = viewer_mpegts::crc32_mpeg(&bytes);
            bytes.extend_from_slice(&crc.to_be_bytes());
            bytes
        }
        let mut sections = Sections::default();
        assert_eq!(sections.push(&section(1)).unwrap(), None);
        let information = sections.push(&section(0)).unwrap().unwrap();
        assert_eq!(information.broadcasters.len(), 2);
        assert_eq!(sections.push(&section(1)).unwrap(), None);
    }
}
