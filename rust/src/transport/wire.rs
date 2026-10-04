//! ARIB caption stream selection on shared MPEG-2 TS/PSI syntax.
use bitfield::bitfield;
use std::ops::Deref;
#[cfg(test)]
pub(crate) use viewer_mpegts::{MAX_PSI_SECTION_LENGTH, SECTION_PREFIX_SIZE, section_size};
pub(crate) use viewer_mpegts::{
    PAT_TABLE_ID, PMT_TABLE_ID, ParseError, Pid, STUFFING_BYTE, SYNC_BYTE, TS_PACKET_SIZE,
    TransportPacket, crc32_mpeg, same_payload_packet,
};
#[cfg(test)]
pub(super) const TS_HEADER_SIZE: usize = 4;
#[cfg(test)]
const CRC_SIZE: usize = 4;
const PRIVATE_PES_STREAM: u8 = 0x06;
const ARIB_CAPTION_COMPONENT: u16 = 0x0008;
const STREAM_IDENTIFIER_DESCRIPTOR: u8 = 0x52;
const HIERARCHICAL_TRANSMISSION_DESCRIPTOR: u8 = 0xc0;
const DATA_COMPONENT_DESCRIPTOR: u8 = 0xfd;
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ComponentTag(pub u8);
impl ComponentTag {
    pub const DEFAULT_CAPTION: Self = Self(0x30);
    pub fn is_caption(self) -> bool {
        (Self::DEFAULT_CAPTION.0..=0x37).contains(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransmissionLayer {
    High,
    Low,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Hierarchy {
    pub layer: TransmissionLayer,
    pub reference: Option<Pid>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CaptionStream {
    pub pid: Pid,
    pub component_tag: ComponentTag,
    pub hierarchy: Option<Hierarchy>,
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ProgramMap {
    pub service: u16,
    pub pcr_pid: Pid,
    pub presentation_pids: Vec<Pid>,
    pub video_pids: Vec<Pid>,
    pub captions: Vec<CaptionStream>,
}

fn is_presentation_stream(stream_type: u8) -> bool {
    use gstreamer_mpegts::ffi as mpegts;
    // Only timed audio/video extends the received media interval. Captions,
    // data broadcasting and unknown private streams can announce future data.
    matches!(
        i32::from(stream_type),
        mpegts::GST_MPEGTS_STREAM_TYPE_VIDEO_MPEG1
            | mpegts::GST_MPEGTS_STREAM_TYPE_VIDEO_MPEG2
            | mpegts::GST_MPEGTS_STREAM_TYPE_VIDEO_MPEG4
            | mpegts::GST_MPEGTS_STREAM_TYPE_VIDEO_H264
            | mpegts::GST_MPEGTS_STREAM_TYPE_VIDEO_HEVC
            | mpegts::GST_MPEGTS_STREAM_TYPE_AUDIO_MPEG1
            | mpegts::GST_MPEGTS_STREAM_TYPE_AUDIO_MPEG2
            | mpegts::GST_MPEGTS_STREAM_TYPE_AUDIO_AAC_ADTS
            | mpegts::GST_MPEGTS_STREAM_TYPE_AUDIO_AAC_LATM
            | mpegts::GST_MPEGTS_STREAM_TYPE_AUDIO_AAC_CLEAN
    )
}

fn is_video_stream(stream_type: u8) -> bool {
    use gstreamer_mpegts::ffi as mpegts;
    matches!(
        i32::from(stream_type),
        mpegts::GST_MPEGTS_STREAM_TYPE_VIDEO_MPEG1
            | mpegts::GST_MPEGTS_STREAM_TYPE_VIDEO_MPEG2
            | mpegts::GST_MPEGTS_STREAM_TYPE_VIDEO_MPEG4
            | mpegts::GST_MPEGTS_STREAM_TYPE_VIDEO_H264
            | mpegts::GST_MPEGTS_STREAM_TYPE_VIDEO_HEVC
    )
}

bitfield! {
    struct PidField(u16);
    u8, reserved, _: 15, 13;
    u16, pid, _: 12, 0;
}
bitfield! {
    struct HierarchyFlags(u8);
    u8, reserved, _: 7, 1;
    bool, high, _: 0;
}
struct Cursor<'a> {
    rest: &'a [u8],
    exhausted: ParseError,
}
impl<'a> Cursor<'a> {
    // Once an outer length is satisfied, missing inner bytes indicate a bad
    // length/field, not a request for more transport input.
    fn bounded(rest: &'a [u8]) -> Self {
        Self {
            rest,
            exhausted: ParseError::Invalid("field exceeds declared length"),
        }
    }
    fn take(&mut self, length: usize) -> Result<&'a [u8], ParseError> {
        let (value, rest) = self.rest.split_at_checked(length).ok_or(self.exhausted)?;
        self.rest = rest;
        Ok(value)
    }
    fn byte(&mut self) -> Result<u8, ParseError> {
        Ok(self.take(1)?[0])
    }
    fn word(&mut self) -> Result<u16, ParseError> {
        let bytes = self.take(2)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }
    fn pid(&mut self) -> Result<Pid, ParseError> {
        let field = PidField(self.word()?);
        if field.reserved() != 0b111 {
            return Err(ParseError::Invalid("PID reserved bits"));
        }
        Ok(Pid(field.pid()))
    }
}

#[derive(Debug)]
pub(crate) struct PsiSection<'a>(pub viewer_mpegts::PsiSection<'a>);
impl<'a> PsiSection<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, ParseError> {
        viewer_mpegts::PsiSection::parse(bytes).map(Self)
    }
    pub fn program_map(&self) -> Result<ProgramMap, ParseError> {
        let map = self.0.program_map()?;
        Descriptors::parse(map.program_descriptors)?;
        let mut captions = Vec::new();
        let mut presentation_pids = Vec::new();
        let mut video_pids = Vec::new();
        let mut seen_component_tags = std::collections::HashSet::new();
        for stream in map.streams {
            let stream_type = stream.stream_type;
            let pid = stream.pid;
            let descriptors = Descriptors::parse(stream.descriptors)?;
            if let Some(tag) = descriptors.component_tag
                && !seen_component_tags.insert(tag)
            {
                return Err(ParseError::Invalid("duplicate component tag"));
            }
            if is_presentation_stream(stream_type) {
                presentation_pids.push(pid);
            }
            if is_video_stream(stream_type) {
                video_pids.push(pid);
            }
            if stream_type == PRIVATE_PES_STREAM
                && descriptors.data_component == Some(ARIB_CAPTION_COMPONENT)
                && let Some(component_tag) =
                    descriptors.component_tag.filter(|tag| tag.is_caption())
            {
                captions.push(CaptionStream {
                    pid,
                    component_tag,
                    hierarchy: descriptors.hierarchy,
                });
            }
        }
        Ok(ProgramMap {
            service: map.service,
            pcr_pid: map.pcr_pid,
            presentation_pids,
            video_pids,
            captions,
        })
    }
}
impl<'a> Deref for PsiSection<'a> {
    type Target = viewer_mpegts::PsiSection<'a>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
#[derive(Default)]
struct Descriptors {
    component_tag: Option<ComponentTag>,
    data_component: Option<u16>,
    hierarchy: Option<Hierarchy>,
}
impl Descriptors {
    fn parse(bytes: &[u8]) -> Result<Self, ParseError> {
        let mut cursor = Cursor::bounded(bytes);
        let mut result = Self::default();
        while !cursor.rest.is_empty() {
            let tag = cursor.byte()?;
            let length = usize::from(cursor.byte()?);
            let value = cursor.take(length)?;
            match tag {
                STREAM_IDENTIFIER_DESCRIPTOR => {
                    if length != 1 || result.component_tag.is_some() {
                        return Err(ParseError::Invalid("stream identifier descriptor"));
                    }
                    result.component_tag = Some(ComponentTag(value[0]));
                }
                DATA_COMPONENT_DESCRIPTOR => {
                    if result.data_component.is_some() {
                        return Err(ParseError::Invalid("duplicate data component descriptor"));
                    }
                    result.data_component = Some(Cursor::bounded(value).word()?);
                }
                HIERARCHICAL_TRANSMISSION_DESCRIPTOR => {
                    if length != 3 || result.hierarchy.is_some() {
                        return Err(ParseError::Invalid("hierarchical transmission descriptor"));
                    }
                    let mut value = Cursor::bounded(value);
                    let flags = HierarchyFlags(value.byte()?);
                    if flags.reserved() != 0x7f {
                        return Err(ParseError::Invalid("hierarchy reserved bits"));
                    }
                    let reference = value.pid()?;
                    result.hierarchy = Some(Hierarchy {
                        layer: if flags.high() {
                            TransmissionLayer::High
                        } else {
                            TransmissionLayer::Low
                        },
                        reference: (reference != Pid::NULL).then_some(reference),
                    });
                }
                _ => {} // Unknown descriptor bodies are opaque, but their lengths are checked.
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Fixed independently known CRC-32/MPEG-2 check value (not generated by parser).
    #[test]
    fn crc_matches_standard_check_vector() {
        assert_eq!(crc32_mpeg(b"123456789"), 0x0376_e6e7);
    }

    fn section(mut bytes: Vec<u8>) -> Vec<u8> {
        let size = bytes.len() + CRC_SIZE - SECTION_PREFIX_SIZE;
        bytes[1] = 0xb0 | (size >> 8) as u8;
        bytes[2] = size as u8;
        bytes.extend_from_slice(&crc32_mpeg(&bytes).to_be_bytes());
        bytes
    }
    #[test]
    fn pat_checks_crc_current_flag_and_every_truncated_prefix() {
        let bytes = section(vec![0, 0, 0, 0, 1, 0xc1, 0, 0, 0, 1, 0xe1, 0]);
        assert_eq!(
            PsiSection::parse(&bytes).unwrap().pat_programs().unwrap(),
            [(1, Pid(0x100))]
        );
        for end in 0..bytes.len() {
            assert!(PsiSection::parse(&bytes[..end]).is_err());
        }
        let mut damaged = bytes.clone();
        damaged[11] ^= 1;
        assert!(matches!(
            PsiSection::parse(&damaged),
            Err(ParseError::Invalid("PSI CRC"))
        ));
        let mut future = bytes[..bytes.len() - CRC_SIZE].to_vec();
        future[5] = 0xc0;
        assert!(PsiSection::parse(&section(future)).is_err());
        assert_eq!(
            section_size(&[0, 0xb3, 0xfe]),
            Err(ParseError::Invalid("PSI section length"))
        );
    }
    #[test]
    fn descriptors_preserve_relationship_and_do_not_confuse_superimpose() {
        let mut pmt = vec![2, 0, 0, 0, 1, 0xc1, 0, 0, 0xe1, 0, 0xf0, 0];
        for (pid, tag, flags, reference) in [
            (0x30, 0x30, 0xff, 0x31),
            (0x31, 0x31, 0xfe, 0x30),
            (0x38, 0x38, 0xff, 0x39),
        ] {
            pmt.extend_from_slice(&[
                6, 0xe1, pid, 0xf0, 12, 0x52, 1, tag, 0xfd, 2, 0, 8, 0xc0, 3, flags, 0xe1,
                reference,
            ]);
        }
        let bytes = section(pmt);
        let parsed = PsiSection::parse(&bytes).unwrap();
        let map = parsed.program_map().unwrap();
        assert_eq!(map.captions.len(), 2);
        assert_eq!(
            map.captions[0].hierarchy,
            Some(Hierarchy {
                layer: TransmissionLayer::High,
                reference: Some(Pid(0x131))
            })
        );
        assert_eq!(
            map.captions[1].hierarchy,
            Some(Hierarchy {
                layer: TransmissionLayer::Low,
                reference: Some(Pid(0x130))
            })
        );
        assert_eq!(
            Descriptors::parse(&[0xc0, 3, 0xfe, 0xff, 0xff])
                .unwrap()
                .hierarchy
                .unwrap()
                .reference,
            None
        );
        for invalid in [
            &[0x52][..],
            &[0x52, 2, 0x30, 0],
            &[0xfd, 1, 0],
            &[0xc0, 3, 0, 0xe1, 0],
            &[0x99, 2, 0],
        ] {
            assert!(Descriptors::parse(invalid).is_err());
        }
        assert!(Descriptors::parse(&[0x99, 2, 0, 0]).is_ok());
    }
    #[test]
    fn malformed_pmt_does_not_return_a_partial_caption_list() {
        let header = [2, 0, 0, 0, 1, 0xc1, 0, 0, 0xe1, 0, 0xf0, 0];
        let caption = [6, 0xe1, 0x30, 0xf0, 7, 0x52, 1, 0x30, 0xfd, 2, 0, 8];
        for tail in [
            vec![6],                                     // Incomplete elementary stream header.
            vec![6, 0xe1, 0x31, 0xf0, 2, 0x52, 1],       // Descriptor exceeds its loop.
            caption.to_vec(),                            // Duplicate PID.
            vec![6, 0xe1, 0x31, 0xf0, 3, 0x52, 1, 0x30], // Duplicate component tag.
        ] {
            let mut bytes = header.to_vec();
            bytes.extend_from_slice(&caption);
            bytes.extend_from_slice(&tail);
            let bytes = section(bytes);
            assert!(PsiSection::parse(&bytes).unwrap().program_map().is_err());
        }
        let mut bytes = header.to_vec();
        bytes[11] = 1; // Program descriptors extend beyond the section body.
        let bytes = section(bytes);
        assert!(PsiSection::parse(&bytes).unwrap().program_map().is_err());
    }
    #[test]
    fn transport_parses_flags_and_rejects_unusable_payloads() {
        let mut packet = [STUFFING_BYTE; TS_PACKET_SIZE];
        packet[..6].copy_from_slice(&[SYNC_BYTE, 0x41, 0x30, 0x3a, 1, 0x80]);
        let parsed = TransportPacket::parse(&packet).unwrap();
        assert_eq!(parsed.pid, Pid(0x130));
        assert!(parsed.start);
        assert!(parsed.discontinuity);
        assert_eq!(parsed.continuity_counter, 10);
        assert_eq!(parsed.payload.len(), 182);
        for (offset, value) in [(0, 0), (1, 0x81), (3, 0xb0), (3, 0), (4, 184), (5, 0x10)] {
            let mut invalid = packet;
            invalid[offset] = value;
            assert!(TransportPacket::parse(&invalid).is_err(), "offset {offset}");
        }
        packet[3] = 0x20;
        packet[4] = 183;
        assert!(TransportPacket::parse(&packet).unwrap().payload.is_empty());
    }

    #[test]
    fn duplicate_comparison_permits_only_pcr_changes() {
        let mut original = [STUFFING_BYTE; TS_PACKET_SIZE];
        // PCR and OPCR are both present, followed by payload at byte 18.
        original[..6].copy_from_slice(&[SYNC_BYTE, 0x41, 0x30, 0x3a, 13, 0x18]);
        assert!(same_payload_packet(&original, &original));
        let mut updated_pcr = original;
        updated_pcr[6..12].copy_from_slice(&[0, 0, 0, 0, 0x7e, 0]);
        assert!(same_payload_packet(&original, &updated_pcr));
        for offset in [1, 3, 4, 5, 12, 18, 187] {
            let mut altered = updated_pcr;
            altered[offset] ^= 1;
            assert!(!same_payload_packet(&original, &altered), "offset {offset}");
        }
        let mut no_adaptation = original;
        no_adaptation[3] = 0x1a;
        assert!(same_payload_packet(&no_adaptation, &no_adaptation));
        let mut changed_payload = no_adaptation;
        changed_payload[6] ^= 1;
        assert!(!same_payload_packet(&no_adaptation, &changed_payload));
    }
}
