//! Bounded, CRC-checked descriptors. ARIB B10 Part 2, SI tables and descriptors.
use super::Program;
use crate::transport::wire::{ParseError, crc32_mpeg};
use gstreamer_mpegts::ffi;
use std::collections::BTreeMap;

// ARIB STD-B10 SI table IDs and descriptor tags are separate namespaces.
// These IDs are shared with DVB. Use the existing library definitions, narrowed
// from C enum integers to their one-byte wire representation (no native calls).
pub(super) const SDT_ACTUAL_TABLE_ID: u8 =
    ffi::GST_MTS_TABLE_ID_SERVICE_DESCRIPTION_ACTUAL_TS as u8;
pub(super) const EIT_ACTUAL_PF_TABLE_ID: u8 =
    ffi::GST_MTS_TABLE_ID_EVENT_INFORMATION_ACTUAL_TS_PRESENT as u8;
pub(super) const TDT_TABLE_ID: u8 = ffi::GST_MTS_TABLE_ID_TIME_DATE as u8;
pub(super) const TOT_TABLE_ID: u8 = ffi::GST_MTS_TABLE_ID_TIME_OFFSET as u8;
const SERVICE_DESCRIPTOR_TAG: u8 = ffi::GST_MTS_DESC_DVB_SERVICE as u8;
const SHORT_EVENT_DESCRIPTOR_TAG: u8 = ffi::GST_MTS_DESC_DVB_SHORT_EVENT as u8;
const EXTENDED_EVENT_DESCRIPTOR_TAG: u8 = ffi::GST_MTS_DESC_DVB_EXTENDED_EVENT as u8;
const CONTENT_DESCRIPTOR_TAG: u8 = ffi::GST_MTS_DESC_DVB_CONTENT as u8;
const CONTENT_ENTRY_SIZE: usize = 2;
const JAPANESE_LANGUAGE_CODE: &[u8] = b"jpn";

