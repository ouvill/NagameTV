use arib_b24::event::{EventSection, EventTime};
use arib_b24::transport::{
    BroadcastItem, DataReceiver, SectionPackets, ServiceReceiver, ServiceUpdate, TsReceiver,
    data_components_from_pmt, pmt_pid_from_pat,
};
use arib_b24::{
    Carousel, DecodeLimits, Descriptor, Error, Module, STANDARD_MAX_MODULE_BYTES, Section,
};
use flate2::{Compression, write::ZlibEncoder};
use std::io::Write;

fn section(table: u8, extension: u16, version: u8, number: u8, message: &[u8]) -> Vec<u8> {
    let length = 5 + message.len() + 4;
    let mut bytes = vec![table, 0xb0 | ((length >> 8) as u8 & 0x0f), length as u8];
    bytes.extend_from_slice(&extension.to_be_bytes());
    bytes.extend_from_slice(&[0xc1 | ((version & 0x1f) << 1), number, number]);
    bytes.extend_from_slice(message);
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
    bytes.extend_from_slice(&crc.to_be_bytes());
    bytes
}

fn message(kind: u16, identity: u32, body: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x11, 0x03];
    bytes.extend_from_slice(&kind.to_be_bytes());
    bytes.extend_from_slice(&identity.to_be_bytes());
    bytes.extend_from_slice(&[0xff, 0]);
    bytes.extend_from_slice(&(body.len() as u16).to_be_bytes());
    bytes.extend_from_slice(body);
    bytes
}

fn dii(transaction: u32, version: u8, size: u32) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&0x1234_5678_u32.to_be_bytes());
    body.extend_from_slice(&3_u16.to_be_bytes());
    body.extend_from_slice(&[0; 10]);
    body.extend_from_slice(&2_u16.to_be_bytes()); // Empty compatibility descriptor.
    body.extend_from_slice(&0_u16.to_be_bytes());
    body.extend_from_slice(&1_u16.to_be_bytes()); // One module.
    body.extend_from_slice(&7_u16.to_be_bytes());
    body.extend_from_slice(&size.to_be_bytes());
    body.push(version);
    let descriptors = [
        1, 8, b't', b'e', b'x', b't', b'/', b'b', b'm', b'l', 2, 5, b'a', b'.', b'b', b'm', b'l',
    ];
    body.push(descriptors.len() as u8);
    body.extend_from_slice(&descriptors);
    body.extend_from_slice(&0_u16.to_be_bytes()); // No private descriptors.
    section(
        0x3b,
        transaction as u16,
        0,
        0,
        &message(0x1002, transaction, &body),
    )
}

fn ddb(version: u8, number: u16, data: &[u8]) -> Vec<u8> {
    ddb_for(7, version, number, data)
}

fn ddb_for(id: u16, version: u8, number: u16, data: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&id.to_be_bytes());
    body.extend_from_slice(&[version, 0xff]);
    body.extend_from_slice(&number.to_be_bytes());
    body.extend_from_slice(data);
    section(
        0x3c,
        id,
        version,
        number as u8,
        &message(0x1003, 0x1234_5678, &body),
    )
}

fn linked_dii(transaction: u32) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&0x1234_5678_u32.to_be_bytes());
    body.extend_from_slice(&3_u16.to_be_bytes());
    body.extend_from_slice(&[0; 10]);
    body.extend_from_slice(&2_u16.to_be_bytes());
    body.extend_from_slice(&0_u16.to_be_bytes());
    body.extend_from_slice(&3_u16.to_be_bytes());
    for (id, position, next, descriptors) in [
        (
            7_u16,
            0_u8,
            8_u16,
            b"\x01\x08text/bml\x02\x0alinked.bml".as_slice(),
        ),
        (8, 1, 9, b"".as_slice()),
        (9, 2, 0, b"".as_slice()),
    ] {
        body.extend_from_slice(&id.to_be_bytes());
        body.extend_from_slice(&3_u32.to_be_bytes());
        body.push(1);
        body.push((descriptors.len() + 5) as u8);
        body.extend_from_slice(descriptors);
        body.extend_from_slice(&[0x04, 3, position]);
        body.extend_from_slice(&next.to_be_bytes());
    }
    body.extend_from_slice(&0_u16.to_be_bytes());
    section(
        0x3b,
        transaction as u16,
        0,
        0,
        &message(0x1002, transaction, &body),
    )
}

