//! Shared wire fixture for CPU decoder tests and the production-window tests.
const DATA_PID: u16 = 0x1f00;
const DOWNLOAD: u32 = 1;
// STD-B24 fascicle 3 §6.2.2: network-originated DII transactions use bits 31..30 = 10.
const TRANSACTION: u32 = 0x8000_0001;
// A full DDB plus its headers and CRC must fit the 4093-byte section-length limit.
const BLOCK_SIZE: u16 = 2048;
pub const SERVICE: u16 = 1;
pub const PMT_PID: u16 = 0x0020;

fn section(table: u8, service: u16, body: &[u8]) -> Vec<u8> {
    let length = body.len() + 9;
    let mut bytes = vec![table, 0xb0 | (length >> 8) as u8, length as u8];
    bytes.extend(service.to_be_bytes());
    bytes.extend([0xc1, 0, 0]);
    bytes.extend(body);
    let mut crc = u32::MAX;
    for byte in &bytes {
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
    bytes.extend(crc.to_be_bytes());
    bytes
}

fn packets(pid: u16, counter: &mut u8, section: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();
    let payload = [&[0], section].concat();
    for (index, chunk) in payload.chunks(184).enumerate() {
        let mut packet = [0xff; 188];
        packet[..4].copy_from_slice(&[
            0x47,
            (pid >> 8) as u8 | if index == 0 { 0x40 } else { 0 },
            pid as u8,
            0x10 | *counter,
        ]);
        *counter = (*counter + 1) & 15;
        packet[4..4 + chunk.len()].copy_from_slice(chunk);
        result.extend(packet);
    }
    result
}

fn component(automatic: bool) -> [u8; 16] {
    [
        0x0d,
        0xe0 | (DATA_PID >> 8) as u8,
        DATA_PID as u8,
        0xf0,
        11,
        0x52,
        1,
        0x40,
        0xfd,
        6,
        0,
        0x0c,
        if automatic { 0x33 } else { 0x23 },
        0x7f,
        0xf7, // Any data event; no transmitted DSM-CC event-message sections.
        0x3f, // No on-demand retrieval or file storage; reserved bits are set.
    ]
}

#[cfg(test)]
pub fn tables(automatic: Option<bool>, counter: u8) -> Vec<u8> {
    let mut bytes = packets(
        0,
        &mut { counter },
        &section(
            0,
            1,
            &[0, SERVICE as u8, 0xe0 | (PMT_PID >> 8) as u8, PMT_PID as u8],
        ),
    );
    let mut body = vec![0xe1, 0, 0xf0, 0];
    if let Some(automatic) = automatic {
        body.extend(component(automatic));
    }
    bytes.extend(packets(
        PMT_PID,
        &mut { counter },
        &section(2, SERVICE, &body),
    ));
    bytes
}

fn message(kind: u16, identifier: u32, body: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x11, 3];
    bytes.extend(kind.to_be_bytes());
    bytes.extend(identifier.to_be_bytes());
    bytes.extend([0xff, 0]);
    bytes.extend((body.len() as u16).to_be_bytes());
    bytes.extend(body);
    bytes
}

pub fn carousel(counter: &mut u8) -> Vec<u8> {
    let content = include_bytes!("overlay.bml");
    assert!(content.len() < usize::from(BLOCK_SIZE));
    let descriptors = b"\x01\x0ftext/x-arib-bml\x02\x0bstartup.bml";
    let mut info = Vec::new();
    info.extend(DOWNLOAD.to_be_bytes());
    info.extend(BLOCK_SIZE.to_be_bytes());
    info.extend([0; 10]);
    info.extend([0, 2, 0, 0, 0, 1]); // compatibility descriptor, one module
    info.extend(0_u16.to_be_bytes());
    info.extend((content.len() as u32).to_be_bytes());
    info.extend([0, descriptors.len() as u8]);
    info.extend(descriptors);
    info.extend([0, 0]);
    let mut bytes = packets(
        DATA_PID,
        counter,
        &section(
            0x3b,
            TRANSACTION as u16,
            &message(0x1002, TRANSACTION, &info),
        ),
    );
    let mut block = vec![0, 0, 0, 0xff, 0, 0]; // module 0, version 0, block 0
    block.extend(content);
    bytes.extend(packets(
        DATA_PID,
        counter,
        &section(0x3c, 0, &message(0x1003, DOWNLOAD, &block)),
    ));
    bytes
}

/// Preserve the media clock and elementary streams; add one BML component.
#[cfg(feature = "native_tests")]
pub fn with_overlay(ts: &[u8], automatic: bool) -> Vec<u8> {
    let mut result = Vec::new();
    let mut pmt = arib_b24::transport::SectionPackets::new(PMT_PID).unwrap();
    let (mut pmt_counter, mut data_counter) = (0, 0);
    for packet in ts.as_chunks::<188>().0 {
        let parsed = viewer_mpegts::TransportPacket::parse(packet).unwrap();
        if parsed.pid.0 != PMT_PID {
            result.extend(packet);
            continue;
        }
        for raw in pmt.push(packet).unwrap() {
            let mut body = raw[8..raw.len() - 4].to_vec();
            body.extend(component(automatic));
            result.extend(packets(
                PMT_PID,
                &mut pmt_counter,
                &section(2, SERVICE, &body),
            ));
            result.extend(carousel(&mut data_counter));
        }
    }
    result
}
