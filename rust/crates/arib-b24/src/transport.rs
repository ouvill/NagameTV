//! MPEG-2 TS section intake and PMT discovery for STD-B24 carousels.
//! See ARIB STD-B24 Part 3 §6.5 and ARIB TR-B15 Part 1 §5.3.

use super::{
    Carousel, DecodeLimits, DecodedModule, DownloadInfo, Error, ModuleLink, Reader, Section,
    event::EventSection, resource::Resource,
};
use std::collections::{BTreeMap, BTreeSet};

pub mod bit;

const DSMCC_STREAM_TYPE_B: u8 = 0x0b;
const DSMCC_STREAM_TYPE_D: u8 = 0x0d;
const STREAM_IDENTIFIER: u8 = 0x52;
const DATA_COMPONENT: u8 = 0xfd;
const DEFAULT_COMPONENT_TAG: u8 = 0x40;
const BML_COMPONENT_IDS: [u16; 4] = [0x0007, 0x000b, 0x000c, 0x000d];
const EVENT_TABLE: u8 = 0x3d;

/// A resource or an event message to pass to a BML presentation layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BroadcastItem {
    Resource(Resource),
    Event(EventSection),
}

/// State changes and payloads needed by a BML browser adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceUpdate {
    Components(Vec<DataComponent>),
    Broadcasters(bit::BroadcasterInformation),
    ModuleList {
        component: DataComponent,
        info: DownloadInfo,
    },
    Module {
        component: DataComponent,
        resources: Vec<Resource>,
    },
    Event {
        component: DataComponent,
        section: EventSection,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataUpdate {
    ModuleList(DownloadInfo),
    Module(Vec<Resource>),
    Event(EventSection),
}

enum DataMessage {
    Info(DownloadInfo),
    Module {
        module: DecodedModule,
        contributors: Vec<u16>,
    },
    Event(EventSection),
}

/// Emits only complete files; a linked fragment is retained until its chain is complete.
fn complete_linked(
    info: &DownloadInfo,
    fragments: &mut BTreeMap<u16, DecodedModule>,
    limits: DecodeLimits,
) -> Result<Vec<(DecodedModule, Vec<u16>)>, Error> {
    let modules: BTreeMap<_, _> = info
        .modules
        .iter()
        .map(|module| (module.id, module))
        .collect();
    let mut predecessors = BTreeSet::new();
    for module in &info.modules {
        if let Some(ModuleLink::Head { next } | ModuleLink::Middle { next }) =
            module.module_link()?
            && !predecessors.insert(next)
        {
            return Err(Error::Invalid("multiple module links to one module"));
        }
    }
    let mut completed = Vec::new();
    for head in &info.modules {
        if !matches!(head.module_link()?, Some(ModuleLink::Head { .. })) {
            continue;
        }
        let mut ids = Vec::new();
        let mut visited = BTreeSet::new();
        let mut current = head.id;
        let mut total = 0_usize;
        loop {
            if !visited.insert(current) {
                return Err(Error::Invalid("module link cycle"));
            }
            let Some(module) = modules.get(&current) else {
                break; // A later DII may advertise this module.
            };
            let link = module
                .module_link()?
                .ok_or(Error::Invalid("missing module link"))?;
            if ids.is_empty() && !matches!(link, ModuleLink::Head { .. })
                || !ids.is_empty() && matches!(link, ModuleLink::Head { .. })
            {
                return Err(Error::Invalid("module link position"));
            }
            let Some(fragment) = fragments.get(&current) else {
                break;
            };
            total = total
                .checked_add(fragment.data.len())
                .ok_or(Error::Invalid("linked file size overflow"))?;
            if total > limits.max_linked_bytes() {
                return Err(Error::LinkedFileTooLarge {
                    actual: total,
                    limit: limits.max_linked_bytes(),
                });
            }
            ids.push(current);
            match link {
                ModuleLink::Head { next } | ModuleLink::Middle { next } => current = next,
                ModuleLink::Last => {
                    let mut data = Vec::new();
                    data.try_reserve_exact(total).map_err(Error::Allocation)?;
                    for id in &ids {
                        let fragment = fragments
                            .remove(id)
                            .ok_or(Error::Invalid("missing linked fragment"))?;
                        data.extend_from_slice(&fragment.data);
                    }
                    let mut head_info = head.clone();
                    head_info
                        .descriptors
                        .retain(|descriptor| descriptor.tag != 0x04);
                    completed.push((
                        DecodedModule {
                            download_id: info.download_id,
                            info: head_info,
                            data,
                        },
                        ids,
                    ));
                    break;
                }
            }
        }
    }
    Ok(completed)
}

/// BML-capable DSM-CC elementary stream advertised in a CRC-checked PMT.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataComponent {
    pid: u16,
    tag: u8,
    data_component_id: u16,
    stream_type: u8,
    bxml_info: Option<BxmlInfo>,
}