#[test]
fn assembles_out_of_order_and_ignores_repeated_carousel_blocks() -> Result<(), Error> {
    let Section::Info(info) = Section::parse(&dii(0x8000_0001, 9, 8))? else {
        panic!("DII")
    };
    assert_eq!(
        info.modules[0].descriptors[0].media_type(),
        Some(b"text/bml".as_slice())
    );
    assert_eq!(
        info.modules[0].descriptors[1].name(),
        Some(b"a.bml".as_slice())
    );
    let mut carousel = Carousel::new(info)?;
    let Section::Block(last) = Section::parse(&ddb(9, 2, b"hi"))? else {
        panic!("DDB")
    };
    assert!(carousel.push(last.clone())?.is_none());
    assert!(carousel.push(last)?.is_none());
    let Section::Block(first) = Section::parse(&ddb(9, 0, b"abc"))? else {
        panic!("DDB")
    };
    assert!(carousel.push(first)?.is_none());
    let Section::Block(middle) = Section::parse(&ddb(9, 1, b"def"))? else {
        panic!("DDB")
    };
    let module = carousel.push(middle)?.expect("completed module");
    assert_eq!(module.data, b"abcdefhi");
    assert_eq!(module.info.size, 8);
    Ok(())
}

#[test]
fn module_over_sixteen_mib_uses_configured_memory_limit() -> Result<(), Error> {
    let size = 17 * 1024 * 1024;
    let Section::Info(mut info) = Section::parse(&dii(1, 1, size as u32))? else {
        panic!("DII")
    };
    info.block_size = 4096;
    Carousel::new(info.clone())?;
    let small = DecodeLimits::new(16 * 1024 * 1024, 16 * 1024 * 1024)?;
    assert!(matches!(
        Carousel::with_limits(info.clone(), small),
        Err(Error::ModuleTooLarge { actual, limit }) if actual == size && limit == 16 * 1024 * 1024
    ));
    let module = Module {
        download_id: info.download_id,
        info: info.modules[0].clone(),
        data: vec![0; size],
    };
    assert_eq!(module.decode()?.data.len(), size);
    let Section::Info(oversize) =
        Section::parse(&dii(1, 1, (STANDARD_MAX_MODULE_BYTES + 1) as u32))?
    else {
        panic!("DII")
    };
    assert!(matches!(
        Carousel::new(oversize),
        Err(Error::Invalid("module exceeds standard maximum"))
    ));
    Ok(())
}

#[test]
fn rejects_corruption_and_wrong_lengths() -> Result<(), Error> {
    let mut invalid = dii(1, 1, 8);
    invalid[15] ^= 1;
    assert!(matches!(
        Section::parse(&invalid),
        Err(Error::Invalid("section CRC-32"))
    ));
    let valid = ddb(1, 0, b"abc");
    assert!(Section::parse(&valid[..valid.len() - 1]).is_err());
    let Section::Info(info) = Section::parse(&dii(1, 1, 8))? else {
        panic!("DII")
    };
    let mut carousel = Carousel::new(info)?;
    let Section::Block(short) = Section::parse(&ddb(1, 0, b"ab"))? else {
        panic!("DDB")
    };
    assert!(matches!(
        carousel.push(short),
        Err(Error::Invalid("block size"))
    ));
    let Section::Block(old) = Section::parse(&ddb(0, 0, b"abc"))? else {
        panic!("DDB")
    };
    assert!(carousel.push(old)?.is_none());
    let Section::Block(first) = Section::parse(&ddb(1, 0, b"abc"))? else {
        panic!("DDB")
    };
    assert!(carousel.push(first)?.is_none());
    let Section::Block(conflict) = Section::parse(&ddb(1, 0, b"xyz"))? else {
        panic!("DDB")
    };
    assert!(matches!(
        carousel.push(conflict),
        Err(Error::Invalid("conflicting block"))
    ));
    Ok(())
}

