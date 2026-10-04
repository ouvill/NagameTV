//! ARIB STD-B24 Part 3, Chapter 6 data-carousel decoder.
//!
//! Feed complete DSM-CC sections from a TS section assembler to [`Section::parse`],
//! then pass the messages to [`Carousel`]. [`transport::TsReceiver`] accepts
//! 188-byte TS packets, and [`Module::decode`] expands zlib modules.
//! BML interpretation and display are separate operations.
//! Specification: <https://www.arib.or.jp/english/html/overview/doc/6-STD-B24v5_1-3p3-E2.pdf>

use std::collections::BTreeMap;
use thiserror::Error;

mod compression;
pub mod event;
pub mod resource;
pub mod transport;

const DII_TABLE: u8 = 0x3b;
const DDB_TABLE: u8 = 0x3c;
const DSMCC_PROTOCOL: u8 = 0x11;
const DOWNLOAD_TYPE: u8 = 0x03;
const DII_MESSAGE: u16 = 0x1002;
const DDB_MESSAGE: u16 = 0x1003;
const MAX_SECTION_LENGTH: usize = 4093;
const SECTION_HEADER_LENGTH: usize = 8;
const CRC_LENGTH: usize = 4;
/// STD-B24 Part 3 §6.2.3.4 describes 256 MiB as one module's maximum size.
pub const STANDARD_MAX_MODULE_BYTES: usize = 256 * 1024 * 1024;
const DEFAULT_MAX_LINKED_BYTES: usize = 1024 * 1024 * 1024;

/// Bounds for encoded modules, decoded modules, and assembled linked files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodeLimits {
    max_module_bytes: usize,
    max_decoded_bytes: usize,
    max_linked_bytes: usize,
}

impl DecodeLimits {
    pub fn new(max_module_bytes: usize, max_decoded_bytes: usize) -> Result<Self, Error> {
        if max_module_bytes == 0 || max_decoded_bytes == 0 {
            return Err(Error::Invalid("zero decoder limit"));
        }
        if max_module_bytes > STANDARD_MAX_MODULE_BYTES {
            return Err(Error::Invalid("module limit exceeds standard maximum"));
        }
        Ok(Self {
            max_module_bytes,
            max_decoded_bytes,
            max_linked_bytes: max_decoded_bytes,
        })
    }

    /// Sets the total bound for a file assembled from linked modules.
    pub fn with_linked_bytes(mut self, max_linked_bytes: usize) -> Result<Self, Error> {
        if max_linked_bytes == 0 {
            return Err(Error::Invalid("zero linked file limit"));
        }
        self.max_linked_bytes = max_linked_bytes;
        Ok(self)
    }

    pub fn max_module_bytes(self) -> usize {
        self.max_module_bytes
    }
    pub fn max_decoded_bytes(self) -> usize {
        self.max_decoded_bytes
    }
    pub fn max_linked_bytes(self) -> usize {
        self.max_linked_bytes
    }
}

impl Default for DecodeLimits {
    fn default() -> Self {
        Self {
            max_module_bytes: STANDARD_MAX_MODULE_BYTES,
            max_decoded_bytes: STANDARD_MAX_MODULE_BYTES,
            max_linked_bytes: DEFAULT_MAX_LINKED_BYTES,
        }
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("truncated {0}")]
    Truncated(&'static str),
    #[error("invalid {0}")]
    Invalid(&'static str),
    #[error("module size {actual} bytes exceeds configured limit of {limit} bytes")]
    ModuleTooLarge { actual: usize, limit: usize },
    #[error("decoded size {actual} bytes exceeds configured limit of {limit} bytes")]
    DecodedTooLarge { actual: usize, limit: usize },
    #[error("linked file size {actual} bytes exceeds configured limit of {limit} bytes")]
    LinkedFileTooLarge { actual: usize, limit: usize },
    #[error("allocating module storage: {0}")]
    Allocation(#[source] std::collections::TryReserveError),
    #[error("unsupported module compression type {0:#04x}")]
    UnsupportedCompression(u8),
    #[error("linked module fragment needs the rest of its chain")]
    LinkedModuleFragment,
    #[error("decompressing module: {0}")]
    Decompression(#[source] std::io::Error),
}

struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }
    fn take(&mut self, count: usize) -> Result<&'a [u8], Error> {
        let (value, rest) = self
            .bytes
            .split_at_checked(count)
            .ok_or(Error::Truncated("field"))?;
        self.bytes = rest;
        Ok(value)
    }
    fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, Error> {
        Ok(u16::from_be_bytes(
            self.take(2)?
                .try_into()
                .map_err(|_| Error::Truncated("u16"))?,
        ))
    }
    fn u32(&mut self) -> Result<u32, Error> {
        Ok(u32::from_be_bytes(
            self.take(4)?
                .try_into()
                .map_err(|_| Error::Truncated("u32"))?,
        ))
    }
    fn finish(&self) -> Result<(), Error> {
        if self.bytes.is_empty() {
            Ok(())
        } else {
            Err(Error::Invalid("trailing bytes"))
        }
    }
}

use viewer_mpegts::crc32_mpeg as crc32;

/// A descriptor's uninterpreted payload, including broadcaster-defined tags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Descriptor {
    pub tag: u8,
    pub data: Vec<u8>,
}

