//! MPEG-TS reassembly and atomic PAT/PMT updates. Binary syntax lives in `wire`;
//! caption selection policy lives in `selection`.
use super::wire::{self, Pid, PsiSection, SYNC_BYTE, TS_PACKET_SIZE, TransportPacket};
use super::{CaptionDecoder, SubtitleCue, selection::select_caption};
use crate::transport::{Pat, Sections};
use std::collections::{HashMap, HashSet};

/// Parses transport tables and forwards exactly one selected subtitle ES.
/// The native ARIB decoder owns the management/character state of that ES.
pub struct TransportParser {
    service: Option<u16>,
    bytes: Vec<u8>,
    psi: HashMap<Pid, Sections>,
    pmt_pids: HashSet<Pid>,
    pat: Pat,
    active_transport: Option<u16>,
    active_service: Option<u16>,
    continuity: HashMap<Pid, (u8, [u8; TS_PACKET_SIZE])>,
    subtitle_pid: Option<Pid>,
    caption_generation: u64,
    reset_pending: bool,
    captions: Option<CaptionDecoder>,
}

impl TransportParser {
    pub fn new(subtitles_enabled: bool) -> Self {
        Self {
            service: None,
            bytes: Vec::new(),
            psi: HashMap::new(),
            pmt_pids: HashSet::new(),
            pat: Pat::default(),
            continuity: HashMap::new(),
            active_transport: None,
            active_service: None,
            subtitle_pid: None,
            caption_generation: 0,
            reset_pending: false,
            captions: subtitles_enabled.then(CaptionDecoder::new),
        }
    }

    pub fn select_service(&mut self, service: u16) {
        if self.service != Some(service) {
            self.service = Some(service);
            self.psi.clear();
            self.pmt_pids.clear();
            self.pat = Pat::default();
            self.active_transport = None;
            self.active_service = None;
            self.continuity.clear();
            self.select_pid(None);
        }
    }

    pub fn decoder_available(&self) -> bool {
        self.captions.as_ref().is_some_and(|c| c.available())
    }

    pub fn take_caption_reset(&mut self) -> bool {
        std::mem::take(&mut self.reset_pending)
    }

    fn reset_captions(&mut self) {
        self.caption_generation = self.caption_generation.wrapping_add(1);
        self.reset_pending = true;
        if let Some(captions) = self.captions.as_mut() {
            captions.discontinuity();
        }
    }

    #[cfg(test)]
    pub fn subtitles_enabled(&self) -> bool {
        self.captions.is_some()
    }

    #[cfg(test)]
    pub fn set_subtitles_enabled(&mut self, enabled: bool) {
        if enabled != self.subtitles_enabled() {
            self.captions = enabled.then(CaptionDecoder::new);
            if let Some(pid) = self.subtitle_pid {
                self.continuity.remove(&pid);
            }
        }
    }

    pub fn push(&mut self, data: &[u8]) -> Vec<SubtitleCue> {
        self.bytes.extend_from_slice(data);
        let mut texts = Vec::new();
        let mut consumed = 0;
        while consumed < self.bytes.len() {
            let Some(sync) = self.bytes[consumed..]
                .iter()
                .position(|byte| *byte == SYNC_BYTE)
            else {
                consumed = self.bytes.len();
                break;
            };
            consumed += sync;
            if self.bytes.len() - consumed < TS_PACKET_SIZE {
                break;
            }
            if self
                .bytes
                .get(consumed + TS_PACKET_SIZE)
                .is_some_and(|byte| *byte != SYNC_BYTE)
            {
                consumed += 1;
                continue;
            }
            let mut packet = [0; TS_PACKET_SIZE];
            packet.copy_from_slice(&self.bytes[consumed..consumed + TS_PACKET_SIZE]);
            consumed += TS_PACKET_SIZE;
            let generation = self.caption_generation;
            let previous_cues = texts.len();
            self.handle_packet(&packet, &mut texts);
            if generation != self.caption_generation {
                texts.drain(..previous_cues);
            }
        }
        self.bytes.drain(..consumed);
        texts
    }

    fn reset_pid(&mut self, pid: Pid) {
        self.psi.remove(&pid);
        if pid == Pid::PAT {
            self.pat = Pat::default();
        }
        if self.subtitle_pid == Some(pid) {
            self.reset_captions();
        }
    }