#[test]
fn a_bad_module_crc_can_be_recovered_on_the_next_carousel_cycle() -> Result<(), Error> {
    let Section::Info(mut info) = Section::parse(&dii(2, 1, 3))? else {
        panic!("DII")
    };
    let mut crc = u32::MAX;
    for byte in b"abc" {
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
    info.modules[0].descriptors.push(Descriptor {
        tag: 0x05,
        data: crc.to_be_bytes().to_vec(),
    });
    let mut carousel = Carousel::new(info)?;
    let Section::Block(bad) = Section::parse(&ddb(1, 0, b"xyz"))? else {
        panic!("DDB")
    };
    assert!(matches!(
        carousel.push(bad),
        Err(Error::Invalid("module CRC-32"))
    ));
    let Section::Block(good) = Section::parse(&ddb(1, 0, b"abc"))? else {
        panic!("DDB")
    };
    assert_eq!(carousel.push(good)?.expect("recovered").data, b"abc");
    Ok(())
}

fn pmt() -> Vec<u8> {
    let body = [
        0xe1, 0x00, // PCR PID
        0xf0, 0x00, // Program descriptors
        0x0b, 0xe1, 0x01, 0xf0, 0x07, // DSM-CC type B, PID 0x101, descriptor length
        0x52, 0x01, 0x40, // Entry component tag
        0xfd, 0x02, 0x00, 0x0c, // Terrestrial BML data component
    ];
    section(0x02, 42, 0, 0, &body)
}

#[test]
fn pmt_keeps_bxml_entry_information_for_browser_startup() -> Result<(), Error> {
    let body = [
        0xe1, 0x00, 0xf0, 0x00, 0x0d, 0xe1, 0x01, 0xf0, 0x0b, 0x52, 0x01, 0x40, 0xfd, 0x06, 0x00,
        0x0c, 0x33, 0x70, 0xf8, 0xa0,
    ];
    let components = data_components_from_pmt(&section(0x02, 42, 0, 0, &body), 42)?;
    assert_eq!(components.len(), 1);
    let component = components[0];
    assert_eq!(component.stream_type(), 0x0d);
    let info = component.bxml_info().expect("BXML info");
    assert_eq!(info.transmission_format, 0);
    let entry = info.entry_point.expect("entry point");
    assert!(entry.auto_start);
    assert_eq!(entry.document_resolution, 3);
    assert_eq!((entry.bml_major_version, entry.bml_minor_version), (1, 0));
    assert_eq!(info.carousel.expect("carousel").data_event_id, 15);
    Ok(())
}

#[test]
fn pmt_parses_bml_versions_from_broadcast_length_descriptor() -> Result<(), Error> {
    let body = [
        0xe1, 0x00, 0xf0, 0x00, 0x0d, 0xe1, 0x01, 0xf0, 0x0f, 0x52, 0x01, 0x40, 0xfd, 0x0a, 0x00,
        0x0c, 0x33, 0x3f, 0x00, 0x03, 0x00, 0x00, 0xff, 0xbf,
    ];
    let components = data_components_from_pmt(&section(0x02, 42, 0, 0, &body), 42)?;
    let info = components[0].bxml_info().expect("BML info");
    let entry = info.entry_point.expect("entry point");
    assert_eq!((entry.bml_major_version, entry.bml_minor_version), (3, 0));
    let carousel = info.carousel.expect("carousel");
    assert_eq!(carousel.data_event_id, 15);
    assert!(carousel.event_sections);
    Ok(())
}

fn pat() -> Vec<u8> {
    section(0x00, 1, 0, 0, &[0x00, 0x2a, 0xe1, 0x00])
}

#[test]
fn program_identity_follows_selected_pat_and_pmt() -> Result<(), Error> {
    let mut receiver = ServiceReceiver::new(42)?;
    receiver.push_updates(&packet(0, 0, true, &[&[0], pat().as_slice()].concat()))?;
    assert_eq!(receiver.program(), None);
    receiver.push_updates(&packet(0x100, 0, true, &[&[0], pmt().as_slice()].concat()))?;
    let program = receiver.program().unwrap();
    assert_eq!(program.transport_stream_id(), 1);
    assert_eq!(program.service_id(), 42);
    assert_eq!(program.pcr_pid(), 0x100);

    // Multiple services may share a PMT PID. Another service must not clear ours.
    let other = section(2, 43, 0, 0, &[0xe1, 0x10, 0xf0, 0]);
    receiver.push_updates(&packet(0x100, 1, true, &[&[0], other.as_slice()].concat()))?;
    assert_eq!(receiver.program(), Some(program));
    assert_eq!(receiver.components().count(), 1);

    // The same PMT PID in a different transport is a new association.
    let changed = section(0, 2, 0, 0, &[0, 42, 0xe1, 0]);
    receiver.push_updates(&packet(0, 1, true, &[&[0], changed.as_slice()].concat()))?;
    assert_eq!(receiver.program(), None);
    assert_eq!(receiver.components().count(), 0);
    receiver.push_updates(&packet(0x100, 2, true, &[&[0], pmt().as_slice()].concat()))?;
    assert_eq!(receiver.program().unwrap().transport_stream_id(), 2);
    Ok(())
}

#[test]
fn bxml_entry_control_rejects_malformed_descriptors() -> Result<(), Error> {
    let Section::Info(mut info) = Section::parse(&dii(1, 0, 3))? else {
        panic!("DII expected");
    };
    for descriptors in [
        vec![Descriptor {
            tag: 0xf0,
            data: vec![],
        }],
        vec![Descriptor {
            tag: 0xf0,
            data: vec![0, 0],
        }],
        vec![
            Descriptor {
                tag: 0xf0,
                data: vec![0]
            };
            2
        ],
    ] {
        info.descriptors = descriptors;
        assert!(matches!(
            info.bxml_return_to_entry(),
            Err(Error::Invalid("BXML private data descriptor"))
        ));
    }
    Ok(())
}

fn packet(pid: u16, counter: u8, start: bool, payload: &[u8]) -> [u8; 188] {
    assert!(payload.len() <= 184);
    let mut bytes = [0xff; 188];
    bytes[..4].copy_from_slice(&[
        0x47,
        ((pid >> 8) as u8 & 0x1f) | if start { 0x40 } else { 0 },
        pid as u8,
        0x30 | counter,
    ]);
    let adaptation_length = 183 - payload.len();
    bytes[4] = adaptation_length as u8;
    if adaptation_length > 0 {
        bytes[5] = 0;
    }
    bytes[188 - payload.len()..].copy_from_slice(payload);
    bytes
}

#[test]
fn discovers_entry_pid_and_receives_split_sections() -> Result<(), Error> {
    let mut pat_packets = SectionPackets::new(0)?;
    let mut pat_payload = vec![0];
    pat_payload.extend_from_slice(&pat());
    let pat_sections = pat_packets.push(&packet(0, 0, true, &pat_payload))?;
    assert_eq!(pmt_pid_from_pat(&pat_sections[0], 42)?, Some(0x100));
    assert_eq!(pmt_pid_from_pat(&pat(), 43)?, None);
    let components = data_components_from_pmt(&pmt(), 42)?;
    assert_eq!(components.len(), 1);
    assert!(components[0].is_entry());
    assert_eq!(components[0].pid(), 0x101);
    let mut type_d = pmt();
    type_d[12] = 0x0d;
    let repaired = section(0x02, 42, 0, 0, &type_d[8..type_d.len() - 4]);
    assert_eq!(data_components_from_pmt(&repaired, 42)?.len(), 1);
    assert!(data_components_from_pmt(&pmt(), 43)?.is_empty());
    let mut corrupt = pmt();
    corrupt[12] ^= 1;
    assert!(matches!(
        data_components_from_pmt(&corrupt, 42),
        Err(Error::Invalid("PMT CRC-32"))
    ));

    let mut receiver = DataReceiver::new(components[0]);
    let dii = dii(1, 2, 6);
    let mut first = vec![0];
    first.extend_from_slice(&dii[..12]);
    assert!(receiver.push(&packet(0x101, 0, true, &first))?.is_empty());
    assert!(
        receiver
            .push(&packet(0x101, 1, false, &dii[12..]))?
            .is_empty()
    );
    let mut block_start = vec![0];
    block_start.extend_from_slice(&ddb(2, 1, b"def"));
    assert!(
        receiver
            .push(&packet(0x101, 2, true, &block_start))?
            .is_empty()
    );
    let mut block_start = vec![0];
    block_start.extend_from_slice(&ddb(2, 0, b"abc"));
    let modules = receiver.push(&packet(0x101, 3, true, &block_start))?;
    assert_eq!(modules.len(), 1);
    assert_eq!(modules[0].data, b"abcdef");
    Ok(())
}

#[test]
fn service_receiver_follows_pat_pmt_to_named_resource() -> Result<(), Error> {
    let mut receiver = ServiceReceiver::new(42)?;
    for (pid, section) in [(0, pat()), (0x100, pmt()), (0x101, dii(1, 2, 6))] {
        let mut payload = vec![0];
        payload.extend_from_slice(&section);
        assert!(receiver.push(&packet(pid, 0, true, &payload))?.is_empty());
    }
    assert_eq!(receiver.pmt_pid(), Some(0x100));
    assert_eq!(receiver.components().count(), 1);
    let mut first = vec![0];
    first.extend_from_slice(&ddb(2, 0, b"abc"));
    assert!(receiver.push(&packet(0x101, 1, true, &first))?.is_empty());
    let mut second = vec![0];
    second.extend_from_slice(&ddb(2, 1, b"def"));
    let resources = receiver.push(&packet(0x101, 2, true, &second))?;
    assert_eq!(resources.len(), 1);
    assert_eq!(resources[0].name, b"a.bml");
    assert_eq!(resources[0].component_tag, Some(0x40));
    assert_eq!(resources[0].data, b"abcdef");
    let descriptor = [0x40, 12, 0x23, 0x4f, 0, 0, 0, 0, 0, 0, 1, 0x56, 0x78, 0xaa];
    let event = section(0x3d, 0x1234, 0, 0, &descriptor);
    let mut payload = vec![0];
    payload.extend_from_slice(&event);
    let items = receiver.push_items(&packet(0x101, 3, true, &payload))?;
    assert!(
        matches!(items.as_slice(), [BroadcastItem::Event(event)] if event.messages[0].message_id == 0x5678)
    );
    Ok(())
}

#[test]
fn service_receiver_assembles_linked_modules_in_chain_order() -> Result<(), Error> {
    let mut receiver = ServiceReceiver::new(42)?;
    for (pid, section) in [(0, pat()), (0x100, pmt()), (0x101, linked_dii(1))] {
        let mut payload = vec![0];
        payload.extend_from_slice(&section);
        assert!(receiver.push(&packet(pid, 0, true, &payload))?.is_empty());
    }
    for (counter, id, data) in [(1, 9, b"ghi"), (2, 8, b"def")] {
        let mut payload = vec![0];
        payload.extend_from_slice(&ddb_for(id, 1, 0, data));
        assert!(
            receiver
                .push(&packet(0x101, counter, true, &payload))?
                .is_empty()
        );
    }
    let mut payload = vec![0];
    payload.extend_from_slice(&ddb_for(7, 1, 0, b"abc"));
    let resources = receiver.push(&packet(0x101, 3, true, &payload))?;
    assert_eq!(resources.len(), 1);
    assert_eq!(resources[0].name, b"linked.bml");
    assert_eq!(
        resources[0].media_type.as_deref(),
        Some(b"text/bml".as_slice())
    );
    assert_eq!(resources[0].module_id, 7);
    assert_eq!(resources[0].data, b"abcdefghi");
    Ok(())
}

#[test]
fn service_receiver_reports_browser_state_before_resources() -> Result<(), Error> {
    let mut receiver = ServiceReceiver::new(42)?;
    let mut payload = vec![0];
    payload.extend_from_slice(&pat());
    assert!(matches!(
        receiver.push_updates(&packet(0, 0, true, &payload))?.as_slice(),
        [ServiceUpdate::Components(components)] if components.is_empty()
    ));
    let mut payload = vec![0];
    payload.extend_from_slice(&pmt());
    assert!(matches!(
        receiver.push_updates(&packet(0x100, 0, true, &payload))?.as_slice(),
        [ServiceUpdate::Components(components)] if components.len() == 1 && components[0].tag() == 0x40
    ));
    let mut payload = vec![0];
    payload.extend_from_slice(&dii(1, 2, 3));
    assert!(matches!(
        receiver.push_updates(&packet(0x101, 0, true, &payload))?.as_slice(),
        [ServiceUpdate::ModuleList { component, info }]
            if component.tag() == 0x40 && info.modules.len() == 1 && info.modules[0].id == 7
    ));
    let mut payload = vec![0];
    payload.extend_from_slice(&dii(1, 2, 3));
    assert!(
        receiver
            .push_updates(&packet(0x101, 1, true, &payload))?
            .is_empty()
    );
    let mut payload = vec![0];
    payload.extend_from_slice(&ddb(2, 0, b"abc"));
    assert!(matches!(
        receiver.push_updates(&packet(0x101, 2, true, &payload))?.as_slice(),
        [ServiceUpdate::Module { resources, .. }] if resources.len() == 1 && resources[0].data == b"abc" && resources[0].component_tag == Some(0x40)
    ));
    Ok(())
}

#[test]
fn linked_file_obeys_aggregate_limit() -> Result<(), Error> {
    let limits = DecodeLimits::new(3, 3)?.with_linked_bytes(8)?;
    let mut receiver = DataReceiver::with_limits(data_components_from_pmt(&pmt(), 42)?[0], limits);
    let mut payload = vec![0];
    payload.extend_from_slice(&linked_dii(1));
    assert!(receiver.push(&packet(0x101, 0, true, &payload))?.is_empty());
    for (counter, id, data) in [(1, 7, b"abc"), (2, 8, b"def")] {
        let mut payload = vec![0];
        payload.extend_from_slice(&ddb_for(id, 1, 0, data));
        assert!(
            receiver
                .push(&packet(0x101, counter, true, &payload))?
                .is_empty()
        );
    }
    let mut payload = vec![0];
    payload.extend_from_slice(&ddb_for(9, 1, 0, b"ghi"));
    assert!(matches!(
        receiver.push(&packet(0x101, 3, true, &payload)),
        Err(Error::LinkedFileTooLarge {
            actual: 9,
            limit: 8
        })
    ));
    Ok(())
}

#[test]
fn new_dii_generation_discards_old_linked_fragments() -> Result<(), Error> {
    let mut receiver = DataReceiver::new(data_components_from_pmt(&pmt(), 42)?[0]);
    for (counter, section) in [(0, linked_dii(1)), (1, ddb_for(7, 1, 0, b"old"))] {
        let mut payload = vec![0];
        payload.extend_from_slice(&section);
        assert!(
            receiver
                .push(&packet(0x101, counter, true, &payload))?
                .is_empty()
        );
    }
    let mut payload = vec![0];
    payload.extend_from_slice(&linked_dii(2));
    assert!(receiver.push(&packet(0x101, 2, true, &payload))?.is_empty());
    for (counter, id, data) in [(3, 8, b"def"), (4, 9, b"ghi")] {
        let mut payload = vec![0];
        payload.extend_from_slice(&ddb_for(id, 1, 0, data));
        assert!(
            receiver
                .push(&packet(0x101, counter, true, &payload))?
                .is_empty()
        );
    }
    let mut payload = vec![0];
    payload.extend_from_slice(&ddb_for(7, 1, 0, b"abc"));
    let modules = receiver.push(&packet(0x101, 5, true, &payload))?;
    assert_eq!(modules.len(), 1);
    assert_eq!(modules[0].data, b"abcdefghi");
    Ok(())
}

#[test]
fn rejects_invalid_module_link_descriptors() -> Result<(), Error> {
    let Section::Info(mut info) = Section::parse(&linked_dii(1))? else {
        panic!("DII")
    };
    info.modules[0].descriptors.last_mut().expect("link").data[0] = 3;
    assert!(matches!(
        Carousel::new(info),
        Err(Error::Invalid("module link position or cycle"))
    ));
    let Section::Info(mut info) = Section::parse(&linked_dii(1))? else {
        panic!("DII")
    };
    info.modules[0].descriptors.last_mut().expect("link").data[2] = 7;
    assert!(matches!(
        Carousel::new(info),
        Err(Error::Invalid("module link position or cycle"))
    ));
    Ok(())
}

#[test]
fn service_receiver_passes_memory_limits_to_carousel() -> Result<(), Error> {
    let limits = DecodeLimits::new(5, 5)?;
    let mut receiver = ServiceReceiver::with_limits(42, limits)?;
    for (pid, section) in [(0, pat()), (0x100, pmt())] {
        let mut payload = vec![0];
        payload.extend_from_slice(&section);
        receiver.push(&packet(pid, 0, true, &payload))?;
    }
    let mut payload = vec![0];
    payload.extend_from_slice(&dii(1, 2, 6));
    assert!(matches!(
        receiver.push(&packet(0x101, 0, true, &payload)),
        Err(Error::ModuleTooLarge {
            actual: 6,
            limit: 5
        })
    ));
    Ok(())
}

#[test]
fn continuity_loss_discards_partial_section() -> Result<(), Error> {
    let mut receiver = TsReceiver::new(0x101)?;
    let section = dii(1, 2, 6);
    let mut first = vec![0];
    first.extend_from_slice(&section[..12]);
    assert!(receiver.push(&packet(0x101, 0, true, &first))?.is_empty());
    assert!(matches!(
        receiver.push(&packet(0x101, 2, false, &section[12..])),
        Err(Error::Invalid("TS continuity"))
    ));
    let mut restart = vec![0];
    restart.extend_from_slice(&section);
    assert_eq!(receiver.push(&packet(0x101, 3, true, &restart))?.len(), 1);
    assert!(receiver.push(&packet(0x101, 3, true, &restart))?.is_empty());
    Ok(())
}

#[test]
fn pointer_prefix_finishes_one_section_and_starts_the_next() -> Result<(), Error> {
    let mut receiver = TsReceiver::new(0x101)?;
    let info = dii(1, 2, 3);
    let mut first = vec![0];
    first.extend_from_slice(&info[..12]);
    assert!(receiver.push(&packet(0x101, 0, true, &first))?.is_empty());
    let remaining = &info[12..];
    let mut second = vec![remaining.len() as u8];
    second.extend_from_slice(remaining);
    second.extend_from_slice(&ddb(2, 0, b"abc"));
    let sections = receiver.push(&packet(0x101, 1, true, &second))?;
    assert!(matches!(
        sections.as_slice(),
        [Section::Info(_), Section::Block(_)]
    ));
    Ok(())
}

#[test]
fn zlib_modules_expand_to_advertised_size() -> Result<(), Error> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(b"<bml>hello</bml>")
        .map_err(Error::Decompression)?;
    let compressed = encoder.finish().map_err(Error::Decompression)?;
    let Section::Info(mut info) = Section::parse(&dii(1, 1, compressed.len() as u32))? else {
        panic!("DII")
    };
    let mut descriptor = vec![0];
    descriptor.extend_from_slice(&16_u32.to_be_bytes());
    info.modules[0].descriptors.push(Descriptor {
        tag: 0xc2,
        data: descriptor,
    });
    let module = Module {
        download_id: info.download_id,
        info: info.modules.remove(0),
        data: compressed,
    };
    assert_eq!(module.clone().decode()?.data, b"<bml>hello</bml>");
    let limits = DecodeLimits::new(STANDARD_MAX_MODULE_BYTES, 15)?;
    assert!(matches!(
        module.clone().decode_with_limits(limits),
        Err(Error::DecodedTooLarge {
            actual: 16,
            limit: 15
        })
    ));
    let mut wrong = module;
    wrong.info.descriptors.last_mut().expect("compression").data[4] = 17;
    assert!(matches!(
        wrong.decode(),
        Err(Error::Invalid("decompressed module size"))
    ));
    Ok(())
}

