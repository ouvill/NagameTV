//! Checked wire representations for ISO/IEC 13818-1 TS/PAT/PMT and ARIB descriptors.
//!
//! ARIB STD-B10 Part 2 §§6.2.18, 6.2.22 and the data component descriptor:
//! https://www.arib.or.jp/english/html/overview/doc/6-STD-B10v5_13-E1.pdf
//! Bit positions below describe big-endian wire words, never native memory layout.
use bitfield::bitfield;

pub(crate) const TS_PACKET_SIZE: usize = 188;
pub(crate) const SYNC_BYTE: u8 = 0x47;
pub(crate) const STUFFING_BYTE: u8 = 0xff;
pub(crate) const PAT_TABLE_ID: u8 = 0x00;
pub(crate) const PMT_TABLE_ID: u8 = 0x02;
const TS_HEADER_SIZE: usize = 4;
const SECTION_PREFIX_SIZE: usize = 3;
const SECTION_HEADER_SIZE: usize = 8;
const CRC_SIZE: usize = 4;
const MAX_PSI_SECTION_LENGTH: usize = 1021;
const PRIVATE_PES_STREAM: u8 = 0x06;
const ARIB_CAPTION_COMPONENT: u16 = 0x0008;
const STREAM_IDENTIFIER_DESCRIPTOR: u8 = 0x52;
const HIERARCHICAL_TRANSMISSION_DESCRIPTOR: u8 = 0xc0;
const DATA_COMPONENT_DESCRIPTOR: u8 = 0xfd;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Pid(pub u16);
impl Pid {
    pub const PAT: Self = Self(0x0000);
    pub const NULL: Self = Self(0x1fff);
}

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
    pub captions: Vec<CaptionStream>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParseError {
    Incomplete,
    Invalid(&'static str),
}

bitfield! {
    struct TsHeader(u32);
    u8, sync, _: 31, 24;
    bool, transport_error, _: 23;
    bool, payload_start, _: 22;
    u16, pid, _: 20, 8;
    u8, scrambling, _: 7, 6;
    u8, adaptation_control, _: 5, 4;
    u8, continuity_counter, _: 3, 0;
}
bitfield! {
    struct AdaptationFlags(u8);
    bool, discontinuity, _: 7;
    bool, pcr, _: 4;
    bool, opcr, _: 3;
    bool, splice, _: 2;
    bool, private_data, _: 1;
    bool, extension, _: 0;
}
bitfield! {
    struct SectionLength(u16);
    bool, syntax, _: 15;
    bool, private, _: 14;
    u8, reserved, _: 13, 12;
    u16, length, _: 11, 0;
}
bitfield! {
    struct SectionVersion(u8);
    u8, reserved, _: 7, 6;
    u8, version, _: 5, 1;
    bool, current, _: 0;
}
bitfield! {
    struct PidField(u16);
    u8, reserved, _: 15, 13;
    u16, pid, _: 12, 0;
}
bitfield! {
    struct LoopLength(u16);
    u8, reserved, _: 15, 12;
    u16, length, _: 11, 0;
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
    fn new(rest: &'a [u8]) -> Self {
        Self {
            rest,
            exhausted: ParseError::Incomplete,
        }
    }
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
    fn descriptor_loop(&mut self) -> Result<&'a [u8], ParseError> {
        let field = LoopLength(self.word()?);
        if field.reserved() != 0b1111 {
            return Err(ParseError::Invalid("loop reserved bits"));
        }
        self.take(usize::from(field.length()))
    }
}

