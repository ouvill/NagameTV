//! Shared MPEG-TS framing and PAT/PMT discovery for audio and subtitles.
use super::{CaptionDecoder, SubtitleCue};
use std::collections::{HashMap, HashSet};

/// Parses common transport tables and optionally forwards subtitle payloads.
/// GStreamer's tsdemux intentionally does not expose Japanese broadcast
/// private-data streams (stream_type 0x06), so this runs before the demuxer.
pub struct TransportParser {
    service: Option<u16>,
    bytes: Vec<u8>,
    psi: HashMap<u16, Vec<u8>>,
    pmt_pids: HashSet<u16>,
    subtitle_pids: HashSet<u16>,
    captions: Option<CaptionDecoder>,
}

impl TransportParser {
    pub fn new(subtitles_enabled: bool) -> Self {
        Self {
            service: None,
            bytes: Vec::new(),
            psi: HashMap::new(),
            pmt_pids: HashSet::new(),
            subtitle_pids: HashSet::new(),
            captions: subtitles_enabled.then(CaptionDecoder::new),
        }
    }

    pub fn select_service(&mut self, service: u16) {
        self.service = Some(service);
    }

    pub fn decoder_available(&self) -> bool {
        self.captions.as_ref().is_some_and(|c| c.available())
    }

    #[cfg(test)]
    pub fn subtitles_enabled(&self) -> bool {
        self.captions.is_some()
    }

    #[cfg(test)]
    pub fn set_subtitles_enabled(&mut self, enabled: bool) {
        if enabled != self.subtitles_enabled() {
            self.captions = enabled.then(CaptionDecoder::new);
        }
    }