/// Additional ARIB BXML information in the PMT data component descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BxmlInfo {
    pub transmission_format: u8,
    pub entry_point: Option<BxmlEntryPoint>,
    pub carousel: Option<BxmlCarousel>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BxmlEntryPoint {
    pub auto_start: bool,
    pub document_resolution: u8,
    pub use_xml: bool,
    pub default_version: bool,
    pub independent: bool,
    pub style_for_tv: bool,
    pub bml_major_version: u16,
    pub bml_minor_version: u16,
    pub bxml_version: Option<(u16, u16)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BxmlCarousel {
    pub data_event_id: u8,
    pub event_sections: bool,
    pub ondemand_retrieval: bool,
    pub file_storable: bool,
    pub start_priority: bool,
}

impl DataComponent {
    pub fn pid(self) -> u16 {
        self.pid
    }
    pub fn tag(self) -> u8 {
        self.tag
    }
    pub fn data_component_id(self) -> u16 {
        self.data_component_id
    }
    pub fn stream_type(self) -> u8 {
        self.stream_type
    }
    pub fn bxml_info(self) -> Option<BxmlInfo> {
        self.bxml_info
    }
    pub fn is_entry(self) -> bool {
        self.tag == DEFAULT_COMPONENT_TAG
    }
}

bitfield::bitfield! {
    struct BxmlHeader(u8);
    u8, transmission_format, _: 7, 6;
    bool, entry_point, _: 5;
    bool, auto_start, _: 4;
    u8, document_resolution, _: 3, 0;
}

bitfield::bitfield! {
    struct BxmlEntryFlags(u8);
    bool, use_xml, _: 7;
    bool, default_version, _: 6;
    bool, independent, _: 5;
    bool, style_for_tv, _: 4;
}

bitfield::bitfield! {
    struct BxmlCarouselFlags(u8);
    u8, data_event_id, _: 7, 4;
    bool, event_sections, _: 3;
}

bitfield::bitfield! {
    struct BxmlCarouselOptions(u8);
    bool, ondemand_retrieval, _: 7;
    bool, file_storable, _: 6;
    bool, start_priority, _: 5;
}

fn parse_bxml_info(bytes: &[u8]) -> Result<BxmlInfo, Error> {
    let mut reader = Reader::new(bytes);
    let header = BxmlHeader(reader.u8()?);
    let transmission_format = header.transmission_format();
    let entry_point = if header.entry_point() {
        let flags = BxmlEntryFlags(reader.u8()?);
        // The four reserved bits are the low nibble of `flags`.
        let default_version = flags.default_version();
        let (bml_major_version, bml_minor_version, bxml_version) = if default_version {
            (1, 0, None)
        } else {
            let major = reader.u16()?;
            let minor = reader.u16()?;
            let bxml = if flags.use_xml() {
                Some((reader.u16()?, reader.u16()?))
            } else {
                None
            };
            (major, minor, bxml)
        };
        Some(BxmlEntryPoint {
            auto_start: header.auto_start(),
            document_resolution: header.document_resolution(),
            use_xml: flags.use_xml(),
            default_version,
            independent: flags.independent(),
            style_for_tv: flags.style_for_tv(),
            bml_major_version,
            bml_minor_version,
            bxml_version,
        })
    } else {
        // Five reserved bits complete the first byte.
        None
    };
    let carousel = if transmission_format == 0 {
        let flags = BxmlCarouselFlags(reader.u8()?);
        let options = BxmlCarouselOptions(reader.u8()?);
        // Six reserved bits complete `options`.
        Some(BxmlCarousel {
            data_event_id: flags.data_event_id(),
            event_sections: flags.event_sections(),
            ondemand_retrieval: options.ondemand_retrieval(),
            file_storable: options.file_storable(),
            start_priority: options.start_priority(),
        })
    } else {
        None
    };
    Ok(BxmlInfo {
        transmission_format,
        entry_point,
        carousel,
    })
}

struct DescriptorFields {
    tag: Option<u8>,
    component: Option<(u16, Option<BxmlInfo>)>,
}

fn descriptor_fields(bytes: &[u8]) -> Result<DescriptorFields, Error> {
    let mut reader = Reader::new(bytes);
    let (mut tag, mut component) = (None, None);
    while !reader.bytes.is_empty() {
        let descriptor_tag = reader.u8()?;
        let length = usize::from(reader.u8()?);
        let value = reader.take(length)?;
        match descriptor_tag {
            STREAM_IDENTIFIER => {
                if value.len() != 1 || tag.replace(value[0]).is_some() {
                    return Err(Error::Invalid("stream identifier descriptor"));
                }
            }
            DATA_COMPONENT => {
                if value.len() < 2 || component.is_some() {
                    return Err(Error::Invalid("data component descriptor"));
                }
                let id = u16::from_be_bytes([value[0], value[1]]);
                let info = if BML_COMPONENT_IDS.contains(&id) && value.len() > 2 {
                    Some(parse_bxml_info(&value[2..])?)
                } else {
                    None
                };
                component = Some((id, info));
            }
            _ => {}
        }
    }
    Ok(DescriptorFields { tag, component })
}

fn psi_error(error: viewer_mpegts::ParseError, context: &'static str) -> Error {
    match error {
        viewer_mpegts::ParseError::Incomplete => Error::Truncated(context),
        viewer_mpegts::ParseError::Invalid("PSI CRC") if context == "PAT section" => {
            Error::Invalid("PAT CRC-32")
        }
        viewer_mpegts::ParseError::Invalid("PSI CRC") if context == "PMT section" => {
            Error::Invalid("PMT CRC-32")
        }
        viewer_mpegts::ParseError::Invalid(reason) => Error::Invalid(reason),
    }
}

/// Finds the PMT PID for `service_id` in one CRC-checked PAT section.
/// Call for each PAT section when the table spans multiple sections.
pub fn pmt_pid_from_pat(bytes: &[u8], service_id: u16) -> Result<Option<u16>, Error> {
    let section =
        viewer_mpegts::PsiSection::parse(bytes).map_err(|error| psi_error(error, "PAT section"))?;
    let programs = section
        .pat_programs()
        .map_err(|error| psi_error(error, "PAT section"))?;
    let mut selected = None;
    for (service, pid) in programs {
        if service == service_id && selected.replace(pid.0).is_some() {
            return Err(Error::Invalid("duplicate PAT service"));
        }
    }
    Ok(selected)
}

/// Discovers BML data streams in one complete PMT section for `service_id`.
/// Other valid services return an empty list.
pub fn data_components_from_pmt(
    bytes: &[u8],
    service_id: u16,
) -> Result<Vec<DataComponent>, Error> {
    let section =
        viewer_mpegts::PsiSection::parse(bytes).map_err(|error| psi_error(error, "PMT section"))?;
    let map = section
        .program_map()
        .map_err(|error| psi_error(error, "PMT section"))?;
    if map.service != service_id {
        return Ok(Vec::new());
    }
    let mut components = Vec::new();
    for stream in map.streams {
        if !matches!(
            stream.stream_type,
            DSMCC_STREAM_TYPE_B | DSMCC_STREAM_TYPE_D
        ) {
            continue;
        }
        let fields = descriptor_fields(stream.descriptors)?;
        if let (Some(tag), Some((data_component_id, bxml_info))) = (fields.tag, fields.component)
            && (DEFAULT_COMPONENT_TAG..=0xff).contains(&tag)
            && BML_COMPONENT_IDS.contains(&data_component_id)
        {
            components.push(DataComponent {
                pid: stream.pid.0,
                tag,
                data_component_id,
                stream_type: stream.stream_type,
                bxml_info,
            });
        }
    }
    Ok(components)
}

/// Reassembles complete sections from 188-byte packets of one PID.
/// Use PID 0 for PAT, then the discovered PMT PID, then a data component PID.
pub struct SectionPackets {
    pid: viewer_mpegts::Pid,
    sections: viewer_mpegts::Sections,
    previous: Option<PreviousPayload>,
}

// A TS payload fits in one packet. Retaining it inline avoids a heap allocation
// for every packet, including repeats of already completed carousel modules.
struct PreviousPayload {
    counter: u8,
    bytes: [u8; viewer_mpegts::TS_PACKET_SIZE],
    len: usize,
}
impl SectionPackets {
    pub fn new(pid: u16) -> Result<Self, Error> {
        if pid > 0x1ffe {
            return Err(Error::Invalid("section PID"));
        }
        Ok(Self {
            pid: viewer_mpegts::Pid(pid),
            sections: viewer_mpegts::Sections::new(viewer_mpegts::SectionKind::Private),
            previous: None,
        })
    }
    fn reset(&mut self) {
        self.sections = viewer_mpegts::Sections::new(viewer_mpegts::SectionKind::Private);
        self.previous = None;
    }
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<Vec<u8>>, Error> {
        let packet = match viewer_mpegts::TransportPacket::parse(bytes) {
            Ok(packet) => packet,
            Err(error) => {
                self.reset();
                return Err(psi_error(error, "TS packet"));
            }
        };
        if packet.pid != self.pid {
            return Ok(Vec::new());
        }
        if packet.discontinuity {
            self.reset();
        }
        if packet.payload.is_empty() {
            return Ok(Vec::new());
        }
        if let Some(previous) = &self.previous {
            if previous.counter == packet.continuity_counter {
                if &previous.bytes[..previous.len] == packet.payload {
                    return Ok(Vec::new());
                }
                self.reset();
                return Err(Error::Invalid("conflicting TS duplicate"));
            }
            if (previous.counter + 1) & 0x0f != packet.continuity_counter {
                self.reset();
                return Err(Error::Invalid("TS continuity"));
            }
        }
        let previous = self.previous.get_or_insert(PreviousPayload {
            counter: packet.continuity_counter,
            bytes: [0; viewer_mpegts::TS_PACKET_SIZE],
            len: 0,
        });
        previous.counter = packet.continuity_counter;
        previous.len = packet.payload.len();
        previous.bytes[..previous.len].copy_from_slice(packet.payload);
        Ok(self.sections.push(packet.start, packet.payload))
    }
}

/// Extracts DII/DDB sections from packets for a selected data PID.
pub struct TsReceiver {
    packets: SectionPackets,
}

impl TsReceiver {
    pub fn new(pid: u16) -> Result<Self, Error> {
        Ok(Self {
            packets: SectionPackets::new(pid)?,
        })
    }

    pub fn push(&mut self, packet: &[u8]) -> Result<Vec<Section>, Error> {
        self.packets
            .push(packet)?
            .into_iter()
            .filter(|section| matches!(section[0], super::DII_TABLE | super::DDB_TABLE))
            .map(|section| Section::parse(&section))
            .collect()
    }
}

/// Receives complete module resources from the BML data PID selected in a PMT.
pub struct DataReceiver {
    ts: TsReceiver,
    carousels: BTreeMap<u32, Carousel>,
    linked: BTreeMap<u32, BTreeMap<u16, DecodedModule>>,
    limits: DecodeLimits,
}

impl DataReceiver {
    pub fn new(component: DataComponent) -> Self {
        Self::with_limits(component, DecodeLimits::default())
    }

    pub fn with_limits(component: DataComponent, limits: DecodeLimits) -> Self {
        Self {
            ts: TsReceiver::new(component.pid).expect("PMT validated PID"),
            carousels: BTreeMap::new(),
            linked: BTreeMap::new(),
            limits,
        }
    }

    pub fn push(&mut self, packet: &[u8]) -> Result<Vec<DecodedModule>, Error> {
        Ok(self
            .receive(packet)?
            .into_iter()
            .filter_map(|item| match item {
                DataMessage::Info(_) => None,
                DataMessage::Module { module, .. } => Some(module),
                DataMessage::Event(_) => None, // Module-only compatibility API.
            })
            .collect())
    }

    fn receive(&mut self, packet: &[u8]) -> Result<Vec<DataMessage>, Error> {
        let mut completed = Vec::new();
        for raw in self.ts.packets.push(packet)? {
            if raw[0] == EVENT_TABLE {
                completed.push(DataMessage::Event(EventSection::parse(&raw)?));
                continue;
            }
            if !matches!(raw[0], super::DII_TABLE | super::DDB_TABLE) {
                continue;
            }
            let section = Section::parse(&raw)?;
            match section {
                Section::Info(info) => {
                    let download_id = info.download_id;
                    let info_changed;
                    if let Some(current) = self.carousels.get_mut(&info.download_id)
                        && current.info().transaction_id == info.transaction_id
                    {
                        let previous_count = current.info().modules.len();
                        current.merge_info(info)?;
                        info_changed = current.info().modules.len() != previous_count;
                    } else {
                        let carousel = Carousel::with_limits(info, self.limits)?;
                        self.linked.remove(&download_id);
                        self.carousels.insert(download_id, carousel);
                        info_changed = true;
                    }
                    if info_changed {
                        let info = self
                            .carousels
                            .get(&download_id)
                            .ok_or(Error::Invalid("missing carousel after DII"))?
                            .info()
                            .clone();
                        completed.push(DataMessage::Info(info));
                    }
                    if let Some(carousel) = self.carousels.get(&download_id)
                        && let Some(fragments) = self.linked.get_mut(&download_id)
                    {
                        completed.extend(
                            complete_linked(carousel.info(), fragments, self.limits)?
                                .into_iter()
                                .map(|(module, contributors)| DataMessage::Module {
                                    module,
                                    contributors,
                                }),
                        );
                    }
                }
                Section::Block(block) => {
                    if let Some(carousel) = self.carousels.get_mut(&block.download_id)
                        && let Some(module) = carousel.push(block)?
                    {
                        let id = module.info.id;
                        match module.decode_with_limits(self.limits) {
                            Ok(decoded) => {
                                if decoded.info.module_link()?.is_some() {
                                    let fragments =
                                        self.linked.entry(decoded.download_id).or_default();
                                    fragments.insert(decoded.info.id, decoded);
                                    completed.extend(
                                        complete_linked(carousel.info(), fragments, self.limits)?
                                            .into_iter()
                                            .map(|(module, contributors)| DataMessage::Module {
                                                module,
                                                contributors,
                                            }),
                                    );
                                } else {
                                    completed.push(DataMessage::Module {
                                        module: decoded,
                                        contributors: vec![id],
                                    });
                                }
                            }
                            Err(error) => {
                                carousel.retry_module(id);
                                return Err(error);
                            }
                        }
                    }
                }
            }
        }
        Ok(completed)
    }

    /// Returns individual resources from every completed module in this packet.
    pub fn push_resources(&mut self, packet: &[u8]) -> Result<Vec<Resource>, Error> {
        Ok(self
            .push_items(packet)?
            .into_iter()
            .filter_map(|item| match item {
                BroadcastItem::Resource(resource) => Some(resource),
                BroadcastItem::Event(_) => None, // Resource-only convenience API.
            })
            .collect())
    }

    /// Returns resources and parsed event messages from the component.
    pub fn push_items(&mut self, packet: &[u8]) -> Result<Vec<BroadcastItem>, Error> {
        Ok(self
            .push_updates(packet)?
            .into_iter()
            .flat_map(|update| match update {
                DataUpdate::ModuleList(_) => Vec::new(),
                DataUpdate::Module(resources) => {
                    resources.into_iter().map(BroadcastItem::Resource).collect()
                }
                DataUpdate::Event(event) => vec![BroadcastItem::Event(event)],
            })
            .collect())
    }

    /// Includes DII module-list changes needed to drive a BML browser.
    pub fn push_updates(&mut self, packet: &[u8]) -> Result<Vec<DataUpdate>, Error> {
        let mut items = Vec::new();
        for message in self.receive(packet)? {
            match message {
                DataMessage::Info(info) => items.push(DataUpdate::ModuleList(info)),
                DataMessage::Module {
                    module,
                    contributors,
                } => {
                    let download_id = module.download_id;
                    match module.resources() {
                        Ok(resources) => {
                            items.push(DataUpdate::Module(resources));
                        }
                        Err(error) => {
                            if let Some(carousel) = self.carousels.get_mut(&download_id) {
                                for id in contributors {
                                    carousel.retry_module(id);
                                }
                            }
                            return Err(error);
                        }
                    }
                }
                DataMessage::Event(event) => items.push(DataUpdate::Event(event)),
            }
        }
        Ok(items)
    }
}

/// Receives the resources of all BML data components for one service.
/// The caller supplies unmodified 188-byte TS packets from a tuner, recording, or network source.
pub struct ServiceReceiver {
    service_id: u16,
    pat_packets: SectionPackets,
    pat: viewer_mpegts::Pat,
    bit_packets: SectionPackets,
    bit: bit::Sections,
    pmt: Option<ProgramAssociation>,
    components: BTreeMap<u16, (DataComponent, DataReceiver)>,
    limits: DecodeLimits,
}

struct ProgramAssociation {
    transport_stream_id: u16,
    pid: u16,
    packets: SectionPackets,
    program: Option<ServiceProgram>,
}

/// Identity and clock source acquired from a complete PAT and the selected PMT.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceProgram {
    transport_stream_id: u16,
    service_id: u16,
    pcr_pid: u16,
}
impl ServiceProgram {
    pub fn transport_stream_id(self) -> u16 {
        self.transport_stream_id
    }
    pub fn service_id(self) -> u16 {
        self.service_id
    }
    pub fn pcr_pid(self) -> u16 {
        self.pcr_pid
    }
}