#[derive(Debug)]
pub(crate) struct TransportPacket<'a> {
    pub pid: Pid,
    pub start: bool,
    pub payload: &'a [u8],
    pub continuity_counter: u8,
    pub discontinuity: bool,
    pub pcr: Option<u64>,
}
impl<'a> TransportPacket<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, ParseError> {
        if bytes.len() < TS_PACKET_SIZE {
            return Err(ParseError::Incomplete);
        }
        if bytes.len() != TS_PACKET_SIZE {
            return Err(ParseError::Invalid("TS packet size"));
        }
        let mut cursor = Cursor::bounded(bytes);
        let header_bytes = cursor.take(TS_HEADER_SIZE)?;
        let header = TsHeader(u32::from_be_bytes(
            header_bytes
                .try_into()
                .map_err(|_| ParseError::Incomplete)?,
        ));
        if header.sync() != SYNC_BYTE {
            return Err(ParseError::Invalid("TS sync"));
        }
        if header.transport_error() {
            return Err(ParseError::Invalid("transport error"));
        }
        if header.scrambling() != 0 {
            return Err(ParseError::Invalid("scrambled TS payload"));
        }
        let (has_adaptation, has_payload) = match header.adaptation_control() {
            1 => (false, true),
            2 => (true, false),
            3 => (true, true),
            _ => return Err(ParseError::Invalid("reserved adaptation control")),
        };
        let mut discontinuity = false;
        let mut pcr = None;
        if has_adaptation {
            let length = usize::from(cursor.byte()?);
            let mut adaptation = Cursor::bounded(
                cursor
                    .take(length)
                    .map_err(|_| ParseError::Invalid("adaptation length"))?,
            );
            if length > 0 {
                let flags = AdaptationFlags(adaptation.byte()?);
                discontinuity = flags.discontinuity();
                // Validate flagged fields even though only discontinuity is needed.
                if flags.pcr() {
                    let value = adaptation.take(6)?;
                    pcr = Some(
                        (u64::from(value[0]) << 25)
                            | (u64::from(value[1]) << 17)
                            | (u64::from(value[2]) << 9)
                            | (u64::from(value[3]) << 1)
                            | u64::from(value[4] >> 7),
                    );
                }
                if flags.opcr() {
                    adaptation.take(6)?;
                }
                if flags.splice() {
                    adaptation.take(1)?;
                }
                if flags.private_data() {
                    let n = usize::from(adaptation.byte()?);
                    adaptation.take(n)?;
                }
                if flags.extension() {
                    let n = usize::from(adaptation.byte()?);
                    adaptation.take(n)?;
                }
            }
            if has_payload == cursor.rest.is_empty() {
                return Err(ParseError::Invalid("adaptation/payload size"));
            }
        }
        Ok(Self {
            pid: Pid(header.pid()),
            start: header.payload_start(),
            payload: if has_payload { cursor.rest } else { &[] },
            continuity_counter: header.continuity_counter(),
            discontinuity,
            pcr,
        })
    }
}

/// H.222.0 §2.4.3.3 allows a duplicate payload packet to carry an updated PCR.
/// Every other byte, including adaptation flags and OPCR, must remain identical.
/// https://www.itu.int/rec/T-REC-H.222.0/en
pub(crate) fn same_payload_packet(
    previous: &[u8; TS_PACKET_SIZE],
    current: &[u8; TS_PACKET_SIZE],
) -> bool {
    let Ok(packet) = TransportPacket::parse(current) else {
        return false;
    };
    if packet.payload.is_empty() {
        return false;
    }
    let header = TsHeader(u32::from_be_bytes([
        current[0], current[1], current[2], current[3],
    ]));
    const ADAPTATION_AND_PAYLOAD: u8 = 3;
    const ADAPTATION_LENGTH_SIZE: usize = 1;
    const ADAPTATION_FLAGS_SIZE: usize = 1;
    const PCR_SIZE: usize = 6;
    const FLAGS_OFFSET: usize = TS_HEADER_SIZE + ADAPTATION_LENGTH_SIZE;
    const PCR_OFFSET: usize = FLAGS_OFFSET + ADAPTATION_FLAGS_SIZE;
    if header.adaptation_control() == ADAPTATION_AND_PAYLOAD
        && current[TS_HEADER_SIZE] > 0
        && AdaptationFlags(current[FLAGS_OFFSET]).pcr()
    {
        // Packet validation above guarantees that the entire PCR is present.
        previous[..PCR_OFFSET] == current[..PCR_OFFSET]
            && previous[PCR_OFFSET + PCR_SIZE..] == current[PCR_OFFSET + PCR_SIZE..]
    } else {
        previous == current
    }
}

/// Required total size from the section prefix; does not require its body yet.
pub(crate) fn section_size(bytes: &[u8]) -> Result<usize, ParseError> {
    let mut cursor = Cursor::new(bytes);
    cursor.byte()?;
    let header = SectionLength(cursor.word()?);
    let length = usize::from(header.length());
    if !header.syntax() || header.private() || header.reserved() != 0b11 {
        return Err(ParseError::Invalid("PAT/PMT section syntax"));
    }
    if !(SECTION_HEADER_SIZE - SECTION_PREFIX_SIZE + CRC_SIZE..=MAX_PSI_SECTION_LENGTH)
        .contains(&length)
    {
        return Err(ParseError::Invalid("PSI section length"));
    }
    Ok(SECTION_PREFIX_SIZE + length)
}

