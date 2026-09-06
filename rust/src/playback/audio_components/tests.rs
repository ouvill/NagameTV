use super::*;
use gst::prelude::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;
fn pmt(service: u16, streams: &[(u16, u8)]) -> Vec<u8> {
    let mut bytes = vec![
        2,
        0xb0,
        0,
        (service >> 8) as u8,
        service as u8,
        0xc1,
        0,
        0,
        0xe1,
        0,
        0xf0,
        0,
    ];
    for (pid, tag) in streams {
        bytes.extend_from_slice(&[
            0x0f,
            0xe0 | (pid >> 8) as u8,
            *pid as u8,
            0xf0,
            3,
            0x52,
            1,
            *tag,
        ]);
    }
    let len = bytes.len() + 1;
    bytes[1] |= (len >> 8) as u8;
    bytes[2] = len as u8;
    let checksum = crc(&bytes);
    bytes.extend_from_slice(&checksum.to_be_bytes());
    bytes
}
fn checksum(bytes: &mut [u8]) {
    let end = bytes.len() - 4;
    let value = crc(&bytes[..end]);
    bytes[end..].copy_from_slice(&value.to_be_bytes());
}

#[test]
fn native_section_message_uses_explicit_service_and_replaces_components() -> TestResult {
    gst::init()?;
    let element = gst::ElementFactory::make("identity").build()?;
    let mut components = Components::for_service(Some(10));
    let raw = pmt(10, &[(0x202, 17), (0x201, 16)]);
    let section = gstreamer_mpegts::Section::new(0x100, &raw).ok_or("invalid fixture section")?;
    let message = gstreamer_mpegts::message_new_mpegts_section(element.upcast_ref(), &section);
    components.observe(&message)?;
    assert_eq!(components.tag_for_stream("source/00000201"), Some(16));
    assert_eq!(components.tag_for_stream("source/00000202"), Some(17));
    assert_eq!(components.tag_for_stream("source/201"), None);
    assert_eq!(components.tag_for_stream("source/0000+201"), None);
    components.update(&pmt(11, &[(0x201, 90)]))?;
    assert_eq!(components.tag_for_stream("source/00000201"), Some(16));
    let mut next = pmt(10, &[(0x201, 90)]);
    next[5] &= !1;
    checksum(&mut next);
    components.update(&next)?;
    assert_eq!(components.tag_for_stream("source/00000201"), Some(16));
    components.update(&pmt(10, &[(0x203, 18)]))?;
    assert_eq!(components.tag_for_stream("source/00000201"), None);
    assert_eq!(components.tag_for_stream("source/00000203"), Some(18));
    components = Components::for_service(None);
    components.observe(&message)?;
    assert_eq!(components.tag_for_stream("source/00000201"), None);
    Ok(())
}

#[test]
fn malformed_and_ambiguous_updates_cannot_leave_trusted_metadata() -> TestResult {
    assert_eq!(crc(b"123456789"), 0x0376_e6e7);
    let valid = pmt(10, &[(0x201, 16)]);
    for len in 0..valid.len() {
        assert!(parse(&valid[..len]).is_err());
    }
    let mut invalid_crc = valid.clone();
    invalid_crc[12] ^= 1;
    assert!(matches!(parse(&invalid_crc), Err(Error::Checksum)));
    let mut components = Components::for_service(Some(10));
    components.update(&valid)?;
    assert!(components.update(&invalid_crc).is_err());
    assert_eq!(components.tag_for_stream("source/00000201"), None);
    assert!(matches!(
        parse(&pmt(10, &[(0x201, 16), (0x201, 17)])),
        Err(Error::Ambiguous)
    ));
    assert!(matches!(
        parse(&pmt(10, &[(0x201, 16), (0x202, 16)])),
        Err(Error::Ambiguous)
    ));
    let mut malformed = valid.clone();
    malformed[18] = 2; // Descriptor extends beyond the declared ES loop.
    checksum(&mut malformed);
    assert!(matches!(parse(&malformed), Err(Error::Structure)));
    let mut multi = valid.clone();
    multi[7] = 1;
    checksum(&mut multi);
    assert!(matches!(parse(&multi), Err(Error::MultipleSections)));
    components.update(&pmt(10, &[]))?;
    assert!(components.entries.is_empty());
    Ok(())
}
