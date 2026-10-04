//! ISO/IEC 13818-1 / ITU-T H.222.0 TS packets and program-specific information.
//! No ARIB data-coding or presentation policy lives here.

use bitfield::bitfield;
use std::collections::BTreeMap;

pub const TS_PACKET_SIZE: usize = 188;
pub const SYNC_BYTE: u8 = 0x47;
pub const STUFFING_BYTE: u8 = 0xff;
pub const PAT_TABLE_ID: u8 = 0x00;
pub const PMT_TABLE_ID: u8 = 0x02;
pub const SECTION_PREFIX_SIZE: usize = 3;
pub const MAX_PSI_SECTION_LENGTH: usize = 1021;
pub const MAX_PRIVATE_SECTION_LENGTH: usize = 4093;
const SECTION_HEADER_SIZE: usize = 8;
const CRC_SIZE: usize = 4;
const TS_HEADER_SIZE: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pid(pub u16);
impl Pid {
    pub const PAT: Self = Self(0);
    pub const NULL: Self = Self(0x1fff);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("incomplete input")]
    Incomplete,
    #[error("invalid {0}")]
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

#[derive(Debug)]
pub struct TransportPacket<'a> {
    pub pid: Pid,
    pub start: bool,
    pub payload: &'a [u8],
    pub continuity_counter: u8,
    pub discontinuity: bool,
    pub pcr: Option<u64>,
    pcr_extension: Option<u16>,
}

impl<'a> TransportPacket<'a> {
    /// The 27 MHz remainder accompanying the 90 kHz PCR base.
    pub fn pcr_extension(&self) -> Option<u16> {
        self.pcr_extension
    }

