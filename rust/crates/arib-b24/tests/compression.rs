use arib_b24::{Descriptor, Module, ModuleInfo};
use flate2::{Compression, Crc, write::ZlibEncoder};
use std::io::Write;

fn compressed(payload: &[u8]) -> Module {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(payload).unwrap();
    let data = encoder.finish().unwrap();
    let mut descriptor = vec![0];
    descriptor.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    Module {
        download_id: 1,
        info: ModuleInfo {
            id: 1,
            size: data.len() as u32,
            version: 0,
            descriptors: vec![Descriptor {
                tag: 0xc2,
                data: descriptor,
            }],
        },
        data,
    }
}

#[test]
fn accepts_verified_crc32_isize_after_zlib() {
    // Authored data spanning multiple output chunks, without broadcast content.
    let payload: Vec<_> = (0..100_000).map(|i| (i % 251) as u8).collect();
    let mut module = compressed(&payload);
    let mut crc = Crc::new();
    crc.update(&payload);
    module.data.extend_from_slice(&crc.sum().to_le_bytes());
    module.data.extend_from_slice(&crc.amount().to_le_bytes());
    module.info.size = module.data.len() as u32;
    assert_eq!(module.clone().decode().unwrap().data, payload);
    for offset in [8, 4] {
        let mut corrupt = module.clone();
        let index = corrupt.data.len() - offset;
        corrupt.data[index] ^= 1;
        assert!(corrupt.decode().is_err());
    }
    module.data.push(0);
    module.info.size += 1;
    assert!(module.decode().is_err());
}

#[test]
fn rejects_incomplete_or_corrupt_zlib_checksum() {
    let module = compressed(b"<bml>checksum must be present</bml>");
    for length in 0..module.data.len() {
        let mut truncated = module.clone();
        truncated.data.truncate(length);
        truncated.info.size = length as u32;
        assert!(truncated.decode().is_err(), "accepted {length} bytes");
    }
    let mut corrupt = module;
    *corrupt.data.last_mut().unwrap() ^= 1;
    assert!(corrupt.decode().is_err());
}

#[test]
fn decodes_empty_stream_and_rejects_unrecognised_trailing_bytes() {
    let module = compressed(b"");
    assert!(module.clone().decode().unwrap().data.is_empty());
    let mut extra = module;
    extra.data.extend_from_slice(b"extra");
    extra.info.size = extra.data.len() as u32;
    assert!(extra.decode().is_err());
}
