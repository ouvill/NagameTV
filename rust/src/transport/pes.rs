//! Shared validated MPEG PES headers for captions and the playback time index.
use bitfield::bitfield;

const PES_START_CODE: [u8; 3] = [0, 0, 1];
pub(crate) const PRIVATE_STREAM_1: u8 = 0xbd;
pub(crate) const PES_PREFIX_BYTES: usize = 6;
pub(crate) const PES_HEADER_BYTES: usize = 9;
const TIMESTAMP_BYTES: usize = 5;
const CLOCK_TICKS_PER_MILLISECOND: u64 = 90;
pub(crate) const MISSING_TIMESTAMP: i64 = i64::MIN;
// ISO/IEC 13818-1 / ITU-T H.222.0 §2.4.3.6: PES packet header and timestamps.
// Wire bit positions live here; parsing below uses their field meanings.
bitfield! {
    struct PesControl(u8);
    impl Debug;
    u8;
    header_marker, _: 7, 6;
    scrambling, _: 5, 4;
}
bitfield! {
    struct PesOptionalFlags(u8);
    impl Debug;
    u8;
    timestamp_mode, _: 7, 6;
    escr, _: 5;
    es_rate, _: 4;
    trick_mode, _: 3;
    additional_copy_info, _: 2;
    previous_crc, _: 1;
    extension, _: 0;
}
bitfield! {
    struct TimestampBits(u64);
    impl Debug;
    u64;
    prefix, _: 39, 36;
    high, _: 35, 33;
    marker_high, _: 32;
    middle, _: 31, 17;
    marker_middle, _: 16;
    low, _: 15, 1;
    marker_low, _: 0;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TimestampMode {
    None,
    Pts,
    PtsAndDts,
}
impl TimestampMode {
    fn parse(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::None),
            2 => Some(Self::Pts),
            3 => Some(Self::PtsAndDts),
            _ => None, // 01 is forbidden.
        }
    }
}

pub(crate) struct PesHeader {
    stream_id: u8,
    declared_end: Option<usize>,
    payload_offset: usize,
    pub pts_ms: i64,
    pub pts_ticks: Option<u64>,
}

fn timestamp(bytes: &[u8], expected_prefix: u64) -> Option<u64> {
    let bytes = bytes.get(..TIMESTAMP_BYTES)?;
    let bits = TimestampBits(
        bytes
            .iter()
            .fold(0, |value, byte| (value << 8) | *byte as u64),
    );
    if bits.prefix() != expected_prefix
        || !bits.marker_high()
        || !bits.marker_middle()
        || !bits.marker_low()
    {
        return None;
    }
    Some((bits.high() << 30) | (bits.middle() << 15) | bits.low())
}

impl PesHeader {
    // Only the header must be present: video callers inspect the first TS
    // payload before the rest of the PES has arrived.
    pub fn parse(pes: &[u8]) -> Option<Self> {
        let fixed = pes.get(..PES_HEADER_BYTES)?;
        if fixed[..3] != PES_START_CODE {
            return None;
        }
        let control = PesControl(fixed[6]);
        if control.header_marker() != 2 || control.scrambling() != 0 {
            return None;
        }
        let declared_length = u16::from_be_bytes([fixed[4], fixed[5]]) as usize;
        let declared_end = (declared_length != 0).then_some(PES_PREFIX_BYTES + declared_length);
        let payload_offset = PES_HEADER_BYTES + fixed[8] as usize;
        if declared_end.is_some_and(|end| payload_offset > end) {
            return None;
        }
        let optional = pes.get(PES_HEADER_BYTES..payload_offset)?;
        let flags = PesOptionalFlags(fixed[7]);
        let mode = TimestampMode::parse(flags.timestamp_mode())?;
        let timestamp_length = match mode {
            TimestampMode::None => 0,
            TimestampMode::Pts => TIMESTAMP_BYTES,
            TimestampMode::PtsAndDts => 2 * TIMESTAMP_BYTES,
        };
        // Every advertised field needs room in PES_header_data_length, even
        // fields that subtitle rendering does not consume.
        let minimum_length = timestamp_length
            + usize::from(flags.escr()) * 6
            + usize::from(flags.es_rate()) * 3
            + usize::from(flags.trick_mode())
            + usize::from(flags.additional_copy_info())
            + usize::from(flags.previous_crc()) * 2
            + usize::from(flags.extension());
        if optional.len() < minimum_length {
            return None;
        }
        let pts_ticks = match mode {
            TimestampMode::None => None,
            TimestampMode::Pts => Some(timestamp(optional, 2)?),
            TimestampMode::PtsAndDts => {
                timestamp(optional.get(TIMESTAMP_BYTES..)?, 1)?;
                Some(timestamp(optional, 3)?)
            }
        };
        Some(Self {
            stream_id: fixed[3],
            declared_end,
            payload_offset,
            pts_ms: pts_ticks.map_or(MISSING_TIMESTAMP, |pts| {
                (pts / CLOCK_TICKS_PER_MILLISECOND) as i64
            }),
            pts_ticks,
        })
    }

    pub fn caption_payload<'a>(&self, pes: &'a [u8]) -> Option<&'a [u8]> {
        if self.stream_id != PRIVATE_STREAM_1 {
            return None;
        }
        // Packet length zero is allowed for video ES, not private_stream_1.
        let end = self.declared_end?;
        let payload = pes.get(self.payload_offset..end)?;
        (!payload.is_empty()).then_some(payload)
    }
}
