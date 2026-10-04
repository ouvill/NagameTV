//! STD-B24 Part 3, Chapter 7 stream-descriptor/event-message syntax.
//! Timing policy belongs to the application clock and BML engine.

use super::{Descriptor, Error, Reader, crc32, descriptors};

const EVENT_TABLE: u8 = 0x3d;
const GENERAL_EVENT: u8 = 0x40;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventTime {
    Immediate,
    Absolute {
        mjd: u16,
        bcd_hms: [u8; 3],
        broadcast_time: bool,
    },
    Npt(u64),
    RelativeBcd([u8; 5]),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventMessage {
    pub group_id: u16,
    pub time: EventTime,
    pub message_type: u8,
    pub message_id: u16,
    pub private_data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventSection {
    pub data_event_id: u8,
    pub group_id: u16,
    pub version: u8,
    pub section_number: u8,
    pub last_section_number: u8,
    pub messages: Vec<EventMessage>,
    /// Includes NPT reference and broadcaster-defined descriptors.
    pub other_descriptors: Vec<Descriptor>,
}

impl EventSection {
    /// Parses a complete table 0x3D section including its MPEG-2 CRC-32.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() < 12 {
            return Err(Error::Truncated("event section"));
        }
        if bytes[0] != EVENT_TABLE || bytes[1] & 0xf0 != 0xb0 {
            return Err(Error::Invalid("event section header"));
        }
        let length = usize::from(u16::from_be_bytes([bytes[1] & 0x0f, bytes[2]]));
        if !(9..=super::MAX_SECTION_LENGTH).contains(&length) || bytes.len() != length + 3 {
            return Err(Error::Invalid("event section length"));
        }
        if bytes[5] & 0xc1 != 0xc1 || bytes[6] > bytes[7] {
            return Err(Error::Invalid("event section version flags"));
        }
        if crc32(bytes) != 0 {
            return Err(Error::Invalid("event section CRC-32"));
        }
        let extension = u16::from_be_bytes([bytes[3], bytes[4]]);
        let data_event_id = (extension >> 12) as u8;
        let group_id = extension & 0x0fff;
        let version = (bytes[5] >> 1) & 0x1f;
        let section_number = bytes[6];
        let last_section_number = bytes[7];
        let mut messages = Vec::new();
        let mut other_descriptors = Vec::new();
        for descriptor in descriptors(&bytes[8..bytes.len() - 4])? {
            if descriptor.tag == GENERAL_EVENT {
                let message = EventMessage::parse(&descriptor.data)?;
                if message.group_id != group_id {
                    return Err(Error::Invalid("event group ID"));
                }
                messages.push(message);
            } else {
                other_descriptors.push(descriptor);
            }
        }
        Ok(Self {
            data_event_id,
            group_id,
            version,
            section_number,
            last_section_number,
            messages,
            other_descriptors,
        })
    }
}

impl EventMessage {
    fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let mut reader = Reader::new(bytes);
        let group_field = reader.u16()?;
        let group_id = group_field >> 4;
        let mode = reader.u8()?;
        let raw: [u8; 5] = reader
            .take(5)?
            .try_into()
            .map_err(|_| Error::Truncated("event time"))?;
        let time = match mode {
            0x00 => EventTime::Immediate,
            0x01 | 0x05 => EventTime::Absolute {
                mjd: u16::from_be_bytes([raw[0], raw[1]]),
                bcd_hms: [raw[2], raw[3], raw[4]],
                broadcast_time: mode == 0x05,
            },
            0x02 => EventTime::Npt(
                u64::from_be_bytes([0, 0, 0, raw[0], raw[1], raw[2], raw[3], raw[4]])
                    & ((1_u64 << 33) - 1),
            ),
            0x03 => EventTime::RelativeBcd(raw),
            _ => return Err(Error::Invalid("event time mode")),
        };
        let message_type = reader.u8()?;
        let message_id = reader.u16()?;
        Ok(Self {
            group_id,
            time,
            message_type,
            message_id,
            private_data: reader.bytes.to_vec(),
        })
    }
}
