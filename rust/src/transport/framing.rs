//! File framing only; packet syntax is always ISO MPEG-TS's 188 bytes.
use super::wire::{SYNC_BYTE, TS_PACKET_SIZE};
const M2TS_PREFIX_BYTES: usize = 4;
const RS_PARITY_BYTES: usize = 16;
const SYNC_CONFIRMATIONS: usize = 3;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Framing {
    offset: u64,
    stride: usize,
}
impl Framing {
    pub fn transport() -> Self {
        Self {
            offset: 0,
            stride: TS_PACKET_SIZE,
        }
    }
    pub fn offset(self) -> u64 {
        self.offset
    }
    pub fn stride(self) -> usize {
        self.stride
    }

    pub fn detect(bytes: &[u8]) -> Option<Self> {
        for (offset, byte) in bytes.iter().enumerate() {
            if *byte != SYNC_BYTE || bytes.len() - offset < TS_PACKET_SIZE {
                continue;
            }
            for stride in [
                TS_PACKET_SIZE,
                TS_PACKET_SIZE + M2TS_PREFIX_BYTES,
                TS_PACKET_SIZE + RS_PARITY_BYTES,
            ] {
                let available = (bytes.len() - offset)
                    .div_ceil(stride)
                    .min(SYNC_CONFIRMATIONS);
                if (1..available)
                    .all(|number| bytes.get(offset + number * stride) == Some(&SYNC_BYTE))
                {
                    return Some(Self {
                        offset: offset as u64,
                        stride,
                    });
                }
            }
        }
        None
    }
    pub fn packets(self, bytes: &[u8]) -> impl Iterator<Item = (u64, &[u8])> {
        bytes
            .chunks(self.stride)
            .enumerate()
            .filter_map(move |(number, frame)| {
                frame
                    .get(..TS_PACKET_SIZE)
                    .map(|packet| ((number * self.stride) as u64, packet))
            })
    }
}