    pub fn parse(bytes: &'a [u8]) -> Result<Self, ParseError> {
        if bytes.len() < TS_PACKET_SIZE {
            return Err(ParseError::Incomplete);
        }
        if bytes.len() != TS_PACKET_SIZE {
            return Err(ParseError::Invalid("TS packet size"));
        }
        let header = TsHeader(u32::from_be_bytes(
            bytes[..4].try_into().map_err(|_| ParseError::Incomplete)?,
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
        let mut offset = TS_HEADER_SIZE;
        let mut discontinuity = false;
        let mut pcr = None;
        let mut pcr_extension = None;
        if has_adaptation {
            let length = usize::from(bytes[offset]);
            offset += 1;
            let adaptation = bytes
                .get(offset..offset + length)
                .ok_or(ParseError::Invalid("adaptation length"))?;
            let mut fields = Cursor::bounded(adaptation);
            if length > 0 {
                let flags = AdaptationFlags(fields.byte()?);
                discontinuity = flags.discontinuity();
                if flags.pcr() {
                    let value = fields.take(6)?;
                    pcr_extension = Some((u16::from(value[4] & 1) << 8) | u16::from(value[5]));
                    pcr = Some(
                        (u64::from(value[0]) << 25)
                            | (u64::from(value[1]) << 17)
                            | (u64::from(value[2]) << 9)
                            | (u64::from(value[3]) << 1)
                            | u64::from(value[4] >> 7),
                    );
                }
                if flags.opcr() {
                    fields.take(6)?;
                }
                if flags.splice() {
                    fields.take(1)?;
                }
                if flags.private_data() {
                    let n = usize::from(fields.byte()?);
                    fields.take(n)?;
                }
                if flags.extension() {
                    let n = usize::from(fields.byte()?);
                    fields.take(n)?;
                }
            }
            offset += length;
            if has_payload == (offset == TS_PACKET_SIZE) {
                return Err(ParseError::Invalid("adaptation/payload size"));
            }
        }
        Ok(Self {
            pid: Pid(header.pid()),
            start: header.payload_start(),
            payload: if has_payload { &bytes[offset..] } else { &[] },
            continuity_counter: header.continuity_counter(),
            discontinuity,
            pcr,
            pcr_extension,
        })
    }
}

/// Duplicate payload packets may differ only in PCR bytes.
pub fn same_payload_packet(
    previous: &[u8; TS_PACKET_SIZE],
    current: &[u8; TS_PACKET_SIZE],
) -> bool {
    let Ok(packet) = TransportPacket::parse(current) else {
        return false;
    };
    if packet.payload.is_empty() {
        return false;
    }
    const FLAGS_OFFSET: usize = TS_HEADER_SIZE + 1;
    const PCR_OFFSET: usize = FLAGS_OFFSET + 1;
    const PCR_SIZE: usize = 6;
    let header = TsHeader(u32::from_be_bytes(
        current[..4].try_into().expect("four bytes"),
    ));
    if header.adaptation_control() == 3
        && current[TS_HEADER_SIZE] > 0
        && AdaptationFlags(current[FLAGS_OFFSET]).pcr()
    {
        previous[..PCR_OFFSET] == current[..PCR_OFFSET]
            && previous[PCR_OFFSET + PCR_SIZE..] == current[PCR_OFFSET + PCR_SIZE..]
    } else {
        previous == current
    }
}

pub fn crc32_mpeg(bytes: &[u8]) -> u32 {
    const POLYNOMIAL: u32 = 0x04c1_1db7;
    bytes.iter().fold(u32::MAX, |mut crc, byte| {
        crc ^= u32::from(*byte) << 24;
        for _ in 0..8 {
            crc = (crc << 1)
                ^ if crc & 0x8000_0000 != 0 {
                    POLYNOMIAL
                } else {
                    0
                };
        }
        crc
    })
}

pub fn section_size(bytes: &[u8]) -> Result<usize, ParseError> {
    let mut cursor = Cursor::new(bytes);
    cursor.byte()?;
    let field = cursor.word()?;
    let length = usize::from(field & 0x0fff);
    if field & 0xf000 != 0xb000 {
        return Err(ParseError::Invalid("PAT/PMT section syntax"));
    }
    if !(SECTION_HEADER_SIZE - SECTION_PREFIX_SIZE + CRC_SIZE..=MAX_PSI_SECTION_LENGTH)
        .contains(&length)
    {
        return Err(ParseError::Invalid("PSI section length"));
    }
    Ok(SECTION_PREFIX_SIZE + length)
}

pub fn private_section_size(bytes: &[u8]) -> Result<usize, ParseError> {
    if bytes.len() < SECTION_PREFIX_SIZE {
        return Err(ParseError::Incomplete);
    }
    if bytes[1] & 0x30 != 0x30 {
        return Err(ParseError::Invalid("private section reserved bits"));
    }
    let length = usize::from(u16::from_be_bytes([bytes[1] & 15, bytes[2]]));
    if !(9..=MAX_PRIVATE_SECTION_LENGTH).contains(&length) {
        return Err(ParseError::Invalid("private section length"));
    }
    Ok(SECTION_PREFIX_SIZE + length)
}

#[derive(Clone, Copy)]
pub enum SectionKind {
    Psi,
    Private,
    Si,
}
impl SectionKind {
    fn size(self, bytes: &[u8]) -> Result<usize, ParseError> {
        match self {
            Self::Psi => section_size(bytes),
            Self::Private => private_section_size(bytes),
            Self::Si => si_section_size(bytes),
        }
    }
}

fn si_section_size(bytes: &[u8]) -> Result<usize, ParseError> {
    if bytes.len() < SECTION_PREFIX_SIZE {
        return Err(ParseError::Incomplete);
    }
    if bytes[1] & 0x30 != 0x30 {
        return Err(ParseError::Invalid("SI reserved bits"));
    }
    let length = usize::from(u16::from_be_bytes([bytes[1] & 15, bytes[2]]));
    let valid = match bytes[0] {
        0x42 | 0x4e => {
            bytes[1] & 0xc0 == 0xc0
                && (9..=if bytes[0] == 0x4e { 4093 } else { 1021 }).contains(&length)
        }
        0x70 => bytes[1] & 0xc0 == 0x40 && length == 5,
        0x73 => bytes[1] & 0xc0 == 0x40 && (11..=1021).contains(&length),
        _ => false,
    };
    if !valid {
        return Err(ParseError::Invalid("SI table syntax/length"));
    }
    Ok(length + SECTION_PREFIX_SIZE)
}

pub struct Sections {
    pending: Option<Vec<u8>>,
    kind: SectionKind,
}
impl Default for Sections {
    fn default() -> Self {
        Self::new(SectionKind::Psi)
    }
}
impl Sections {
    pub fn new(kind: SectionKind) -> Self {
        Self {
            pending: None,
            kind,
        }
    }
    pub fn si() -> Self {
        Self::new(SectionKind::Si)
    }
    pub fn is_empty(&self) -> bool {
        self.pending.is_none()
    }
    pub fn pending_len(&self) -> Option<usize> {
        self.pending.as_ref().map(Vec::len)
    }
    pub fn push(&mut self, start: bool, payload: &[u8]) -> Vec<Vec<u8>> {
        let mut complete = Vec::new();
        if payload.is_empty() {
            return complete;
        }
        if start {
            let pointer = usize::from(payload[0]);
            let Some((prefix, sections)) = payload[1..].split_at_checked(pointer) else {
                self.pending = None;
                return complete;
            };
            if sections.is_empty() {
                self.pending = None;
                return complete;
            }
            complete.extend(self.finish_pending(prefix));
            self.pending = None;
            self.start_sections(sections, &mut complete);
        } else {
            complete.extend(self.finish_pending(payload));
        }
        complete
    }
    fn finish_pending(&mut self, bytes: &[u8]) -> Option<Vec<u8>> {
        let mut data = self.pending.take()?;
        let prefix = SECTION_PREFIX_SIZE
            .saturating_sub(data.len())
            .min(bytes.len());
        data.extend_from_slice(&bytes[..prefix]);
        let bytes = &bytes[prefix..];
        match self.kind.size(&data) {
            Ok(size) => {
                let remaining = size - data.len();
                data.reserve_exact(remaining);
                data.extend_from_slice(&bytes[..remaining.min(bytes.len())]);
                if data.len() == size {
                    return Some(data);
                }
                self.pending = Some(data);
            }
            Err(ParseError::Incomplete) => self.pending = Some(data),
            Err(ParseError::Invalid(_)) => {}
        }
        None
    }
    fn start_sections(&mut self, mut bytes: &[u8], complete: &mut Vec<Vec<u8>>) {
        while !bytes.is_empty() && bytes[0] != STUFFING_BYTE {
            match self.kind.size(bytes) {
                Ok(size) if bytes.len() >= size => {
                    complete.push(bytes[..size].to_vec());
                    bytes = &bytes[size..];
                }
                Ok(size) => {
                    self.pending = Some(bytes.to_vec());
                    self.pending
                        .as_mut()
                        .expect("pending set")
                        .reserve_exact(size - bytes.len());
                    break;
                }
                Err(ParseError::Incomplete) => {
                    self.pending = Some(bytes.to_vec());
                    break;
                }
                Err(ParseError::Invalid(_)) => break,
            }
        }
    }
}

pub struct Cursor<'a> {
    rest: &'a [u8],
    exhausted: ParseError,
}
impl<'a> Cursor<'a> {
    pub fn new(rest: &'a [u8]) -> Self {
        Self {
            rest,
            exhausted: ParseError::Incomplete,
        }
    }
    pub fn bounded(rest: &'a [u8]) -> Self {
        Self {
            rest,
            exhausted: ParseError::Invalid("field exceeds declared length"),
        }
    }
    pub fn take(&mut self, length: usize) -> Result<&'a [u8], ParseError> {
        let (value, rest) = self.rest.split_at_checked(length).ok_or(self.exhausted)?;
        self.rest = rest;
        Ok(value)
    }
    pub fn byte(&mut self) -> Result<u8, ParseError> {
        Ok(self.take(1)?[0])
    }
    pub fn word(&mut self) -> Result<u16, ParseError> {
        let value = self.take(2)?;
        Ok(u16::from_be_bytes([value[0], value[1]]))
    }
    pub fn pid(&mut self) -> Result<Pid, ParseError> {
        let field = self.word()?;
        if field & 0xe000 != 0xe000 {
            return Err(ParseError::Invalid("PID reserved bits"));
        }
        Ok(Pid(field & 0x1fff))
    }
    pub fn descriptor_loop(&mut self) -> Result<&'a [u8], ParseError> {
        let field = self.word()?;
        if field & 0xf000 != 0xf000 {
            return Err(ParseError::Invalid("loop reserved bits"));
        }
        self.take(usize::from(field & 0x0fff))
    }
    pub fn rest(&self) -> &'a [u8] {
        self.rest
    }
}