#[derive(Debug)]
pub(crate) struct PsiSection<'a> {
    pub table_id: u8,
    pub extension: u16,
    pub version: u8,
    pub section_number: u8,
    pub last_section_number: u8,
    body: &'a [u8],
}
impl<'a> PsiSection<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, ParseError> {
        let size = section_size(bytes)?;
        if bytes.len() < size {
            return Err(ParseError::Incomplete);
        }
        if bytes.len() != size {
            return Err(ParseError::Invalid("trailing section bytes"));
        }
        if crc32_mpeg(bytes) != 0 {
            return Err(ParseError::Invalid("PSI CRC"));
        }
        let mut cursor = Cursor::bounded(bytes);
        let table_id = cursor.byte()?;
        cursor.take(2)?;
        let extension = cursor.word()?;
        let version = SectionVersion(cursor.byte()?);
        if version.reserved() != 0b11 || !version.current() {
            return Err(ParseError::Invalid("non-current PSI section"));
        }
        let section_number = cursor.byte()?;
        let last_section_number = cursor.byte()?;
        if section_number > last_section_number {
            return Err(ParseError::Invalid("section numbering"));
        }
        Ok(Self {
            table_id,
            extension,
            version: version.version(),
            section_number,
            last_section_number,
            body: &bytes[SECTION_HEADER_SIZE..size - CRC_SIZE],
        })
    }
    pub fn pat_programs(&self) -> Result<Vec<(u16, Pid)>, ParseError> {
        if self.table_id != PAT_TABLE_ID {
            return Err(ParseError::Invalid("not a PAT"));
        }
        let mut cursor = Cursor::bounded(self.body);
        let mut programs = Vec::new();
        while !cursor.rest.is_empty() {
            let service = cursor.word()?;
            let pid = cursor.pid()?;
            // Program zero points to the network table, not a program map.
            if service != 0 {
                if pid == Pid::PAT || pid == Pid::NULL {
                    return Err(ParseError::Invalid("program map PID"));
                }
                programs.push((service, pid));
            }
        }
        Ok(programs)
    }
    pub fn program_map(&self) -> Result<ProgramMap, ParseError> {
        if self.table_id != PMT_TABLE_ID
            || self.section_number != 0
            || self.last_section_number != 0
        {
            return Err(ParseError::Invalid("not a single-section PMT"));
        }
        let mut cursor = Cursor::bounded(self.body);
        let pcr_pid = cursor.pid()?; // PCR_PID may be the null PID.
        Descriptors::parse(cursor.descriptor_loop()?)?;
        let mut captions = Vec::new();
        let mut seen_pids = std::collections::HashSet::new();
        let mut seen_component_tags = std::collections::HashSet::new();
        while !cursor.rest.is_empty() {
            let stream_type = cursor.byte()?;
            let pid = cursor.pid()?;
            if pid == Pid::PAT || pid == Pid::NULL || !seen_pids.insert(pid) {
                return Err(ParseError::Invalid("elementary stream PID"));
            }
            let descriptors = Descriptors::parse(cursor.descriptor_loop()?)?;
            if let Some(tag) = descriptors.component_tag
                && !seen_component_tags.insert(tag)
            {
                return Err(ParseError::Invalid("duplicate component tag"));
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
            service: self.extension,
            pcr_pid,
            captions,
        })
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

/// ISO/IEC 13818-1 CRC-32: initial all ones, no reflection or final XOR.
pub(super) fn crc32_mpeg(bytes: &[u8]) -> u32 {
    const POLYNOMIAL: u32 = 0x04c1_1db7;
    bytes.iter().fold(u32::MAX, |mut crc, byte| {
        crc ^= u32::from(*byte) << 24;
        for _ in 0..u8::BITS {
            crc = if crc & (1 << 31) != 0 {
                (crc << 1) ^ POLYNOMIAL
            } else {
                crc << 1
            };
        }
        crc
    })
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
