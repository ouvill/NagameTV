mod decoder;
mod gst_clock;
mod model;
mod timing;
pub(crate) use gst_clock::SubtitleClock;
pub(crate) use timing::SubtitleUpdate;

use decoder::AribDecoder;
pub use model::SubtitleCue;
use std::collections::{HashMap, HashSet};

/// Extracts ARIB caption PES packets directly from an MPEG-TS byte stream.
/// GStreamer's tsdemux intentionally does not expose Japanese broadcast
/// private-data streams (stream_type 0x06), so this runs before the demuxer.
pub struct TsSubtitleExtractor {
    pub audio_components: crate::audio::ComponentMap,
    bytes: Vec<u8>,
    psi: HashMap<u16, Vec<u8>>,
    pmt_pids: HashSet<u16>,
    subtitle_pids: HashSet<u16>,
    pes: HashMap<u16, Vec<u8>>,
    decoder: Option<AribDecoder>,
}

impl TsSubtitleExtractor {
    pub fn new() -> Self {
        Self {
            audio_components: Default::default(),
            bytes: Vec::new(),
            psi: HashMap::new(),
            pmt_pids: HashSet::new(),
            subtitle_pids: HashSet::new(),
            pes: HashMap::new(),
            decoder: AribDecoder::new(),
        }
    }

    pub fn push(&mut self, data: &[u8]) -> Vec<SubtitleCue> {
        self.bytes.extend_from_slice(data);
        let mut texts = Vec::new();
        loop {
            let Some(sync) = self.bytes.iter().position(|byte| *byte == 0x47) else {
                self.bytes.clear();
                break;
            };
            if sync > 0 {
                self.bytes.drain(..sync);
            }
            if self.bytes.len() < 188 {
                break;
            }
            if self.bytes.get(188).is_some_and(|byte| *byte != 0x47) {
                self.bytes.remove(0);
                continue;
            }
            let packet: Vec<u8> = self.bytes.drain(..188).collect();
            self.handle_packet(&packet, &mut texts);
        }
        texts
    }

