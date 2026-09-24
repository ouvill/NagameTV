use super::*;
use proptest::prelude::*;

const CRC_BYTES: usize = 4;
const MAX_PAYLOAD_BYTES: usize = wire::TS_PACKET_SIZE - wire::TS_HEADER_SIZE;
const MIN_PSI_SECTION_LENGTH: usize = 9;

fn section(service: u16) -> Vec<u8> {
    let mut data = vec![wire::PAT_TABLE_ID, 0xb0, 13, 0, 1, 0xc1, 0, 0];
    data.extend_from_slice(&service.to_be_bytes());
    data.extend_from_slice(&[0xe1, 0]);
    data.extend_from_slice(&wire::crc32_mpeg(&data).to_be_bytes());
    data
}

fn start(data: &[u8]) -> Vec<u8> {
    let mut payload = vec![0];
    payload.extend_from_slice(data);
    payload
}

#[test]
fn continuation_cannot_start_an_unannounced_section() {
    let first = section(1);
    let unannounced = section(2);
    for split in 1..first.len() {
        let mut sections = Sections::default();
        assert!(sections.push(true, &start(&first[..split])).is_empty());
        let mut continuation = first[split..].to_vec();
        continuation.extend_from_slice(&unannounced);
        assert_eq!(
            sections.push(false, &continuation),
            std::slice::from_ref(&first)
        );
        assert!(sections.is_empty());
        assert!(sections.push(false, &unannounced).is_empty());
        assert_eq!(
            sections.push(true, &start(&unannounced)),
            std::slice::from_ref(&unannounced)
        );
    }
}

#[test]
fn pointer_prefix_only_finishes_the_pending_section() {
    let first = section(1);
    let unannounced = section(2);
    let next = section(3);
    for split in 1..first.len() {
        let mut sections = Sections::default();
        assert!(sections.push(true, &start(&first[..split])).is_empty());
        let mut payload = vec![(first.len() - split + unannounced.len()) as u8];
        payload.extend_from_slice(&first[split..]);
        payload.extend_from_slice(&unannounced);
        payload.extend_from_slice(&next);
        assert_eq!(sections.push(true, &payload), [first.clone(), next.clone()]);
        assert!(sections.is_empty());
    }
}

#[test]
fn new_start_discards_incomplete_and_invalid_sections() {
    let first = section(1);
    let next = section(2);
    for pointer in [0, 1, u8::MAX] {
        let mut sections = Sections::default();
        assert!(sections.push(true, &start(&first[..1])).is_empty());
        // The old prefix is still incomplete, or the pointer is out of range.
        assert!(sections.push(true, &[pointer, 0xb0]).is_empty());
        assert_eq!(
            sections.push(true, &start(&next)),
            std::slice::from_ref(&next)
        );
        assert!(sections.is_empty());
    }
    let mut sections = Sections::default();
    assert!(sections.push(true, &[0, 0, 0, 0]).is_empty());
    assert!(sections.push(false, &next).is_empty());
    assert_eq!(sections.push(true, &start(&next)), [next]);
}

#[test]
fn one_start_can_carry_multiple_sections_and_a_split_header() {
    let first = section(1);
    let next = section(2);
    for split in 1..next.len() {
        let mut sections = Sections::default();
        let mut payload = start(&first);
        payload.extend_from_slice(&next[..split]);
        assert_eq!(sections.push(true, &payload), std::slice::from_ref(&first));
        assert_eq!(
            sections.push(false, &next[split..]),
            std::slice::from_ref(&next)
        );
        assert!(sections.is_empty());
    }
    let mut sections = Sections::default();
    let mut payload = start(&first);
    payload.extend_from_slice(&next);
    payload.push(wire::STUFFING_BYTE);
    assert_eq!(sections.push(true, &payload), [first, next]);
    assert!(sections.is_empty());
}

proptest! {
    #[test]
    fn split_sections_preserve_bytes_and_ignore_trailing_stuffing(
        body in prop::collection::vec(any::<u8>(), MIN_PSI_SECTION_LENGTH..=wire::MAX_PSI_SECTION_LENGTH),
        chunk_size in 1usize..=MAX_PAYLOAD_BYTES,
    ) {
        let mut section = vec![wire::PAT_TABLE_ID, 0xb0 | (body.len() >> 8) as u8, body.len() as u8];
        section.extend(body);
        let mut payload = start(&section);
        payload.extend_from_slice(&[wire::STUFFING_BYTE; wire::TS_PACKET_SIZE]);
        let mut sections = Sections::default();
        let mut complete = Vec::new();
        // The pointer byte and at least one section byte belong to the first packet.
        complete.extend(sections.push(true, &payload[..2]));
        for chunk in payload[2..].chunks(chunk_size) {
            complete.extend(sections.push(false, chunk));
        }
        prop_assert_eq!(complete, [section]);
        prop_assert!(sections.is_empty());
    }

    #[test]
    fn arbitrary_payloads_keep_pending_data_bounded_and_recover_at_a_new_start(
        payloads in prop::collection::vec((any::<bool>(), prop::collection::vec(any::<u8>(), 0..=MAX_PAYLOAD_BYTES)), 0..100),
    ) {
        let mut sections = Sections::default();
        for (start, payload) in payloads {
            for complete in sections.push(start, &payload) {
                prop_assert_eq!(wire::section_size(&complete), Ok(complete.len()));
            }
            if let Some(pending) = &sections.pending {
                prop_assert!(!pending.is_empty());
                prop_assert!(pending.len() < SECTION_PREFIX_SIZE + wire::MAX_PSI_SECTION_LENGTH);
            }
        }
        let valid = section(1);
        prop_assert_eq!(sections.push(true, &start(&valid)), [valid]);
        prop_assert!(sections.is_empty());
    }
}

#[test]
fn si_sections_keep_their_larger_length_limit() {
    const MAX_EIT_LENGTH: usize = 4093;
    let mut section = vec![0; SECTION_PREFIX_SIZE + MAX_EIT_LENGTH];
    section[0] = gstreamer_mpegts::ffi::GST_MTS_TABLE_ID_EVENT_INFORMATION_ACTUAL_TS_PRESENT as u8;
    section[1] = 0xf0 | (MAX_EIT_LENGTH >> 8) as u8;
    section[2] = MAX_EIT_LENGTH as u8;
    let crc_offset = section.len() - CRC_BYTES;
    let crc = wire::crc32_mpeg(&section[..crc_offset]);
    section[crc_offset..].copy_from_slice(&crc.to_be_bytes());
    let mut sections = Sections::si();
    let mut complete = sections.push(true, &start(&section[..1]));
    for chunk in section[1..].chunks(MAX_PAYLOAD_BYTES) {
        complete.extend(sections.push(false, chunk));
    }
    assert_eq!(complete, [section]);
    assert!(sections.is_empty());
}