#[derive(Debug)]
pub struct PsiSection<'a> {
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
        let version = cursor.byte()?;
        if version & 0xc1 != 0xc1 {
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
            version: (version >> 1) & 0x1f,
            section_number,
            last_section_number,
            body: &bytes[SECTION_HEADER_SIZE..size - CRC_SIZE],
        })
    }
    pub fn body(&self) -> &'a [u8] {
        self.body
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
            if service != 0 {
                if pid == Pid::PAT || pid == Pid::NULL {
                    return Err(ParseError::Invalid("program map PID"));
                }
                programs.push((service, pid));
            }
        }
        Ok(programs)
    }
    pub fn program_map(&self) -> Result<ProgramMap<'a>, ParseError> {
        if self.table_id != PMT_TABLE_ID
            || self.section_number != 0
            || self.last_section_number != 0
        {
            return Err(ParseError::Invalid("not a single-section PMT"));
        }
        let mut cursor = Cursor::bounded(self.body);
        let pcr_pid = cursor.pid()?;
        let program_descriptors = cursor.descriptor_loop()?;
        validate_descriptors(program_descriptors)?;
        let mut streams = Vec::new();
        let mut seen = std::collections::HashSet::new();
        while !cursor.rest.is_empty() {
            let stream_type = cursor.byte()?;
            let pid = cursor.pid()?;
            if pid == Pid::PAT || pid == Pid::NULL || !seen.insert(pid) {
                return Err(ParseError::Invalid("elementary stream PID"));
            }
            let descriptors = cursor.descriptor_loop()?;
            validate_descriptors(descriptors)?;
            streams.push(ElementaryStream {
                stream_type,
                pid,
                descriptors,
            });
        }
        Ok(ProgramMap {
            service: self.extension,
            pcr_pid,
            program_descriptors,
            streams,
        })
    }
}