    pub fn push(&mut self, data: &[u8]) -> Vec<SubtitleCue> {
        self.bytes.extend_from_slice(data);
        let mut texts = Vec::new();
        let mut consumed = 0;
        while consumed < self.bytes.len() {
            let Some(sync) = self.bytes[consumed..].iter().position(|byte| *byte == 0x47) else {
                consumed = self.bytes.len();
                break;
            };
            consumed += sync;
            if self.bytes.len() - consumed < 188 {
                break;
            }
            if self
                .bytes
                .get(consumed + 188)
                .is_some_and(|byte| *byte != 0x47)
            {
                consumed += 1;
                continue;
            }
            let mut packet = [0; 188];
            packet.copy_from_slice(&self.bytes[consumed..consumed + 188]);
            consumed += 188;
            self.handle_packet(&packet, &mut texts);
        }
        // Retain only the incomplete tail. Moving it once avoids shifting all
        // remaining input after every 188-byte packet in a large source buffer.
        self.bytes.drain(..consumed);
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
                if pointer < payload.len() {
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
        if let Some(captions) = self.captions.as_mut() {
            captions.push(pid, payload_start, payload, texts);
        }
    }

    fn parse_complete_psi_sections(&mut self, pid: u16) {
        loop {
            // Stuffing ends this assembly. Remove it, rather than retaining a
            // sentinel that would cause every following continuation to pile up.
            // Only a new payload start can create another assembly for this PID.
            if self
                .psi
                .get(&pid)
                .is_some_and(|data| data.first() == Some(&0xff))
            {
                self.psi.remove(&pid);
                break;
            }
            let section_size = self.psi.get(&pid).and_then(|data| {
                if data.len() < 3 {
                    return None;
                }
                Some(3 + ((((data[1] & 0x0f) as usize) << 8) | data[2] as usize))
            });
            let Some(section_size) = section_size else {
                break;
            };
            let Some(data) = self.psi.get_mut(&pid) else {
                break;
            };
            if data.len() < section_size {
                break;
            }
            let section = data.drain(..section_size).collect::<Vec<_>>();
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
                if program != 0
                    && self.service.is_none_or(|wanted| wanted == program)
                    && self.pmt_pids.len() < 32
                {
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
                if stream_type == 0x06
                    && is_caption_stream(descriptors)
                    && self.subtitle_pids.len() < 8
                    && self.subtitle_pids.insert(elementary_pid)
                {
                    tracing::debug!(
                        pid = format_args!("0x{elementary_pid:04x}"),
                        "ARIB subtitle stream detected"
                    );
                }
                pos += 5 + info_len;
            }
        }
    }
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
    // In tests, unwrap/expect assert successful setup or an expected result.
    // Failures intentionally fail the test; they are not assumed impossible IO.
    use super::TransportParser;

    fn ts_packet(pid: u16, start: bool, payload: &[u8]) -> [u8; 188] {
        assert!(!payload.is_empty() && payload.len() <= 184);
        let mut packet = [0xff; 188];
        packet[..4].copy_from_slice(&[
            0x47,
            ((pid >> 8) as u8 & 0x1f) | if start { 0x40 } else { 0 },
            pid as u8,
            if payload.len() == 184 { 0x10 } else { 0x30 },
        ]);
        if payload.len() < 184 {
            packet[4] = (183 - payload.len()) as u8;
            if packet[4] > 0 {
                packet[5] = 0;
            }
        }
        packet[188 - payload.len()..].copy_from_slice(payload);
        packet
    }

    #[test]
    fn session_parser_uses_explicit_service_metadata_with_an_unrelated_endpoint_id()
    -> Result<(), Box<dyn std::error::Error>> {
        let channels = crate::channels::parse(
            br#"[{"id":777,"name":"Fixture","type":1,"networkId":4,"serviceId":42}]"#,
        )?;
        let channel = channels.first().ok_or("missing test channel")?;
        let mut parser = super::super::parser_for(channel.broadcast)?;
        // PAT maps service 7 to PID 0x100 and service 42 to PID 0x101.
        let mut section = vec![0, 0xb0, 17, 0, 1, 0xc1, 0, 0, 0, 7, 0xe1, 0, 0, 42, 0xe1, 1];
        let mut crc = 0xffff_ffff_u32;
        for &byte in &section {
            crc ^= u32::from(byte) << 24;
            for _ in 0..8 {
                crc = if crc & 0x8000_0000 == 0 {
                    crc << 1
                } else {
                    (crc << 1) ^ 0x04c1_1db7
                };
            }
        }
        section.extend_from_slice(&crc.to_be_bytes());
        section.insert(0, 0); // PSI pointer field.
        parser.push(&ts_packet(0, true, &section));
        assert_eq!(parser.pmt_pids, std::collections::HashSet::from([0x101]));
        assert!(matches!(
            super::super::parser_for(None),
            Err(super::super::Error::MissingService)
        ));
        Ok(())
    }

    #[test]
    fn disabling_drops_partial_subtitles_and_reenable_waits_for_start() {
        let mut parser = TransportParser::new(false);
        parser.subtitle_pids.insert(0x120);
        let start = ts_packet(0x120, true, &[0, 0, 1, 0xbd, 0, 0, 0x80, 0, 0]);
        let continuation = ts_packet(0x120, false, &[0x55; 184]);
        for _ in 0..100 {
            assert!(parser.push(&start).is_empty());
            assert!(parser.push(&continuation).is_empty());
            assert!(parser.captions.is_none());
        }
        parser.set_subtitles_enabled(true);
        parser.push(&continuation);
        assert_eq!(parser.captions.as_ref().unwrap().pending_bytes(0x120), None);
        parser.push(&start);
        assert_eq!(
            parser.captions.as_ref().unwrap().pending_bytes(0x120),
            Some(9)
        );
        parser.set_subtitles_enabled(true);
        assert_eq!(
            parser.captions.as_ref().unwrap().pending_bytes(0x120),
            Some(9)
        );
        parser.set_subtitles_enabled(false);
        assert!(parser.captions.is_none());
        parser.set_subtitles_enabled(true);
        parser.push(&continuation);
        assert_eq!(parser.captions.as_ref().unwrap().pending_bytes(0x120), None);
        parser.push(&start);
        assert_eq!(
            parser.captions.as_ref().unwrap().pending_bytes(0x120),
            Some(9)
        );
    }

    #[test]
    fn psi_stuffing_does_not_accumulate_and_next_section_recovers() {
        let mut extractor = TransportParser::new(true);
        extractor.push(&ts_packet(0, true, &[0, 0xff]));
        for _ in 0..1000 {
            extractor.push(&ts_packet(0, false, &[0xff; 184]));
        }
        assert!(extractor.psi.get(&0).is_none_or(Vec::is_empty));
        let pat = [0, 0, 0xb0, 13, 0, 1, 0xc1, 0, 0, 0, 1, 0xe1, 0, 0, 0, 0, 0];
        extractor.push(&ts_packet(0, true, &pat));
        assert!(extractor.pmt_pids.contains(&0x100));
    }

    #[test]
    fn unterminated_pes_is_discarded_until_the_next_start() {
        let mut extractor = TransportParser::new(true);
        extractor.subtitle_pids.insert(0x120);
        extractor.push(&ts_packet(0x120, true, &[0, 0, 1, 0xbd, 0, 0, 0x80, 0, 0]));
        for _ in 0..1000 {
            extractor.push(&ts_packet(0x120, false, &[0x55; 184]));
        }
        assert!(
            !extractor
                .captions
                .as_ref()
                .unwrap()
                .pending_bytes(0x120)
                .is_some()
        );
        extractor.push(&ts_packet(0x120, true, &[0, 0, 1, 0xbd, 0, 10]));
        assert_eq!(
            extractor
                .captions
                .as_ref()
                .unwrap()
                .pending_bytes(0x120)
                .unwrap(),
            6
        );
        extractor.push(&ts_packet(0x120, false, &[0; 10]));
        assert!(
            !extractor
                .captions
                .as_ref()
                .unwrap()
                .pending_bytes(0x120)
                .is_some()
        );
    }

    #[test]
    fn largest_nonzero_pes_length_is_accepted_including_final_ts_padding() {
        let mut extractor = TransportParser::new(true);
        extractor.subtitle_pids.insert(0x120);
        let mut pes = vec![0; u16::MAX as usize + 6];
        pes[..6].copy_from_slice(&[0, 0, 1, 0xbd, 0xff, 0xff]);
        let mut consumed = 0;
        for (index, chunk) in pes.chunks(184).enumerate() {
            let mut payload = [0xff; 184];
            payload[..chunk.len()].copy_from_slice(chunk);
            extractor.push(&ts_packet(0x120, index == 0, &payload));
            consumed += chunk.len();
            if consumed < pes.len() {
                assert_eq!(
                    extractor
                        .captions
                        .as_ref()
                        .unwrap()
                        .pending_bytes(0x120)
                        .unwrap(),
                    consumed
                );
            }
        }
        assert!(
            !extractor
                .captions
                .as_ref()
                .unwrap()
                .pending_bytes(0x120)
                .is_some()
        );
    }

    #[test]
    fn framing_preserves_tables_across_chunk_sizes_and_garbage_prefix() {
        let pat = [0, 0, 0xb0, 13, 0, 1, 0xc1, 0, 0, 0, 1, 0xe1, 0, 0, 0, 0, 0];
        let packet = ts_packet(0, true, &pat);
        let mut input = vec![0x11, 0x47, 0x22, 0x33];
        for _ in 0..256 {
            input.extend_from_slice(&packet);
        }
        // End mid-packet, then supply its remainder in a later call.
        input.extend_from_slice(&packet[..73]);
        for chunk_size in [1, 17, 187, 188, 189, 16384, input.len()] {
            let mut parser = TransportParser::new(false);
            for chunk in input.chunks(chunk_size) {
                assert!(parser.push(chunk).is_empty());
                assert!(parser.bytes.len() < 188);
            }
            assert_eq!(
                parser.pmt_pids,
                std::collections::HashSet::from([0x100]),
                "chunk size {chunk_size}"
            );
            assert_eq!(parser.bytes, packet[..73]);
            assert!(parser.push(&packet[73..]).is_empty());
            assert!(parser.bytes.is_empty());
        }
    }

    #[test]
    #[ignore = "requires an MPEG-TS fixture supplied through MIRAKURUN_SUBTITLE_TS_FIXTURE"]
    fn discovers_caption_stream_in_fixture() {
        let path = std::env::var("MIRAKURUN_SUBTITLE_TS_FIXTURE").unwrap();
        let data = std::fs::read(path).unwrap();
        let mut extractor = TransportParser::new(true);
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