impl ServiceReceiver {
    pub fn new(service_id: u16) -> Result<Self, Error> {
        Self::with_limits(service_id, DecodeLimits::default())
    }

    pub fn with_limits(service_id: u16, limits: DecodeLimits) -> Result<Self, Error> {
        if service_id == 0 {
            return Err(Error::Invalid("service ID"));
        }
        Ok(Self {
            service_id,
            pat_packets: SectionPackets::new(0)?,
            pat: viewer_mpegts::Pat::default(),
            bit_packets: SectionPackets::new(bit::PID)?,
            bit: bit::Sections::default(),
            pmt: None,
            components: BTreeMap::new(),
            limits,
        })
    }

    pub fn pmt_pid(&self) -> Option<u16> {
        self.pmt.as_ref().map(|pmt| pmt.pid)
    }

    pub fn program(&self) -> Option<ServiceProgram> {
        self.pmt.as_ref().and_then(|pmt| pmt.program)
    }

    pub fn components(&self) -> impl Iterator<Item = DataComponent> + '_ {
        self.components.values().map(|(component, _)| *component)
    }

    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<Resource>, Error> {
        Ok(self
            .push_items(bytes)?
            .into_iter()
            .filter_map(|item| match item {
                BroadcastItem::Resource(resource) => Some(resource),
                BroadcastItem::Event(_) => None, // Resource-only convenience API.
            })
            .collect())
    }

    /// Returns resources and event messages for the selected service.
    pub fn push_items(&mut self, bytes: &[u8]) -> Result<Vec<BroadcastItem>, Error> {
        Ok(self
            .push_updates(bytes)?
            .into_iter()
            .flat_map(|update| match update {
                ServiceUpdate::Components(_)
                | ServiceUpdate::Broadcasters(_)
                | ServiceUpdate::ModuleList { .. } => Vec::new(),
                ServiceUpdate::Module { resources, .. } => {
                    resources.into_iter().map(BroadcastItem::Resource).collect()
                }
                ServiceUpdate::Event { section, .. } => vec![BroadcastItem::Event(section)],
            })
            .collect())
    }

    /// Emits PMT components, DII module lists, resources and event sections.
    pub fn push_updates(&mut self, bytes: &[u8]) -> Result<Vec<ServiceUpdate>, Error> {
        let packet = viewer_mpegts::TransportPacket::parse(bytes)
            .map_err(|error| psi_error(error, "TS packet"))?;
        if packet.pid.0 == bit::PID {
            let mut updates = Vec::new();
            for raw in self.bit_packets.push(bytes)? {
                if let Some(information) = self.bit.push(&raw)? {
                    updates.push(ServiceUpdate::Broadcasters(information));
                }
            }
            return Ok(updates);
        }
        if packet.pid == viewer_mpegts::Pid::PAT {
            let mut updates = Vec::new();
            for raw in self.pat_packets.push(bytes)? {
                let section = viewer_mpegts::PsiSection::parse(&raw)
                    .map_err(|error| psi_error(error, "PAT section"))?;
                if let Some(programs) = self.pat.push(&section) {
                    let next = programs.get(&self.service_id).map(|pid| pid.0);
                    if next != self.pmt_pid()
                        || self
                            .pmt
                            .as_ref()
                            .is_some_and(|pmt| pmt.transport_stream_id != section.extension)
                    {
                        self.pmt = match next {
                            Some(pid) => Some(ProgramAssociation {
                                transport_stream_id: section.extension,
                                pid,
                                packets: SectionPackets::new(pid)?,
                                program: None,
                            }),
                            None => None,
                        };
                        self.components.clear();
                        updates.push(ServiceUpdate::Components(Vec::new()));
                    }
                }
            }
            return Ok(updates);
        }
        if let Some(pmt) = &mut self.pmt
            && packet.pid.0 == pmt.pid
        {
            let transport_stream_id = pmt.transport_stream_id;
            let sections = pmt.packets.push(bytes)?;
            let mut updates = Vec::new();
            for raw in sections {
                let section = viewer_mpegts::PsiSection::parse(&raw)
                    .map_err(|error| psi_error(error, "PMT section"))?;
                let map = section
                    .program_map()
                    .map_err(|error| psi_error(error, "PMT section"))?;
                // Several services can share a PMT PID. Another service's PMT
                // must not clear this service's components or clock source.
                if map.service != self.service_id {
                    continue;
                }
                let discovered = data_components_from_pmt(&raw, self.service_id)?;
                self.pmt.as_mut().expect("selected PMT").program = Some(ServiceProgram {
                    transport_stream_id,
                    service_id: map.service,
                    pcr_pid: map.pcr_pid.0,
                });
                let previous: Vec<_> = self.components().collect();
                let mut old = std::mem::take(&mut self.components);
                for component in discovered {
                    let receiver = match old.remove(&component.pid) {
                        Some((previous, receiver)) if previous == component => receiver,
                        _ => DataReceiver::with_limits(component, self.limits),
                    };
                    self.components.insert(component.pid, (component, receiver));
                }
                if previous != self.components().collect::<Vec<_>>() {
                    updates.push(ServiceUpdate::Components(self.components().collect()));
                }
            }
            return Ok(updates);
        }
        if let Some((component, receiver)) = self.components.get_mut(&packet.pid.0) {
            return Ok(receiver
                .push_updates(bytes)?
                .into_iter()
                .map(|update| match update {
                    DataUpdate::ModuleList(info) => ServiceUpdate::ModuleList {
                        component: *component,
                        info,
                    },
                    DataUpdate::Module(mut resources) => {
                        for resource in &mut resources {
                            resource.component_tag = Some(component.tag);
                        }
                        ServiceUpdate::Module {
                            component: *component,
                            resources,
                        }
                    }
                    DataUpdate::Event(section) => ServiceUpdate::Event {
                        component: *component,
                        section,
                    },
                })
                .collect());
        }
        Ok(Vec::new())
    }
}