fn validate_descriptors(mut bytes: &[u8]) -> Result<(), ParseError> {
    while !bytes.is_empty() {
        if bytes.len() < 2 {
            return Err(ParseError::Invalid("descriptor length"));
        }
        let length = usize::from(bytes[1]);
        bytes = bytes
            .get(2 + length..)
            .ok_or(ParseError::Invalid("descriptor length"))?;
    }
    Ok(())
}

pub struct ProgramMap<'a> {
    pub service: u16,
    pub pcr_pid: Pid,
    pub program_descriptors: &'a [u8],
    pub streams: Vec<ElementaryStream<'a>>,
}
pub struct ElementaryStream<'a> {
    pub stream_type: u8,
    pub pid: Pid,
    pub descriptors: &'a [u8],
}

#[derive(Default)]
pub struct Pat {
    identity: Option<(u16, u8, u8)>,
    sections: BTreeMap<u8, Vec<(u16, Pid)>>,
}
impl Pat {
    pub fn push(&mut self, section: &PsiSection<'_>) -> Option<BTreeMap<u16, Pid>> {
        let programs = section.pat_programs().ok()?;
        let identity = (
            section.extension,
            section.version,
            section.last_section_number,
        );
        if self.identity != Some(identity) {
            self.identity = Some(identity);
            self.sections.clear();
        }
        self.sections.insert(section.section_number, programs);
        if self.sections.len() != usize::from(section.last_section_number) + 1 {
            return None;
        }
        let mut programs = BTreeMap::new();
        for &(service, pid) in self.sections.values().flatten() {
            if programs.insert(service, pid).is_some() {
                return None;
            }
        }
        Some(programs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_crc(mut section: Vec<u8>) -> Vec<u8> {
        let crc = crc32_mpeg(&section);
        section.extend_from_slice(&crc.to_be_bytes());
        section
    }

    #[test]
    fn pcr_keeps_the_27mhz_extension_and_33bit_base() {
        let mut bytes = [0xff; TS_PACKET_SIZE];
        bytes[..6].copy_from_slice(&[0x47, 0, 0x20, 0x20, 183, 0x10]);
        let base = (1_u64 << 33) - 17;
        let extension = 299_u16;
        bytes[6..12].copy_from_slice(&[
            (base >> 25) as u8,
            (base >> 17) as u8,
            (base >> 9) as u8,
            (base >> 1) as u8,
            ((base & 1) << 7) as u8 | 0x7e | (extension >> 8) as u8,
            extension as u8,
        ]);
        let packet = TransportPacket::parse(&bytes).unwrap();
        assert_eq!(packet.pcr, Some(base));
        assert_eq!(packet.pcr_extension(), Some(extension));
    }

    #[test]
    fn pat_and_pmt_share_checked_section_parser() {
        let pat = with_crc(vec![0, 0xb0, 13, 0, 1, 0xc1, 0, 0, 0, 42, 0xe1, 0]);
        let parsed = PsiSection::parse(&pat).unwrap();
        assert_eq!(parsed.pat_programs().unwrap(), [(42, Pid(0x100))]);
        let mut collector = Pat::default();
        assert_eq!(collector.push(&parsed).unwrap().get(&42), Some(&Pid(0x100)));

        let pmt = with_crc(vec![
            2, 0xb0, 18, 0, 42, 0xc1, 0, 0, 0xe1, 1, 0xf0, 0, 0x0d, 0xe1, 2, 0xf0, 0,
        ]);
        let map = PsiSection::parse(&pmt).unwrap().program_map().unwrap();
        assert_eq!(map.service, 42);
        assert_eq!(map.pcr_pid, Pid(0x101));
        assert_eq!(map.streams[0].pid, Pid(0x102));
        let mut damaged = pmt;
        damaged[13] ^= 1;
        assert_eq!(
            PsiSection::parse(&damaged).unwrap_err(),
            ParseError::Invalid("PSI CRC")
        );
    }

    #[test]
    fn private_section_crosses_packet_boundary() {
        let section = with_crc(vec![0x3b, 0x30, 9, 0, 0, 0, 0, 0]);
        let mut sections = Sections::new(SectionKind::Private);
        assert!(sections.push(true, &[0, section[0], section[1]]).is_empty());
        assert_eq!(sections.push(false, &section[2..]), vec![section]);
    }
}