impl Descriptor {
    /// Module type bytes (usually an ASCII MIME type).
    pub fn media_type(&self) -> Option<&[u8]> {
        (self.tag == 0x01).then_some(&self.data)
    }
    /// Module name bytes; character coding depends on the data service.
    pub fn name(&self) -> Option<&[u8]> {
        (self.tag == 0x02).then_some(&self.data)
    }
}

fn descriptors(bytes: &[u8]) -> Result<Vec<Descriptor>, Error> {
    let mut reader = Reader::new(bytes);
    let mut result = Vec::new();
    while !reader.bytes.is_empty() {
        let tag = reader.u8()?;
        let length = usize::from(reader.u8()?);
        result.push(Descriptor {
            tag,
            data: reader.take(length)?.to_vec(),
        });
    }
    Ok(result)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleInfo {
    pub id: u16,
    pub size: u32,
    pub version: u8,
    pub descriptors: Vec<Descriptor>,
}

/// STD-B24 Part 3 §6.2.3.4: position in a file split across modules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleLink {
    Head { next: u16 },
    Middle { next: u16 },
    Last,
}

impl ModuleInfo {
    pub fn module_link(&self) -> Result<Option<ModuleLink>, Error> {
        let mut links = self.descriptors.iter().filter(|item| item.tag == 0x04);
        let Some(descriptor) = links.next() else {
            return Ok(None);
        };
        if links.next().is_some() || descriptor.data.len() != 3 {
            return Err(Error::Invalid("module link descriptor"));
        }
        let next = u16::from_be_bytes([descriptor.data[1], descriptor.data[2]]);
        match descriptor.data[0] {
            0 if next != self.id => Ok(Some(ModuleLink::Head { next })),
            1 if next != self.id => Ok(Some(ModuleLink::Middle { next })),
            2 => Ok(Some(ModuleLink::Last)),
            _ => Err(Error::Invalid("module link position or cycle")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadInfo {
    pub transaction_id: u32,
    pub download_id: u32,
    pub block_size: u16,
    pub modules: Vec<ModuleInfo>,
    pub descriptors: Vec<Descriptor>,
}

impl DownloadInfo {
    /// STD-B24 Volume 2, Part 2 §9.3.4: request entry-document invocation
    /// when this component's data event changes, even from another component.
    pub fn bxml_return_to_entry(&self) -> Result<Option<bool>, Error> {
        const BXML_PRIVATE_DATA: u8 = 0xf0;
        let mut descriptors = self
            .descriptors
            .iter()
            .filter(|item| item.tag == BXML_PRIVATE_DATA);
        let Some(descriptor) = descriptors.next() else {
            return Ok(None);
        };
        if descriptor.data.len() != 1 || descriptors.next().is_some() {
            return Err(Error::Invalid("BXML private data descriptor"));
        }
        Ok(Some(descriptor.data[0] & 0x80 != 0))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadBlock {
    pub download_id: u32,
    pub module_id: u16,
    pub module_version: u8,
    pub block_number: u16,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Section {
    Info(DownloadInfo),
    Block(DownloadBlock),
}

impl Section {
    /// Parses one complete section including its MPEG-2 CRC-32.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() < SECTION_HEADER_LENGTH + CRC_LENGTH {
            return Err(Error::Truncated("DSM-CC section"));
        }
        let table = bytes[0];
        if !matches!(table, DII_TABLE | DDB_TABLE) {
            return Err(Error::Invalid("DSM-CC table ID"));
        }
        if bytes[1] & 0xf0 != 0xb0 {
            return Err(Error::Invalid("section flags"));
        }
        let length = usize::from(u16::from_be_bytes([bytes[1] & 0x0f, bytes[2]]));
        if !(SECTION_HEADER_LENGTH + CRC_LENGTH - 3..=MAX_SECTION_LENGTH).contains(&length) {
            return Err(Error::Invalid("section length"));
        }
        if bytes.len() != length + 3 {
            return Err(Error::Invalid("section size"));
        }
        if bytes[5] & 0xc1 != 0xc1 {
            return Err(Error::Invalid("section version flags"));
        }
        if crc32(bytes) != 0 {
            return Err(Error::Invalid("section CRC-32"));
        }
        let extension = u16::from_be_bytes([bytes[3], bytes[4]]);
        let section_number = bytes[6];
        let payload = &bytes[SECTION_HEADER_LENGTH..bytes.len() - CRC_LENGTH];
        let (identity, body) = parse_message(
            payload,
            if table == DII_TABLE {
                DII_MESSAGE
            } else {
                DDB_MESSAGE
            },
        )?;
        match table {
            DII_TABLE => {
                if extension != identity as u16
                    || section_number != 0
                    || (bytes[5] >> 1) & 0x1f != 0
                {
                    return Err(Error::Invalid("DII section header"));
                }
                Ok(Self::Info(parse_dii(identity, body)?))
            }
            DDB_TABLE => {
                let block = parse_ddb(identity, body)?;
                if extension != block.module_id
                    || section_number != block.block_number as u8
                    || (bytes[5] >> 1) & 0x1f != block.module_version & 0x1f
                {
                    return Err(Error::Invalid("DDB section header"));
                }
                Ok(Self::Block(block))
            }
            _ => unreachable!(),
        }
    }
}

fn parse_message(payload: &[u8], expected: u16) -> Result<(u32, &[u8]), Error> {
    let mut reader = Reader::new(payload);
    if reader.u8()? != DSMCC_PROTOCOL || reader.u8()? != DOWNLOAD_TYPE || reader.u16()? != expected
    {
        return Err(Error::Invalid("DSM-CC message type"));
    }
    let identity = reader.u32()?;
    if reader.u8()? != 0xff {
        return Err(Error::Invalid("DSM-CC reserved byte"));
    }
    let adaptation_length = usize::from(reader.u8()?);
    let message_length = usize::from(reader.u16()?);
    if message_length < adaptation_length {
        return Err(Error::Invalid("message length"));
    }
    let mut message = Reader::new(reader.take(message_length)?);
    message.take(adaptation_length)?;
    reader.finish()?;
    Ok((identity, message.bytes))
}

fn parse_dii(transaction_id: u32, bytes: &[u8]) -> Result<DownloadInfo, Error> {
    let mut reader = Reader::new(bytes);
    let download_id = reader.u32()?;
    let block_size = reader.u16()?;
    if block_size == 0 {
        return Err(Error::Invalid("zero block size"));
    }
    reader.take(1 + 1 + 4 + 4)?; // windowSize, ackPeriod, two timeout fields
    let compatibility_length = usize::from(reader.u16()?);
    reader.take(compatibility_length)?;
    let count = usize::from(reader.u16()?);
    let mut modules = Vec::with_capacity(count.min(1024));
    for _ in 0..count {
        let id = reader.u16()?;
        let size = reader.u32()?;
        let version = reader.u8()?;
        let info_length = usize::from(reader.u8()?);
        let descriptors = descriptors(reader.take(info_length)?)?;
        if modules.iter().any(|module: &ModuleInfo| module.id == id) {
            return Err(Error::Invalid("duplicate module ID"));
        }
        modules.push(ModuleInfo {
            id,
            size,
            version,
            descriptors,
        });
    }
    let private_length = usize::from(reader.u16()?);
    let descriptors = descriptors(reader.take(private_length)?)?;
    reader.finish()?;
    Ok(DownloadInfo {
        transaction_id,
        download_id,
        block_size,
        modules,
        descriptors,
    })
}

fn parse_ddb(download_id: u32, bytes: &[u8]) -> Result<DownloadBlock, Error> {
    let mut reader = Reader::new(bytes);
    let module_id = reader.u16()?;
    let module_version = reader.u8()?;
    if reader.u8()? != 0xff {
        return Err(Error::Invalid("DDB reserved byte"));
    }
    let block_number = reader.u16()?;
    Ok(DownloadBlock {
        download_id,
        module_id,
        module_version,
        block_number,
        data: reader.bytes.to_vec(),
    })
}

/// A fully reconstructed module. `data` is still compressed if its DII has a compression descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    pub download_id: u32,
    pub info: ModuleInfo,
    pub data: Vec<u8>,
}

/// Bytes ready for a BML/resource interpreter, with DII metadata retained.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedModule {
    pub download_id: u32,
    pub info: ModuleInfo,
    pub data: Vec<u8>,
}

impl Module {
    /// Expands compression type 0 (RFC 1950 zlib), as used by ARIB TR-B15.
    /// The advertised original size and the decoder's size limit are enforced.
    pub fn decode(self) -> Result<DecodedModule, Error> {
        self.decode_with_limits(DecodeLimits::default())
    }

    pub fn decode_with_limits(self, limits: DecodeLimits) -> Result<DecodedModule, Error> {
        if self.data.len()
            != usize::try_from(self.info.size)
                .map_err(|_| Error::Invalid("module size exceeds platform"))?
        {
            return Err(Error::Invalid("module wire size"));
        }
        if self.data.len() > limits.max_module_bytes {
            return Err(Error::ModuleTooLarge {
                actual: self.data.len(),
                limit: limits.max_module_bytes,
            });
        }

        let mut compression = self.info.descriptors.iter().filter(|item| item.tag == 0xc2);
        let data = if let Some(descriptor) = compression.next() {
            if compression.next().is_some() || descriptor.data.len() != 5 {
                return Err(Error::Invalid("compression descriptor"));
            }
            if descriptor.data[0] != 0 {
                return Err(Error::UnsupportedCompression(descriptor.data[0]));
            }
            let original_size = u32::from_be_bytes(
                descriptor.data[1..]
                    .try_into()
                    .map_err(|_| Error::Invalid("compression descriptor"))?,
            );
            let original_size = usize::try_from(original_size)
                .map_err(|_| Error::Invalid("decoded size exceeds platform"))?;
            if original_size > limits.max_decoded_bytes {
                return Err(Error::DecodedTooLarge {
                    actual: original_size,
                    limit: limits.max_decoded_bytes,
                });
            }
            compression::decode(&self.data, original_size)?
        } else {
            self.data
        };
        Ok(DecodedModule {
            download_id: self.download_id,
            info: self.info,
            data,
        })
    }
}

struct Pending {
    info: ModuleInfo,
    blocks: Vec<Option<Vec<u8>>>,
    remaining: usize,
}

/// A carousel bound to a parsed DII. Replacing the DII starts a new generation.
pub struct Carousel {
    info: DownloadInfo,
    module_indices: BTreeMap<u16, usize>,
    pending: BTreeMap<u16, Pending>,
    limits: DecodeLimits,
}

impl Carousel {
    pub fn new(info: DownloadInfo) -> Result<Self, Error> {
        Self::with_limits(info, DecodeLimits::default())
    }

    pub fn with_limits(info: DownloadInfo, limits: DecodeLimits) -> Result<Self, Error> {
        if info.block_size == 0 {
            return Err(Error::Invalid("zero block size"));
        }
        let mut pending = BTreeMap::new();
        let mut module_indices = BTreeMap::new();
        for module in &info.modules {
            module.module_link()?;
            if module.size == 0 {
                return Err(Error::Invalid("unknown module size"));
            }
            let module_size = usize::try_from(module.size)
                .map_err(|_| Error::Invalid("module size exceeds platform"))?;
            if module_size > STANDARD_MAX_MODULE_BYTES {
                return Err(Error::Invalid("module exceeds standard maximum"));
            }
            if module_size > limits.max_module_bytes {
                return Err(Error::ModuleTooLarge {
                    actual: module_size,
                    limit: limits.max_module_bytes,
                });
            }
            if module
                .descriptors
                .iter()
                .any(|descriptor| descriptor.tag == 0x05 && descriptor.data.len() != 4)
            {
                return Err(Error::Invalid("module CRC descriptor"));
            }
            let count = module_size.div_ceil(usize::from(info.block_size));
            if count > usize::from(u16::MAX) + 1 {
                return Err(Error::Invalid("block count"));
            }
            if pending
                .insert(
                    module.id,
                    Pending {
                        info: module.clone(),
                        blocks: vec![None; count],
                        remaining: count,
                    },
                )
                .is_some()
            {
                return Err(Error::Invalid("duplicate module ID"));
            }
            module_indices.insert(module.id, module_indices.len());
        }
        Ok(Self {
            info,
            module_indices,
            pending,
            limits,
        })
    }

    pub fn info(&self) -> &DownloadInfo {
        &self.info
    }

    /// Adds another DII fragment from the same carousel generation.
    pub fn merge_info(&mut self, mut info: DownloadInfo) -> Result<(), Error> {
        if info.download_id != self.info.download_id
            || info.transaction_id != self.info.transaction_id
            || info.block_size != self.info.block_size
        {
            return Err(Error::Invalid("DII carousel identity"));
        }
        // DII repeats every carousel cycle. Do not reconstruct block storage
        // for modules already received or still being assembled.
        if info == self.info {
            return Ok(());
        }
        let mut seen = std::collections::BTreeSet::new();
        for module in &info.modules {
            if !seen.insert(module.id) {
                return Err(Error::Invalid("duplicate module ID"));
            }
            if let Some(&index) = self.module_indices.get(&module.id)
                && self.info.modules[index] != *module
            {
                return Err(Error::Invalid("conflicting DII module"));
            }
        }
        info.modules
            .retain(|module| !self.module_indices.contains_key(&module.id));
        if info.modules.is_empty() {
            return Ok(());
        }
        // Validate all new modules before committing any of them.
        let candidate = Self::with_limits(info, self.limits)?;
        for module in candidate.info.modules {
            self.module_indices
                .insert(module.id, self.info.modules.len());
            self.info.modules.push(module);
        }
        self.pending.extend(candidate.pending);
        Ok(())
    }

    fn retry_module(&mut self, id: u16) {
        if let Some(&index) = self.module_indices.get(&id) {
            let info = &self.info.modules[index];
            let count = (info.size as usize).div_ceil(usize::from(self.info.block_size));
            self.pending.insert(
                id,
                Pending {
                    info: info.clone(),
                    blocks: vec![None; count],
                    remaining: count,
                },
            );
        }
    }

    /// Accepts a DDB. Duplicate blocks are harmless; conflicting duplicates are errors.
    /// Blocks from another carousel or module version are ignored as stale.
    pub fn push(&mut self, block: DownloadBlock) -> Result<Option<Module>, Error> {
        if block.download_id != self.info.download_id {
            return Ok(None);
        }
        let Some(pending) = self.pending.get_mut(&block.module_id) else {
            return Ok(None);
        };
        if block.module_version != pending.info.version {
            return Ok(None);
        }
        let index = usize::from(block.block_number);
        let Some(slot) = pending.blocks.get_mut(index) else {
            return Err(Error::Invalid("block number"));
        };
        let expected = (usize::try_from(pending.info.size)
            .map_err(|_| Error::Invalid("module size exceeds platform"))?
            - index * usize::from(self.info.block_size))
        .min(usize::from(self.info.block_size));
        if block.data.len() != expected {
            return Err(Error::Invalid("block size"));
        }
        if let Some(existing) = slot {
            if *existing != block.data {
                return Err(Error::Invalid("conflicting block"));
            }
            return Ok(None);
        }
        *slot = Some(block.data);
        pending.remaining -= 1;
        if pending.remaining != 0 {
            return Ok(None);
        }
        let Some(complete) = self.pending.remove(&block.module_id) else {
            unreachable!()
        };
        let block_count = complete.blocks.len();
        let size = usize::try_from(complete.info.size)
            .map_err(|_| Error::Invalid("module size exceeds platform"))?;
        let mut data = Vec::new();
        data.try_reserve_exact(size).map_err(Error::Allocation)?;
        for part in complete.blocks {
            data.extend(part.ok_or(Error::Invalid("missing block"))?);
        }
        for descriptor in &complete.info.descriptors {
            if descriptor.tag == 0x05 {
                let expected: [u8; 4] = descriptor
                    .data
                    .as_slice()
                    .try_into()
                    .map_err(|_| Error::Invalid("module CRC descriptor"))?;
                if crc32(&data) != u32::from_be_bytes(expected) {
                    self.pending.insert(
                        block.module_id,
                        Pending {
                            info: complete.info,
                            blocks: vec![None; block_count],
                            remaining: block_count,
                        },
                    );
                    return Err(Error::Invalid("module CRC-32"));
                }
            }
        }
        Ok(Some(Module {
            download_id: self.info.download_id,
            info: complete.info,
            data,
        }))
    }
}
