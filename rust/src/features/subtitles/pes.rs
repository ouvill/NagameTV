use super::{SubtitleCue, decoder::AribDecoder};
#[cfg(test)]
use crate::transport::pes::{MISSING_TIMESTAMP, PES_HEADER_BYTES, PRIVATE_STREAM_1};
use crate::transport::pes::{PES_PREFIX_BYTES, PesHeader};
use std::collections::HashMap;

// A declared PES length covers bytes after its six-byte prefix. The final TS
// payload can also contain stuffing; never pass that stuffing to the decoder.
const MAX_CAPTION_PES_BYTES: usize = u16::MAX as usize + PES_PREFIX_BYTES + 183;

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

    pub fn available(&self) -> bool {
        self.decoder.is_some()
    }

    pub fn new() -> Self {
        Self {
            pes: HashMap::new(),
            decoder: AribDecoder::new(),
        }
    }

    /// Lost transport data invalidates both PES assembly and ARIB management state.
    pub fn discontinuity(&mut self) {
        self.pes.clear();
        self.decoder = AribDecoder::new();
    }

    pub fn push(
        &mut self,
        pid: u16,
        payload_start: bool,
        payload: &[u8],
        texts: &mut Vec<SubtitleCue>,
    ) {
        if payload_start {
            // Complete PES packets are decoded immediately below. A pending
            // packet at the next start is truncated and must not be decoded.
            self.pes.remove(&pid);
            if payload.len() > MAX_CAPTION_PES_BYTES {
                return;
            }
            self.pes.insert(pid, payload.to_vec());
        } else if let Some(pes) = self.pes.get_mut(&pid) {
            if payload.len() > MAX_CAPTION_PES_BYTES.saturating_sub(pes.len()) {
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
            let Some(prefix) = pes.get(..PES_PREFIX_BYTES) else {
                return false;
            };
            let size = u16::from_be_bytes([prefix[4], prefix[5]]) as usize;
            size != 0 && pes.len() >= size + PES_PREFIX_BYTES
        });
        if complete && let Some(pes) = self.pes.remove(&pid) {
            self.decode_pes(&pes, texts);
        }
    }

    fn decode_pes(&mut self, pes: &[u8], texts: &mut Vec<SubtitleCue>) {
        let Some(header) = PesHeader::parse(pes) else {
            return;
        };
        let Some(payload) = header.caption_payload(pes) else {
            return;
        };
        if let Some(text) = self
            .decoder
            .as_mut()
            .and_then(|decoder| decoder.decode_pes(payload, header.pts_ms))
        {
            texts.push(text);
        }
    }
}

#[cfg(test)]
pub(super) fn pes_pts_ms(pes: &[u8]) -> i64 {
    PesHeader::parse(pes).map_or(MISSING_TIMESTAMP, |header| header.pts_ms)
}

#[cfg(test)]
use super::fixture;

#[cfg(test)]
mod tests {
    use super::*;

    fn packet() -> Vec<u8> {
        let length = u16::try_from(8 + fixture::SAMPLE.len()).unwrap();
        let mut bytes = vec![0, 0, 1, PRIVATE_STREAM_1];
        bytes.extend_from_slice(&length.to_be_bytes());
        bytes.extend_from_slice(&[0x80, 0x80, 5, 0x21, 0, 5, 0xbf, 0x21]); // 90,000 ticks.
        bytes.extend_from_slice(fixture::SAMPLE);
        bytes
    }

    #[test]
    fn assembles_caption_and_excludes_transport_stuffing() {
        let mut bytes = packet();
        let end = bytes.len();
        bytes.extend_from_slice(&[0xff; 32]);
        let header = PesHeader::parse(&bytes).unwrap();
        assert_eq!(
            header.caption_payload(&bytes).unwrap(),
            &bytes[PES_HEADER_BYTES + 5..end]
        );
        let mut decoder = CaptionDecoder::new();
        let mut cues = Vec::new();
        decoder.push(0x130, true, &bytes[..30], &mut cues);
        assert!(cues.is_empty());
        decoder.push(0x130, false, &bytes[30..], &mut cues);
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].text, "♬〜");
        assert_eq!(cues[0].pts_ms, Some(1000));
        assert_eq!(decoder.pending_bytes(0x130), None);
    }

    #[test]
    fn next_start_discards_even_a_decodable_but_declared_truncated_packet() {
        let mut truncated = packet();
        let length = u16::from_be_bytes([truncated[4], truncated[5]]) + 20;
        truncated[4..6].copy_from_slice(&length.to_be_bytes());
        let mut decoder = CaptionDecoder::new();
        let mut cues = Vec::new();
        decoder.push(0x130, true, &truncated, &mut cues);
        decoder.push(0x130, true, &packet(), &mut cues);
        assert_eq!(cues.len(), 1);
    }

    #[test]
    fn validates_pes_and_timestamp_markers_without_requiring_video_body() {
        let valid = packet();
        assert_eq!(pes_pts_ms(&valid), 1000);
        for (offset, value) in [
            (2, 0),
            (6, 0),
            (6, 0x90),
            (7, 0x40),
            (8, 4),
            (9, 0x31),
            (9, 0x20),
            (11, 4),
            (13, 0x20),
        ] {
            let mut malformed = valid.clone();
            malformed[offset] = value;
            assert!(
                PesHeader::parse(&malformed).is_none(),
                "offset {offset}, value {value}"
            );
        }
        for length in 0..14 {
            assert!(PesHeader::parse(&valid[..length]).is_none());
        }
        let mut video = valid[..14].to_vec();
        video[3] = 0xe0;
        assert_eq!(pes_pts_ms(&video), 1000);
        assert!(
            PesHeader::parse(&video)
                .unwrap()
                .caption_payload(&video)
                .is_none()
        );
        video[4..6].fill(0);
        assert_eq!(pes_pts_ms(&video), 1000);
    }

    #[test]
    fn validates_dts_and_optional_header_lengths() {
        let mut bytes = packet();
        bytes[7] = 0xc0;
        bytes[8] = 10;
        bytes[9] = 0x31;
        bytes.splice(14..14, [0x11, 0, 5, 0xbf, 0x21]);
        let length = u16::try_from(bytes.len() - PES_PREFIX_BYTES).unwrap();
        bytes[4..6].copy_from_slice(&length.to_be_bytes());
        assert_eq!(pes_pts_ms(&bytes), 1000);
        bytes[14] = 0x21;
        assert_eq!(pes_pts_ms(&bytes), MISSING_TIMESTAMP);
        let mut bytes = packet();
        bytes[7] |= 0x20; // ESCR flag without its required six bytes.
        assert!(PesHeader::parse(&bytes).is_none());
        bytes = packet();
        bytes[4..6].copy_from_slice(&3_u16.to_be_bytes());
        assert!(PesHeader::parse(&bytes).is_none());
    }

    #[test]
    fn discontinuity_drops_pending_data_and_zero_length_captions_are_rejected() {
        let mut bytes = packet();
        bytes[4..6].fill(0);
        assert!(
            PesHeader::parse(&bytes)
                .unwrap()
                .caption_payload(&bytes)
                .is_none()
        );
        let mut decoder = CaptionDecoder::new();
        let mut cues = Vec::new();
        decoder.push(0x130, true, &bytes[..30], &mut cues);
        decoder.discontinuity();
        assert_eq!(decoder.pending_bytes(0x130), None);
        decoder.push(0x130, false, &bytes[30..], &mut cues);
        assert!(cues.is_empty());
        decoder.push(0x130, true, &packet(), &mut cues);
        assert_eq!(cues.len(), 1);
    }
}
