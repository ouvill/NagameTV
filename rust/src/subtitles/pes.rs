use super::{SubtitleCue, decoder::AribDecoder};
use std::collections::HashMap;

// A nonzero 16-bit PES length covers at most 65535 bytes after its six-byte
// prefix. Allow the rest of the final TS payload too. Apply the same finite
// budget to length-zero caption PES, which would otherwise wait indefinitely
// for the next payload start on damaged input.
const MAX_CAPTION_PES_BYTES: usize = u16::MAX as usize + 6 + 183;

/// Owns only subtitle payloads and native decoder state. Dropped when disabled.
pub(crate) struct CaptionDecoder {
    pes: HashMap<u16, Vec<u8>>,
    decoder: Option<AribDecoder>,
}
impl CaptionDecoder {
    #[cfg(test)]
    pub fn pending_bytes(&self, pid: u16) -> Option<usize> {
        self.pes.get(&pid).map(Vec::len)
    }

    pub fn new() -> Self {
        Self {
            pes: HashMap::new(),
            decoder: AribDecoder::new(),
        }
    }
    pub fn push(
        &mut self,
        pid: u16,
        payload_start: bool,
        payload: &[u8],
        texts: &mut Vec<SubtitleCue>,
    ) {
        if payload_start {
            if let Some(previous) = self.pes.remove(&pid) {
                self.decode_pes(&previous, texts);
            }
            self.pes.insert(pid, payload.to_vec());
        } else if let Some(pes) = self.pes.get_mut(&pid) {
            if pes.len() + payload.len() > MAX_CAPTION_PES_BYTES {
                self.pes.remove(&pid);
                tracing::warn!(
                    pid,
                    "Discarding oversized subtitle PES; waiting for next start"
                );
                return;
            }
            pes.extend_from_slice(payload);
        }
        let complete = self.pes.get(&pid).is_some_and(|pes| {
            if pes.len() < 6 {
                return false;
            }
            let size = u16::from_be_bytes([pes[4], pes[5]]) as usize;
            size != 0 && pes.len() >= size + 6
        });
        if complete && let Some(pes) = self.pes.remove(&pid) {
            self.decode_pes(&pes, texts);
        }
    }

    fn decode_pes(&mut self, pes: &[u8], texts: &mut Vec<SubtitleCue>) {
        if pes.len() < 9 || &pes[..3] != b"\0\0\x01" {
            return;
        }
        let payload_offset = 9 + pes[8] as usize;
        if payload_offset >= pes.len() {
            return;
        }
        if let Some(text) = self
            .decoder
            .as_mut()
            .and_then(|decoder| decoder.decode_pes(&pes[payload_offset..], pes_pts_ms(pes)))
        {
            texts.push(text);
        }
    }
}

pub(super) fn pes_pts_ms(pes: &[u8]) -> i64 {
    if pes.len() < 14 || pes[7] & 0x80 == 0 {
        return i64::MIN;
    }
    let pts = (((pes[9] as u64 >> 1) & 0x07) << 30)
        | ((pes[10] as u64) << 22)
        | (((pes[11] as u64 >> 1) & 0x7f) << 15)
        | ((pes[12] as u64) << 7)
        | ((pes[13] as u64 >> 1) & 0x7f);
    (pts / 90) as i64
}