pub(crate) fn section_size(data: &[u8]) -> Result<usize, ParseError> {
    if data.len() < 3 {
        return Err(ParseError::Incomplete);
    }
    if data[1] & 0x30 != 0x30 {
        return Err(ParseError::Invalid("SI reserved bits"));
    }
    let length = usize::from(u16::from_be_bytes([data[1] & 15, data[2]]));
    let valid = match data[0] {
        SDT_ACTUAL_TABLE_ID | EIT_ACTUAL_PF_TABLE_ID => {
            data[1] & 0xc0 == 0xc0
                && (9..=if data[0] == EIT_ACTUAL_PF_TABLE_ID {
                    4093
                } else {
                    1021
                })
                    .contains(&length)
        }
        TDT_TABLE_ID => data[1] & 0xc0 == 0x40 && length == 5,
        TOT_TABLE_ID => data[1] & 0xc0 == 0x40 && (11..=1021).contains(&length),
        _ => false,
    };
    if !valid {
        return Err(ParseError::Invalid("SI table syntax/length"));
    }
    Ok(length + 3)
}
fn checked(data: &[u8]) -> Option<&[u8]> {
    if section_size(data).ok()? != data.len() || (data[0] != TDT_TABLE_ID && crc32_mpeg(data) != 0)
    {
        return None;
    }
    if matches!(data[0], SDT_ACTUAL_TABLE_ID | EIT_ACTUAL_PF_TABLE_ID)
        && (data[5] & 0xc1 != 0xc1 || data[6] > data[7])
    {
        return None;
    }
    Some(data)
}
fn word(data: &[u8]) -> u16 {
    u16::from_be_bytes([data[0], data[1]])
}
fn bcd(byte: u8) -> Option<u64> {
    (byte >> 4 < 10 && byte & 15 < 10).then_some(u64::from(byte >> 4) * 10 + u64::from(byte & 15))
}
fn duration(bytes: &[u8]) -> Option<u64> {
    let (hours, minutes, seconds) = (
        bcd(*bytes.first()?)?,
        bcd(*bytes.get(1)?)?,
        bcd(*bytes.get(2)?)?,
    );
    (minutes < 60 && seconds < 60).then_some((hours * 3600 + minutes * 60 + seconds) * 1000)
}
fn date(bytes: &[u8]) -> Option<i64> {
    if bytes.len() != 5 || bytes == [255; 5] {
        return None;
    }
    let time = duration(&bytes[2..])?;
    if time >= 86_400_000 {
        return None;
    }
    Some((i64::from(word(bytes)) - 40587) * 86_400_000 + time as i64 - 9 * 3_600_000)
}
fn descriptors(mut bytes: &[u8]) -> Option<Vec<(u8, &[u8])>> {
    let mut values = Vec::new();
    while !bytes.is_empty() {
        let tag = *bytes.first()?;
        let size = usize::from(*bytes.get(1)?);
        let (body, rest) = bytes.get(2..)?.split_at_checked(size)?;
        values.push((tag, body));
        bytes = rest;
    }
    Some(values)
}
fn text(bytes: &[u8]) -> String {
    libaribcaption::text::decode(bytes)
}
fn string(bytes: &mut &[u8]) -> Option<String> {
    let length = usize::from(*bytes.first()?);
    let (body, rest) = bytes.get(1..)?.split_at_checked(length)?;
    *bytes = rest;
    Some(text(body))
}
pub(super) fn time_table(data: &[u8]) -> Option<i64> {
    let data = checked(data)?;
    if data[0] == TOT_TABLE_ID {
        if data[8] & 0xf0 != 0xf0 {
            return None;
        }
        let n = usize::from(word(&data[8..]) & 0x0fff);
        if n + 14 != data.len() {
            return None;
        }
        descriptors(&data[10..10 + n])?;
    }
    date(&data[3..8])
}
pub(super) fn station(data: &[u8], transport: u16, service: u16) -> Option<(u16, String, String)> {
    let data = checked(data)?;
    if word(&data[3..]) != transport || data.len() < 15 {
        return None;
    }
    let network = word(&data[8..]);
    let mut remaining = &data[11..data.len() - 4];
    while !remaining.is_empty() {
        let head = remaining.get(..5)?;
        let size = usize::from(word(&head[3..]) & 0x0fff);
        let (body, rest) = remaining.get(5..)?.split_at_checked(size)?;
        remaining = rest;
        if word(head) != service {
            continue;
        }
        for (tag, body) in descriptors(body)? {
            if tag == SERVICE_DESCRIPTOR_TAG {
                let mut bytes = body.get(1..)?;
                let provider = string(&mut bytes)?;
                let name = string(&mut bytes)?;
                if !bytes.is_empty() {
                    return None;
                }
                return Some((network, provider, name));
            }
        }
    }
    None
}
pub(super) struct Event {
    pub network: u16,
    pub number: u8,
    pub program: Option<Program>,
}
pub(super) fn event(data: &[u8], transport: u16, service: u16) -> Option<Event> {
    let data = checked(data)?;
    if data.len() < 18
        || word(&data[3..]) != service
        || word(&data[8..]) != transport
        || data[6] > 1
    {
        return None;
    }
    let network = word(&data[10..]);
    let mut event = Event {
        network,
        number: data[6],
        program: None,
    };
    let body = &data[14..data.len() - 4];
    if body.is_empty() {
        return Some(event);
    }
    let head = body.get(..12)?;
    let size = usize::from(word(&head[10..]) & 0x0fff);
    if body.len() != size + 12 {
        return None;
    } // p/f contains exactly one event per section.
    let mut program = Program {
        event_id: word(head),
        service_id: service,
        network_id: network,
        transport_stream_id: transport,
        start_at: date(&head[2..7]),
        duration: duration(&head[7..10]),
        name: String::new(),
        description: String::new(),
        extended: String::new(),
        genres: Vec::new(),
    };
    let mut fragments = BTreeMap::new();
    for (tag, body) in descriptors(&body[12..])? {
        match tag {
            SHORT_EVENT_DESCRIPTOR_TAG if body.starts_with(JAPANESE_LANGUAGE_CODE) => {
                let mut body = &body[3..];
                program.name = string(&mut body)?;
                program.description = string(&mut body)?;
                if !body.is_empty() {
                    return None;
                }
            }
            EXTENDED_EVENT_DESCRIPTOR_TAG if body.get(1..4) == Some(JAPANESE_LANGUAGE_CODE) => {
                let number = body[0] >> 4;
                let last = body[0] & 15;
                if number > last {
                    return None;
                }
                let length = usize::from(*body.get(4)?);
                let (mut items, mut tail) = body.get(5..)?.split_at_checked(length)?;
                let mut value = String::new();
                while !items.is_empty() {
                    let label = string(&mut items)?;
                    let item = string(&mut items)?;
                    if !label.is_empty() {
                        value.push_str(&label);
                        value.push('\n');
                    }
                    value.push_str(&item);
                    value.push('\n');
                }
                value.push_str(&string(&mut tail)?);
                if !tail.is_empty() || fragments.insert(number, (last, value)).is_some() {
                    return None;
                }
            }
            CONTENT_DESCRIPTOR_TAG => {
                let (genres, remainder) = body.as_chunks::<CONTENT_ENTRY_SIZE>();
                if !remainder.is_empty() {
                    return None;
                }
                program
                    .genres
                    .extend(genres.iter().map(|v| (v[0] >> 4, v[0] & 15)));
            }
            _ => {}
        }
    }
    if let Some((last, _)) = fragments.values().next()
        && fragments.len() == usize::from(*last) + 1
        && fragments.values().all(|(n, _)| n == last)
    {
        program.extended = fragments
            .values()
            .map(|(_, s)| s.as_str())
            .collect::<Vec<_>>()
            .join("\n");
    }
    event.program = Some(program);
    Some(event)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arib_dates_are_jst_and_undefined_is_not_epoch_zero() {
        assert_eq!(date(&[0x9e, 0x8b, 0x09, 0, 0]), Some(0));
        assert_eq!(date(&[255; 5]), None);
        assert_eq!(date(&[0x9e, 0x8b, 0x24, 0, 0]), None);
        assert_eq!(duration(&[0, 0x60, 0]), None);
    }
}
