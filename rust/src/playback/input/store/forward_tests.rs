//! Forward delivery protects the playback cursor independently of TS age.
use super::super::limits::{CONSUMED_MARGIN_BYTES, FORWARD_BUFFER_BYTES};
use super::*;

fn null_block() -> Vec<u8> {
    let mut packet = [0xff; super::super::TS_PACKET_SIZE];
    packet[..4].copy_from_slice(&[0x47, 0x1f, 0xff, 0x10]);
    packet.repeat(READ_BYTES / packet.len())
}

#[test]
fn unread_program_survives_more_than_two_seconds_of_media_time() -> std::io::Result<()> {
    let bytes = include_bytes!("../../../../../tests/fixtures/recording-seek.ts");
    let mut store = Store::new(Retention::Off, 1, true)?;
    store.append(bytes)?;
    assert!(store.index.end_ns().unwrap() > Duration::from_secs(30).as_nanos() as u64);
    assert_eq!(store.start(), 0);
    let mut offset = 0;
    while offset < bytes.len() as u64 {
        let ReadResult::Data { bytes: block, .. } = store.read_for_playback(offset)? else {
            panic!("unread TS was discarded")
        };
        assert_eq!(
            block.as_ref(),
            &bytes[offset as usize..offset as usize + block.as_ref().len()]
        );
        offset += block.as_ref().len() as u64;
        store.append(&[])?;
    }
    assert_eq!(offset, bytes.len() as u64);
    Ok(())
}

#[test]
fn small_chunks_fill_the_same_tail_and_only_consumed_bytes_are_reclaimed() -> std::io::Result<()> {
    let block = null_block();
    let mut store = Store::new(Retention::Off, 1, false)?;
    let total = (CONSUMED_MARGIN_BYTES as usize * 3).div_ceil(READ_BYTES) * READ_BYTES;
    for _ in 0..total / READ_BYTES {
        for packet in block.chunks(super::super::TS_PACKET_SIZE) {
            store.append(packet)?;
            // Diagnostic/preview reads neither advance nor pin the play cursor.
            assert!(matches!(
                store.read(store.end - packet.len() as u64)?,
                ReadResult::Data { .. }
            ));
        }
    }
    assert_eq!(store.start(), 0);
    assert_eq!(store.segments.len() * READ_BYTES, total);
    assert!(matches!(
        store.read_for_playback(store.end - READ_BYTES as u64)?,
        ReadResult::Data { .. }
    ));
    store.append(&block)?;
    assert!(store.start() > 0);
    let retained = store.end - store.start();
    assert!(retained >= CONSUMED_MARGIN_BYTES);
    assert!(retained <= CONSUMED_MARGIN_BYTES + (READ_BYTES * 2) as u64);
    assert!(matches!(
        store.read_for_playback(store.end - READ_BYTES as u64)?,
        ReadResult::Data { .. }
    ));
    Ok(())
}

#[test]
fn unread_overflow_is_bounded_logs_once_and_can_resume_at_a_new_cursor() {
    let (result, log) = crate::logging::tests::capture_output("warn", || -> std::io::Result<()> {
        let mut store = Store::new(Retention::Off, 1, false)?;
        let block = null_block();
        for _ in 0..(FORWARD_BUFFER_BYTES / READ_BYTES as u64 + 20) {
            store.append(&block)?;
            assert!(store.end - store.start() <= FORWARD_BUFFER_BYTES);
        }
        assert!(matches!(store.read_for_playback(0)?, ReadResult::Expired));
        assert!(matches!(store.playback, PlaybackCursor::Expired(0)));
        assert!(matches!(
            store.read_for_playback(store.start())?,
            ReadResult::Data { .. }
        ));
        assert!(matches!(store.playback, PlaybackCursor::Reading(_)));
        Ok(())
    });
    result.unwrap();
    assert_eq!(log.matches("Forward TS buffer limit exceeded").count(), 1);
}