    fn handle_packet(&mut self, bytes: &[u8; TS_PACKET_SIZE], texts: &mut Vec<SubtitleCue>) {
        let Ok(packet) = TransportPacket::parse(bytes) else {
            return;
        };
        let pid = packet.pid;
        let is_table = pid == Pid::PAT || self.pmt_pids.contains(&pid);
        if !is_table && (self.subtitle_pid != Some(pid) || self.captions.is_none()) {
            return;
        }
        // Duplicates can carry a changed PCR, and can repeat a discontinuity
        // indicator. Suppress them before resetting assembly/decoder state.
        if !packet.payload.is_empty()
            && let Some((previous, previous_bytes)) = self.continuity.get(&pid)
            && *previous == packet.continuity_counter
            && wire::same_payload_packet(previous_bytes, bytes)
        {
            return;
        }
        if packet.discontinuity {
            self.reset_pid(pid);
            self.continuity.remove(&pid);
        }
        if packet.payload.is_empty() {
            return;
        }
        if let Some((previous, _)) = self.continuity.get(&pid)
            && packet.continuity_counter != (previous + 1) % 16
        {
            self.reset_pid(pid);
        }
        self.continuity
            .insert(pid, (packet.continuity_counter, *bytes));
        let payload = packet.payload;
        if is_table {
            for section in self.psi.entry(pid).or_default().push(packet.start, payload) {
                self.parse_psi(pid, &section);
            }
        } else if let Some(captions) = self.captions.as_mut() {
            captions.push(pid.0, packet.start, payload, texts);
        }
    }

    fn select_pid(&mut self, pid: Option<Pid>) {
        if self.subtitle_pid != pid {
            if let Some(old) = self.subtitle_pid {
                self.continuity.remove(&old);
            }
            self.subtitle_pid = pid;
            self.reset_captions();
            tracing::debug!(?pid, "ARIB subtitle stream selected");
        }
    }

    fn parse_psi(&mut self, pid: Pid, data: &[u8]) {
        let Ok(section) = PsiSection::parse(data) else {
            return;
        };
        if pid == Pid::PAT && section.table_id == wire::PAT_TABLE_ID {
            let Some(programs_by_service) = self.pat.push(&section) else {
                return;
            };
            // Production always specifies a service. Fixture/tool callers without
            // one choose one service deterministically, never mix multiple PMTs.
            let chosen = self
                .service
                .or_else(|| programs_by_service.keys().next().copied());
            let pmt_pids: HashSet<_> = chosen
                .and_then(|service| programs_by_service.get(&service).copied())
                .into_iter()
                .collect();
            if pmt_pids != self.pmt_pids
                || self.active_transport != Some(section.extension)
                || self.active_service != chosen
            {
                self.active_transport = Some(section.extension);
                self.active_service = chosen;
                self.pmt_pids = pmt_pids;
                self.psi.retain(|pid, _| *pid == Pid::PAT);
                self.continuity.retain(|pid, _| *pid == Pid::PAT);
                self.select_pid(None);
            }
        } else if self.pmt_pids.contains(&pid) && section.table_id == wire::PMT_TABLE_ID {
            let Ok(map) = section.program_map() else {
                return;
            };
            if self
                .service
                .or(self.active_service)
                .is_some_and(|wanted| wanted != map.service)
            {
                return;
            }
            self.select_pid(select_caption(&map.captions));
        }
    }
}

#[cfg(test)]
mod tests {
    // In tests, unwrap/expect assert successful setup or an expected result.
    // Failures intentionally fail the test; they are not assumed impossible IO.
    use super::{Pid, TransportParser};

    fn append_crc(mut section: Vec<u8>) -> Vec<u8> {
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
        section
    }

    fn pat_payload() -> Vec<u8> {
        let mut payload = vec![0];
        payload.extend(append_crc(vec![
            0, 0xb0, 13, 0, 1, 0xc1, 0, 0, 0, 1, 0xe1, 0,
        ]));
        payload
    }

