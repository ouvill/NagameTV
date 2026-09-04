use std::collections::{HashMap, HashSet};
use std::ffi::{c_char, c_void};
use std::ptr;

#[repr(C)]
struct AribInstance {
    _private: [u8; 0],
}
#[repr(C)]
struct AribParser {
    _private: [u8; 0],
}
#[repr(C)]
struct AribDecoderOpaque {
    _private: [u8; 0],
}

#[link(name = "aribb24")]
unsafe extern "C" {
    fn arib_instance_new(opaque: *mut c_void) -> *mut AribInstance;
    fn arib_instance_destroy(instance: *mut AribInstance);
    fn arib_get_parser(instance: *mut AribInstance) -> *mut AribParser;
    fn arib_get_decoder(instance: *mut AribInstance) -> *mut AribDecoderOpaque;
    fn arib_initialize_decoder_a_profile(decoder: *mut AribDecoderOpaque);
    fn arib_finalize_decoder(decoder: *mut AribDecoderOpaque);
    fn arib_parse_pes(parser: *mut AribParser, data: *const c_void, size: usize);
    fn arib_parser_get_data(parser: *mut AribParser, size: *mut usize) -> *const u8;
    fn arib_decode_buffer(
        decoder: *mut AribDecoderOpaque,
        data: *const u8,
        size: usize,
        output: *mut c_char,
        output_size: usize,
    ) -> usize;
}

pub struct AribDecoder {
    instance: *mut AribInstance,
    parser: *mut AribParser,
    decoder: *mut AribDecoderOpaque,
}

// Access is serialized by Playback's Mutex; libaribb24 instances are not shared otherwise.
unsafe impl Send for AribDecoder {}

impl AribDecoder {
    pub fn new() -> Option<Self> {
        unsafe {
            let instance = arib_instance_new(ptr::null_mut());
            if instance.is_null() {
                return None;
            }
            let parser = arib_get_parser(instance);
            let decoder = arib_get_decoder(instance);
            if parser.is_null() || decoder.is_null() {
                arib_instance_destroy(instance);
                return None;
            }
            arib_initialize_decoder_a_profile(decoder);
            Some(Self {
                instance,
                parser,
                decoder,
            })
        }
    }

    pub fn decode_pes(&mut self, pes: &[u8]) -> Option<String> {
        unsafe {
            arib_parse_pes(self.parser, pes.as_ptr().cast(), pes.len());
            let mut parsed_size = 0;
            let parsed = arib_parser_get_data(self.parser, &mut parsed_size);
            if parsed.is_null() || parsed_size == 0 {
                return None;
            }
            let mut output = vec![0_u8; 64 * 1024];
            let written = arib_decode_buffer(
                self.decoder,
                parsed,
                parsed_size,
                output.as_mut_ptr().cast(),
                output.len(),
            );
            if written == 0 || written > output.len() {
                return None;
            }
            let text = String::from_utf8_lossy(&output[..written])
                .trim_matches(char::from(0))
                .trim()
                .to_owned();
            (!text.is_empty()).then_some(text)
        }
    }
}

impl Drop for AribDecoder {
    fn drop(&mut self) {
        unsafe {
            arib_finalize_decoder(self.decoder);
            arib_instance_destroy(self.instance);
        }
    }
}

/// Extracts ARIB caption PES packets directly from an MPEG-TS byte stream.
/// GStreamer's tsdemux intentionally does not expose Japanese broadcast
/// private-data streams (stream_type 0x06), so this runs before the demuxer.
pub struct TsSubtitleExtractor {
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
            bytes: Vec::new(),
            psi: HashMap::new(),
            pmt_pids: HashSet::new(),
            subtitle_pids: HashSet::new(),
            pes: HashMap::new(),
            decoder: AribDecoder::new(),
        }
    }

    pub fn push(&mut self, data: &[u8]) -> Vec<String> {
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

    fn handle_packet(&mut self, packet: &[u8], texts: &mut Vec<String>) {
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
                if 1 + pointer < payload.len() {
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
                        eprintln!("ARIB subtitle stream detected (PID 0x{elementary_pid:04x})");
                    }
                }
                pos += 5 + info_len;
            }
        }
    }

    fn decode_pes(&mut self, pes: &[u8], texts: &mut Vec<String>) {
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
            .and_then(|decoder| decoder.decode_pes(&pes[payload_offset..]))
        {
            texts.push(text);
        }
    }
}

fn is_caption_stream(descriptors: &[u8]) -> bool {
    let mut pos = 0;
    while pos + 2 <= descriptors.len() {
        let tag = descriptors[pos];
        let len = descriptors[pos + 1] as usize;
        if pos + 2 + len > descriptors.len() {
            break;
        }
        let value = &descriptors[pos + 2..pos + 2 + len];
        // ARIB STD-B24 data_component_descriptor; 0x0008 is captions.
        if tag == 0xfd && value.len() >= 2 && value[0] == 0 && value[1] == 8 {
            return true;
        }
        pos += 2 + len;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::TsSubtitleExtractor;

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
        eprintln!("decoded {} subtitle screens", texts.len());
    }
}