#[test]
fn same_generation_dii_fragments_add_modules_without_losing_blocks() -> Result<(), Error> {
    let Section::Info(first) = Section::parse(&dii(3, 1, 3))? else {
        panic!("DII")
    };
    let mut next = first.clone();
    next.modules[0].id = 8;
    let mut carousel = Carousel::new(first)?;
    let Section::Block(first_block) = Section::parse(&ddb(1, 0, b"abc"))? else {
        panic!("DDB")
    };
    let completed = carousel.push(first_block)?.expect("first module");
    assert_eq!(completed.data, b"abc");
    carousel.merge_info(next.clone())?;
    assert_eq!(carousel.info().modules.len(), 2);
    carousel.merge_info(next)?;
    assert_eq!(carousel.info().modules.len(), 2);
    Ok(())
}

#[test]
fn repeated_dii_and_failed_merges_preserve_partial_and_completed_modules() -> Result<(), Error> {
    let Section::Info(info) = Section::parse(&dii(3, 1, 6))? else {
        panic!("DII")
    };
    let Section::Block(first) = Section::parse(&ddb(1, 0, b"abc"))? else {
        panic!("DDB")
    };
    let Section::Block(last) = Section::parse(&ddb(1, 1, b"def"))? else {
        panic!("DDB")
    };
    let mut carousel = Carousel::new(info.clone())?;
    assert!(carousel.push(first.clone())?.is_none());
    carousel.merge_info(info.clone())?;
    let mut additional = info.clone();
    additional.modules[0].id = 8;
    carousel.merge_info(additional.clone())?;
    carousel.merge_info(info.clone())?; // Repeated fragment of a merged DII.
    carousel.merge_info(additional)?;

    let accepted = carousel.info().clone();
    let mut conflict = info.clone();
    conflict.modules[0].size += 1;
    assert!(carousel.merge_info(conflict).is_err());
    let mut duplicate = info.clone();
    duplicate.modules.push(duplicate.modules[0].clone());
    assert!(carousel.merge_info(duplicate).is_err());
    let mut invalid_new = info.clone();
    invalid_new.modules[0].id = 9;
    invalid_new.modules[0].size = 0;
    assert!(carousel.merge_info(invalid_new).is_err());
    assert_eq!(carousel.info(), &accepted);

    assert_eq!(carousel.push(last.clone())?.unwrap().data, b"abcdef");
    carousel.merge_info(info)?;
    assert!(carousel.push(first)?.is_none());
    assert!(carousel.push(last)?.is_none());
    Ok(())
}

#[test]
fn parses_event_message_section_and_preserves_private_payload() -> Result<(), Error> {
    let descriptor = [
        0x40, 12, // General event descriptor and size
        0x23, 0x4f, // Group 0x234 and reserved bits
        0,    // Immediate
        0, 0, 0, 0, 0, // No time
        1, 0x56, 0x78, // Message type and ID
        0xaa, // BML-specific payload
    ];
    let raw = section(0x3d, 0x1234, 0, 0, &descriptor);
    let event = EventSection::parse(&raw)?;
    assert_eq!(event.data_event_id, 1);
    assert_eq!(event.group_id, 0x234);
    assert_eq!(event.messages.len(), 1);
    assert_eq!(event.messages[0].time, EventTime::Immediate);
    assert_eq!(event.messages[0].message_id, 0x5678);
    assert_eq!(event.messages[0].private_data, [0xaa]);
    Ok(())
}