    fn ts_packet(pid: u16, start: bool, payload: &[u8]) -> [u8; 188] {
        assert!(!payload.is_empty() && payload.len() <= 184);
        thread_local! {
            static COUNTERS: std::cell::RefCell<std::collections::HashMap<u16, u8>> = Default::default();
        }
        let counter = COUNTERS.with(|counters| {
            let mut counters = counters.borrow_mut();
            let next = counters.entry(pid).or_default();
            let counter = *next;
            *next = (*next + 1) % 16;
            counter
        });
        let mut packet = [0xff; 188];
        packet[..4].copy_from_slice(&[
            0x47,
            ((pid >> 8) as u8 & 0x1f) | if start { 0x40 } else { 0 },
            pid as u8,
            (if payload.len() == 184 { 0x10 } else { 0x30 }) | counter,
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

    fn pmt(streams: &[(u16, u8, Option<bool>)]) -> Vec<u8> {
        let mut section = vec![2, 0xb0, 0, 0, 1, 0xc1, 0, 0, 0xe1, 0, 0xf0, 0];
        for &(pid, tag, high_quality) in streams {
            let mut descriptors = vec![0x52, 1, tag, 0xfd, 2, 0, 8];
            if let Some(high_quality) = high_quality {
                let reference = streams
                    .iter()
                    .find(|&&(other, _, quality)| other != pid && quality == Some(!high_quality))
                    .map(|s| s.0)
                    .unwrap_or(Pid::NULL.0);
                descriptors.extend_from_slice(&[
                    0xc0,
                    3,
                    0xfe | u8::from(high_quality),
                    0xe0 | (reference >> 8) as u8,
                    reference as u8,
                ]);
            }
            section.extend_from_slice(&[
                6,
                0xe0 | (pid >> 8) as u8,
                pid as u8,
                0xf0,
                descriptors.len() as u8,
            ]);
            section.extend_from_slice(&descriptors);
        }
        let length = section.len() + 4 - 3;
        section[1] |= (length >> 8) as u8;
        section[2] = length as u8;
        append_crc(section)
    }

    fn apply_pmt(parser: &mut TransportParser, section: &[u8]) {
        parser.pmt_pids.insert(Pid(0x100));
        let mut payload = vec![0];
        payload.extend_from_slice(section);
        parser.push(&ts_packet(0x100, true, &payload));
    }

    #[test]
    fn high_quality_caption_wins_in_either_pmt_order_and_ignores_other_pes() {
        let high = (0x131, 0x31, Some(true));
        let low = (0x130, 0x30, Some(false));
        for streams in [[high, low], [low, high]] {
            let mut parser = TransportParser::new(true);
            apply_pmt(&mut parser, &pmt(&streams));
            assert_eq!(parser.subtitle_pid, Some(Pid(0x131)));
            for pid in [0x130, 0x131] {
                parser.push(&ts_packet(pid, true, &[0, 0, 1, 0xbd, 0, 0, 0x80, 0, 0]));
            }
            let decoder = parser.captions.as_ref().unwrap();
            assert_eq!(decoder.pending_bytes(0x130), None);
            assert_eq!(decoder.pending_bytes(0x131), Some(9));
            // A retransmitted PMT must not discard the selected stream's PES.
            apply_pmt(&mut parser, &pmt(&streams));
            assert_eq!(
                parser.captions.as_ref().unwrap().pending_bytes(0x131),
                Some(9)
            );
        }
    }

    #[test]
    fn caption_selection_falls_back_and_excludes_superimpose() {
        for (streams, expected) in [
            (vec![(0x131, 0x31, Some(false))], Some(0x131)),
            (vec![(0x130, 0x30, None)], Some(0x130)),
            (
                vec![(0x131, 0x31, None), (0x130, 0x30, Some(false))],
                Some(0x130),
            ),
            (
                vec![(0x130, 0x30, None), (0x138, 0x38, Some(true))],
                Some(0x130),
            ),
            (vec![(0x138, 0x38, Some(true))], None),
            (vec![(0x131, 0x31, None), (0x130, 0x30, None)], Some(0x130)),
        ] {
            let mut parser = TransportParser::new(true);
            apply_pmt(&mut parser, &pmt(&streams));
            assert_eq!(parser.subtitle_pid, expected.map(Pid));
        }
    }

    #[test]
    fn changed_or_removed_selection_discards_old_decoder_state() {
        let mut parser = TransportParser::new(true);
        apply_pmt(
            &mut parser,
            &pmt(&[(0x130, 0x30, Some(true)), (0x131, 0x31, Some(false))]),
        );
        parser.push(&ts_packet(0x130, true, &[0, 0, 1, 0xbd, 0, 0, 0x80, 0, 0]));
        apply_pmt(&mut parser, &pmt(&[(0x131, 0x31, Some(false))]));
        assert_eq!(parser.subtitle_pid, Some(Pid(0x131)));
        assert_eq!(parser.captions.as_ref().unwrap().pending_bytes(0x130), None);
        parser.push(&ts_packet(0x131, false, &[0x55; 10]));
        assert_eq!(parser.captions.as_ref().unwrap().pending_bytes(0x131), None);
        parser.push(&ts_packet(0x131, true, &[0, 0, 1, 0xbd, 0, 0, 0x80, 0, 0]));
        apply_pmt(&mut parser, &pmt(&[]));
        assert_eq!(parser.subtitle_pid, None);
        assert_eq!(parser.captions.as_ref().unwrap().pending_bytes(0x131), None);
    }

    #[test]
    fn disabled_subtitles_select_only_one_stream_without_allocating_decoder() {
        let mut parser = TransportParser::new(false);
        for _ in 0..100 {
            apply_pmt(
                &mut parser,
                &pmt(&[(0x130, 0x30, Some(true)), (0x131, 0x31, Some(false))]),
            );
            parser.push(&ts_packet(0x130, true, &[0, 0, 1, 0xbd, 0, 0, 0x80, 0, 0]));
            apply_pmt(&mut parser, &pmt(&[(0x131, 0x31, Some(false))]));
            assert_eq!(parser.subtitle_pid, Some(Pid(0x131)));
            assert!(parser.captions.is_none());
        }
    }

    #[test]
    fn invalid_future_or_other_service_pmt_preserves_selection_and_pending_pes() {
        let mut parser = TransportParser::new(true);
        parser.select_service(1);
        apply_pmt(&mut parser, &pmt(&[(0x130, 0x30, None)]));
        parser.push(&ts_packet(0x130, true, &[0, 0, 1, 0xbd, 0, 0, 0x80, 0, 0]));
        let valid = pmt(&[(0x131, 0x31, Some(true))]);
        for (index, value) in [
            (5, 0xc0), // future PMT
            (6, 1),    // incomplete multi-section table
            (7, 1),
            (4, 2),     // another service
            (11, 0xff), // program descriptor loop beyond the section
            (16, 0xff), // ES descriptor loop beyond the section
            (18, 0xff), // descriptor length beyond its ES loop
        ] {
            let mut invalid = valid.clone();
            invalid[index] = value;
            invalid.truncate(invalid.len() - 4);
            let invalid = append_crc(invalid);
            apply_pmt(&mut parser, &invalid);
            assert_eq!(
                parser.subtitle_pid,
                Some(Pid(0x130)),
                "changed byte {index}"
            );
            assert_eq!(
                parser.captions.as_ref().unwrap().pending_bytes(0x130),
                Some(9)
            );
        }
        parser.parse_psi(Pid(0x100), &valid[..valid.len() - 1]);
        assert_eq!(parser.subtitle_pid, Some(Pid(0x130)));
        assert_eq!(
            parser.captions.as_ref().unwrap().pending_bytes(0x130),
            Some(9)
        );
    }

    #[test]
    fn pat_changes_apply_atomically_and_invalidate_reused_pids() {
        let mut parser = TransportParser::new(true);
        parser.select_service(1);
        let pat = append_crc(vec![0, 0xb0, 13, 0, 1, 0xc1, 0, 0, 0, 1, 0xe1, 0]);
        parser.parse_psi(Pid::PAT, &pat);
        apply_pmt(&mut parser, &pmt(&[(0x130, 0x30, None)]));
        parser.push(&ts_packet(0x130, true, &[0, 0, 1, 0xbd, 0, 0, 0x80, 0, 0]));

        // A new two-section PAT is not applied until both sections arrive.
        let first = append_crc(vec![0, 0xb0, 13, 0, 1, 0xc3, 0, 1, 0, 1, 0xe1, 1]);
        parser.parse_psi(Pid::PAT, &first);
        assert_eq!(parser.subtitle_pid, Some(Pid(0x130)));
        let second = append_crc(vec![0, 0xb0, 13, 0, 1, 0xc3, 1, 1, 0, 2, 0xe1, 2]);
        parser.parse_psi(Pid::PAT, &second);
        assert_eq!(
            parser.pmt_pids,
            std::collections::HashSet::from([Pid(0x101)])
        );
        assert_eq!(parser.subtitle_pid, None);
        assert_eq!(parser.captions.as_ref().unwrap().pending_bytes(0x130), None);
        // The old PMT PID must not select captions again.
        parser.parse_psi(Pid(0x100), &pmt(&[(0x130, 0x30, None)]));
        assert_eq!(parser.subtitle_pid, None);
        parser.parse_psi(Pid(0x101), &pmt(&[(0x130, 0x30, None)]));
        assert_eq!(parser.subtitle_pid, Some(Pid(0x130)));

        // A new transport can reuse service and elementary PIDs.
        let other_transport = append_crc(vec![0, 0xb0, 13, 0, 2, 0xc1, 0, 0, 0, 1, 0xe1, 1]);
        parser.parse_psi(Pid::PAT, &other_transport);
        assert_eq!(parser.subtitle_pid, None);
    }

    #[test]
    fn bad_crc_preserves_selection_and_a_gap_discards_pending_pes() {
        let mut parser = TransportParser::new(true);
        apply_pmt(&mut parser, &pmt(&[(0x130, 0x30, None)]));
        let start = ts_packet(0x130, true, &[0, 0, 1, 0xbd, 0, 0, 0x80, 0, 0]);
        parser.push(&start);
        let mut invalid = pmt(&[(0x131, 0x30, None)]);
        *invalid.last_mut().unwrap() ^= 1;
        apply_pmt(&mut parser, &invalid);
        assert_eq!(parser.subtitle_pid, Some(Pid(0x130)));
        assert_eq!(
            parser.captions.as_ref().unwrap().pending_bytes(0x130),
            Some(9)
        );
        let mut continuation = ts_packet(0x130, false, &[0; 10]);
        continuation[3] = (continuation[3] & 0xf0) | ((start[3] + 2) & 0x0f);
        parser.push(&continuation);
        assert_eq!(parser.captions.as_ref().unwrap().pending_bytes(0x130), None);
        // Resume only at a fresh PES start, with the following counter.
        let mut next = ts_packet(0x130, true, &[0, 0, 1, 0xbd, 0, 0, 0x80, 0, 0]);
        next[3] = (next[3] & 0xf0) | ((continuation[3] + 1) & 0x0f);
        parser.push(&next);
        assert_eq!(
            parser.captions.as_ref().unwrap().pending_bytes(0x130),
            Some(9)
        );
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
        assert_eq!(
            parser.pmt_pids,
            std::collections::HashSet::from([Pid(0x101)])
        );
        assert!(matches!(
            super::super::parser_for(None),
            Err(super::super::Error::MissingService)
        ));
        Ok(())
    }

    #[test]
    fn disabling_drops_partial_subtitles_and_reenable_waits_for_start() {
        let mut parser = TransportParser::new(false);
        parser.subtitle_pid = Some(Pid(0x120));
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
        assert!(
            extractor
                .psi
                .get(&Pid::PAT)
                .is_none_or(crate::transport::Sections::is_empty)
        );
        let pat = pat_payload();
        extractor.push(&ts_packet(0, true, &pat));
        assert!(extractor.pmt_pids.contains(&Pid(0x100)));
    }

    #[test]
    fn unterminated_pes_is_discarded_until_the_next_start() {
        let mut extractor = TransportParser::new(true);
        extractor.subtitle_pid = Some(Pid(0x120));
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
        extractor.subtitle_pid = Some(Pid(0x120));
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
        let pat = pat_payload();
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
                std::collections::HashSet::from([Pid(0x100)]),
                "chunk size {chunk_size}"
            );
            assert_eq!(parser.bytes, packet[..73]);
            assert!(parser.push(&packet[73..]).is_empty());
            assert!(parser.bytes.is_empty());
        }
    }

    #[test]
    #[ignore = "requires an MPEG-TS fixture supplied through MIRAKURUN_SUBTITLE_TS_FIXTURE"]
    fn discovers_caption_stream_in_fixture() -> Result<(), Box<dyn std::error::Error>> {
        use std::io::Read;
        let path = std::env::var("MIRAKURUN_SUBTITLE_TS_FIXTURE")?;
        let mut input = std::fs::File::open(path)?;
        let mut extractor = TransportParser::new(true);
        if let Some(service) = std::env::var_os("MIRAKURUN_SUBTITLE_SERVICE_ID") {
            extractor.select_service(service.to_str().ok_or("service ID is not UTF-8")?.parse()?);
        }
        let mut buffer = [0; 16 * 1024];
        let mut screens = 0;
        let mut positioned = 0;
        let mut timestamped = 0;
        loop {
            let count = input.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            for cue in extractor.push(&buffer[..count]) {
                screens += 1;
                positioned += usize::from(!cue.cells.is_empty());
                timestamped += usize::from(cue.pts_ms.is_some());
            }
        }
        assert!(
            extractor.subtitle_pid.is_some(),
            "no ARIB caption stream was discovered"
        );
        assert!(positioned > 0, "caption had no positioned cells");
        eprintln!(
            "decoded {screens} subtitle screens; positioned={positioned}, timestamped={timestamped}"
        );
        Ok(())
    }
}
