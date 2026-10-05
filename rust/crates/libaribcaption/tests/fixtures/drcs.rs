//! Authored DRCS data, without received broadcast content.
#![allow(dead_code)] // Different test consumers use different packet builders.
pub fn caption_group(id: u8, body: &[u8]) -> Vec<u8> {
    let mut data = vec![0x80, 0xff, 0xf0, id << 2, 0, 0];
    data.extend((body.len() as u16).to_be_bytes());
    data.extend(body);
    let mut crc = 0_u16;
    for byte in &data[3..] {
        crc ^= u16::from(*byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    data.extend(crc.to_be_bytes());
    data
}

pub fn data_unit(parameter: u8, data: &[u8]) -> Vec<u8> {
    let mut unit = vec![0x1f, parameter];
    unit.extend(&(data.len() as u32).to_be_bytes()[1..]);
    unit.extend(data);
    unit
}

pub fn management(format: u8, units: &[u8]) -> Vec<u8> {
    let mut body = vec![0, 1, 0, b'j', b'p', b'n', format << 4];
    body.extend(&(units.len() as u32).to_be_bytes()[1..]);
    body.extend(units);
    caption_group(0, &body)
}

pub fn statement(text: &[u8]) -> Vec<u8> {
    let unit = data_unit(0x20, text);
    let mut body = vec![0];
    body.extend(&(unit.len() as u32).to_be_bytes()[1..]);
    body.extend(unit);
    caption_group(1, &body)
}

// 2x2 DRCS-1, code 0x21. Statement-local definitions permit redefinition
// without relying on management-version duplicate suppression.
pub fn bitmap_statement(pixels: u8, mixed: bool) -> Vec<u8> {
    let mut units = data_unit(0x30, &[1, 0x41, 0x21, 1, 0, 0, 2, 2, pixels]);
    let mut text = b"\x0c\x1b\x28\x20\x41\x21".to_vec();
    if mixed {
        text.extend(b"\x0eA");
    }
    units.extend(data_unit(0x20, &text));
    let mut body = vec![0];
    body.extend(&(units.len() as u32).to_be_bytes()[1..]);
    body.extend(units);
    caption_group(1, &body)
}