    fn handle_packet(&mut self, packet: &[u8], texts: &mut Vec<SubtitleCue>) {
        if packet.len() != 188 || packet[0] != 0x47 || packet[1] & 0x80 != 0 {
            return;
        }
        let payload_start = packet[1] & 0x40 != 0;
        let pid = (((packet[1] & 0x1f) as u16) << 8) | packet[2] as u16;
        let control = (packet[3] >> 4) & 0x03;
        if control != 1 && control != 3 {
            return;
        }
        let mut offset = 4;
        if control == 3 {
            offset += 1 + packet[4] as usize;
        }
        if offset >= packet.len() {
            return;
        }
        let payload = &packet[offset..];

        if pid == 0 || self.pmt_pids.contains(&pid) {
            if payload_start && !payload.is_empty() {
                let pointer = payload[0] as usize;
                if 1 + pointer <= payload.len() {
                    if let Some(section) = self.psi.get_mut(&pid) {
                        section.extend_from_slice(&payload[1..1 + pointer]);
                    }
                    self.parse_complete_psi_sections(pid);
                    self.psi.insert(pid, payload[1 + pointer..].to_vec());
                }
            } else if let Some(section) = self.psi.get_mut(&pid) {
                section.extend_from_slice(payload);
            }
            self.parse_complete_psi_sections(pid);
            return;
        }
        if !self.subtitle_pids.contains(&pid) {
            return;
        }
        if payload_start {
            if let Some(previous) = self.pes.remove(&pid) {
                self.decode_pes(&previous, texts);
            }
            self.pes.insert(pid, payload.to_vec());
        } else if let Some(pes) = self.pes.get_mut(&pid) {
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

    fn parse_complete_psi_sections(&mut self, pid: u16) {
        loop {
            let section_size = self.psi.get(&pid).and_then(|data| {
                if data.len() < 3 || data[0] == 0xff {
                    return None;
                }
                Some(3 + ((((data[1] & 0x0f) as usize) << 8) | data[2] as usize))
            });
            let Some(section_size) = section_size else {
                break;
            };
            if !self
                .psi
                .get(&pid)
                .is_some_and(|data| data.len() >= section_size)
            {
                break;
            }
            let section = self
                .psi
                .get_mut(&pid)
                .expect("PSI buffer exists")
                .drain(..section_size)
                .collect::<Vec<_>>();
            self.parse_psi(pid, &section);
        }
    }

    fn parse_psi(&mut self, pid: u16, section: &[u8]) {
        if section.len() < 3 {
            return;
        }
        let section_len = (((section[1] & 0x0f) as usize) << 8) | section[2] as usize;
        if section.len() < section_len + 3 {
            return;
        }
        if pid == 0 && section[0] == 0x00 {
            let end = section_len.saturating_sub(1);
            let mut pos = 8;
            while pos + 4 <= end {
                let program = u16::from_be_bytes([section[pos], section[pos + 1]]);
                let pmt_pid = (((section[pos + 2] & 0x1f) as u16) << 8) | section[pos + 3] as u16;
                if program != 0 {
                    self.pmt_pids.insert(pmt_pid);
                }
                pos += 4;
            }
        } else if self.pmt_pids.contains(&pid) && section[0] == 0x02 && section.len() >= 12 {
            if let Some((service, components)) = crate::audio::pmt_components(section) {
                self.audio_components.insert(service, components);
            }
            let program_info_len = (((section[10] & 0x0f) as usize) << 8) | section[11] as usize;
            let end = section_len.saturating_sub(1);
            let mut pos = 12 + program_info_len;
            while pos + 5 <= end {
                let stream_type = section[pos];
                let elementary_pid =
                    (((section[pos + 1] & 0x1f) as u16) << 8) | section[pos + 2] as u16;
                let info_len =
                    (((section[pos + 3] & 0x0f) as usize) << 8) | section[pos + 4] as usize;
                if pos + 5 + info_len > section.len() {
                    break;
                }
                let descriptors = &section[pos + 5..pos + 5 + info_len];
                if stream_type == 0x06 && is_caption_stream(descriptors) {
                    if self.subtitle_pids.insert(elementary_pid) {
                        tracing::debug!(
                            pid = format_args!("0x{elementary_pid:04x}"),
                            "ARIB subtitle stream detected"
                        );
                    }
                }
                pos += 5 + info_len;
            }
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

fn pes_pts_ms(pes: &[u8]) -> i64 {
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

fn is_caption_stream(descriptors: &[u8]) -> bool {
    let mut component_tag = None;
    let mut data_component_id = None;
    let mut pos = 0;
    while pos + 2 <= descriptors.len() {
        let tag = descriptors[pos];
        let len = descriptors[pos + 1] as usize;
        if pos + 2 + len > descriptors.len() {
            break;
        }
        let value = &descriptors[pos + 2..pos + 2 + len];
        if tag == 0x52 && !value.is_empty() {
            component_tag = Some(value[0]);
        } else if tag == 0xfd && value.len() >= 2 {
            data_component_id = Some(u16::from_be_bytes([value[0], value[1]]));
        }
        pos += 2 + len;
    }
    data_component_id == Some(0x0008)
        && component_tag.is_some_and(|tag| (0x30..=0x37).contains(&tag))
}

#[cfg(test)]
mod tests {
    use super::TsSubtitleExtractor;

    #[test]
    fn updates_audio_mapping_from_fragmented_pmt_with_pointer_completion() {
        fn packet(payload: &[u8]) -> Vec<u8> {
            let mut packet = vec![0xff; 188];
            packet[..4].copy_from_slice(&[0x47, 0x41, 0x00, 0x30]);
            let adaptation = 183 - payload.len();
            packet[4] = adaptation as u8;
            if adaptation > 0 {
                packet[5] = 0;
            }
            packet[5 + adaptation..].copy_from_slice(payload);
            packet
        }
        let first = [
            0x2, 0xb0, 0x15, 0x0, 0xa, 0xc1, 0x0, 0x0, 0xe2, 0x0, 0xf0, 0x0, 0xf, 0xe2, 0x1, 0xf0,
            0x3, 0x52, 0x1, 0x10, 0xf6, 0x9b, 0x97, 0x17,
        ];
        let replacement = [
            0x2, 0xb0, 0x15, 0x0, 0xa, 0xc1, 0x0, 0x0, 0xe2, 0x0, 0xf0, 0x0, 0xf, 0xe2, 0x2, 0xf0,
            0x3, 0x52, 0x1, 0x11, 0xdf, 0x22, 0x9d, 0x28,
        ];
        let mut extractor = TsSubtitleExtractor::new();
        extractor.pmt_pids.insert(0x100);
        let mut payload = vec![0];
        payload.extend_from_slice(&first[..12]);
        extractor.push(&packet(&payload));
        assert!(extractor.audio_components.is_empty());
        let mut payload = vec![(first.len() - 12) as u8];
        payload.extend_from_slice(&first[12..]);
        payload.extend_from_slice(&replacement);
        let packet = packet(&payload);
        for fragment in packet.chunks(7) {
            extractor.push(fragment);
        }
        let components = &extractor.audio_components[&10];
        assert_eq!(components.len(), 1);
        assert_eq!(components.get(&0x202), Some(&17));
        assert!(!components.contains_key(&0x201));
    }

    #[test]
    #[ignore = "requires an MPEG-TS fixture supplied through MIRAKURUN_SUBTITLE_TS_FIXTURE"]
    fn discovers_caption_stream_in_fixture() {
        let path = std::env::var("MIRAKURUN_SUBTITLE_TS_FIXTURE").unwrap();
        let data = std::fs::read(path).unwrap();
        let mut extractor = TsSubtitleExtractor::new();
        let mut texts = Vec::new();
        for chunk in data.chunks(16 * 1024) {
            texts.extend(extractor.push(chunk));
        }
        assert!(
            !extractor.subtitle_pids.is_empty(),
            "no ARIB caption stream was discovered"
        );
        assert!(
            texts.iter().any(|cue| !cue.cells.is_empty()),
            "caption had no positioned cells"
        );
        eprintln!("decoded {} subtitle screens", texts.len());
    }
}
