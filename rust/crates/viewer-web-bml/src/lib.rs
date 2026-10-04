//! Converts decoded ARIB carousel updates to the web-bml browser message schema.
//! The browser runs inside Qt WebEngine; this crate has no Qt or Node.js dependency.

use arib_b24::{
    event::EventTime,
    resource::{Resource, ResourceMapping},
    transport::{BxmlInfo, DataComponent, ServiceUpdate},
};
use base64::{display::Base64Display, engine::general_purpose::STANDARD};
use serde::{Serialize, Serializer};
use serde_json::{Value, json};

/// Supply the browser's startup prerequisite while program metadata is unavailable.
/// Unknown fields remain null rather than inventing broadcast metadata.
pub fn program_info(service_id: u16, original_network_id: Option<u16>) -> Value {
    program(&ProgramInfo {
        service_id,
        original_network_id,
        transport_stream_id: None,
        event: None,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramInfo {
    pub service_id: u16,
    pub original_network_id: Option<u16>,
    pub transport_stream_id: Option<u16>,
    pub event: Option<ProgramEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramEvent {
    pub id: u16,
    pub name: String,
    pub start_unix_ms: Option<i64>,
    pub duration_seconds: Option<u64>,
}

pub fn program(info: &ProgramInfo) -> Value {
    json!({
        "type": "programInfo",
        "originalNetworkId": info.original_network_id,
        "transportStreamId": info.transport_stream_id,
        "serviceId": info.service_id,
        "eventId": info.event.as_ref().map(|event| event.id),
        "eventName": info.event.as_ref().map(|event| &event.name),
        "startTimeUnixMillis": info.event.as_ref().and_then(|event| event.start_unix_ms),
        "durationSeconds": info.event.as_ref().and_then(|event| event.duration_seconds),
        "indefiniteDuration": info.event.as_ref().map(|event| event.duration_seconds.is_none()),
        "networkId": null,
    })
}

pub fn current_time(unix_ms: i64) -> Value {
    json!({"type": "currentTime", "timeUnixMillis": unix_ms})
}

pub fn pcr(base: u64, extension: u16) -> Value {
    json!({"type": "pcr", "pcrBase": base, "pcrExtension": extension})
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("module contains no resources")]
    EmptyModule,
    #[error("module resources have inconsistent identities")]
    MixedModule,
    #[error("resource has no content type")]
    MissingContentType,
    #[error("invalid resource content type: {0}")]
    ContentType(#[from] mime::FromStrError),
    #[error("resource location is not UTF-8: {0}")]
    Location(#[from] std::str::Utf8Error),
    #[error("web-bml cannot represent this event time mode")]
    UnsupportedEventTime,
    #[error("BML control information: {0}")]
    Control(#[from] arib_b24::Error),
    #[error("serializing web-bml message: {0}")]
    Serialize(#[from] serde_json::Error),
}

/// Validated wire message borrowing resource bytes from a received update.
/// Serialize this directly to stream Base64 into the output buffer, without
/// retaining an intermediate encoded copy of each resource.
///
/// The received update must remain alive until serialization finishes:
/// ```compile_fail
/// let message = {
///     let update = arib_b24::transport::ServiceUpdate::Components(Vec::new());
///     viewer_web_bml::prepare(&update).unwrap()
/// };
/// serde_json::to_string(&message).unwrap();
/// ```
pub struct PreparedUpdate<'a>(Representation<'a>);

enum Representation<'a> {
    Module(ModuleMessage<'a>),
    Other(Value),
}

impl Serialize for PreparedUpdate<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match &self.0 {
            Representation::Module(module) => module.serialize(serializer),
            Representation::Other(value) => value.serialize(serializer),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ModuleMessage<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    component_id: u8,
    module_id: u16,
    files: Vec<File<'a>>,
    version: u8,
    data_event_id: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct File<'a> {
    content_location: Option<&'a str>,
    content_type: Value,
    data_base64: Base64<'a>,
}

struct Base64<'a>(&'a [u8]);
impl Serialize for Base64<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&Base64Display::new(self.0, &STANDARD))
    }
}

/// Check representability before writing any bytes to a transport.
pub fn prepare(update: &ServiceUpdate) -> Result<PreparedUpdate<'_>, Error> {
    let ServiceUpdate::Module {
        component,
        resources,
    } = update
    else {
        return Ok(PreparedUpdate(Representation::Other(encode_ref(update)?)));
    };
    let first = resources.first().ok_or(Error::EmptyModule)?;
    if resources.iter().any(|resource| {
        resource.download_id != first.download_id
            || resource.module_id != first.module_id
            || resource.module_version != first.module_version
    }) {
        return Err(Error::MixedModule);
    }
    Ok(PreparedUpdate(Representation::Module(ModuleMessage {
        kind: "moduleDownloaded",
        component_id: component.tag(),
        module_id: first.module_id,
        files: resources.iter().map(file).collect::<Result<_, _>>()?,
        version: first.module_version,
        data_event_id: first.download_id >> 28,
    })))
}

fn bxml(info: BxmlInfo) -> Value {
    let mut result = json!({
        "transmissionFormat": info.transmission_format,
        "entryPointFlag": info.entry_point.is_some(),
    });
    if let Some(entry) = info.entry_point {
        let mut value = json!({
            "autoStartFlag": entry.auto_start,
            "documentResolution": entry.document_resolution,
            "useXML": entry.use_xml,
            "defaultVersionFlag": entry.default_version,
            "independentFlag": entry.independent,
            "styleForTVFlag": entry.style_for_tv,
            "bmlMajorVersion": entry.bml_major_version,
            "bmlMinorVersion": entry.bml_minor_version,
        });
        if let Some((major, minor)) = entry.bxml_version {
            value["bxmlMajorVersion"] = json!(major);
            value["bxmlMinorVersion"] = json!(minor);
        }
        result["entryPointInfo"] = value;
    }
    if let Some(carousel) = info.carousel {
        result["additionalAribCarouselInfo"] = json!({
            "dataEventId": carousel.data_event_id,
            "eventSectionFlag": carousel.event_sections,
            "ondemandRetrievalFlag": carousel.ondemand_retrieval,
            "fileStorableFlag": carousel.file_storable,
            "startPriority": u8::from(carousel.start_priority),
        });
    }
    result
}

fn component(component: DataComponent) -> Value {
    let mut value = json!({
        "pid": component.pid(),
        "componentId": component.tag(),
        "streamType": component.stream_type(),
        "dataComponentId": component.data_component_id(),
    });
    if let Some(info) = component.bxml_info() {
        value["bxmlInfo"] = bxml(info);
    }
    value
}

fn file(resource: &Resource) -> Result<File<'_>, Error> {
    let media_type = resource
        .media_type
        .as_deref()
        .ok_or(Error::MissingContentType)?;
    let media_type = std::str::from_utf8(media_type)?;
    let parsed: mime::Mime = media_type.parse()?;
    let parameters: Vec<_> = parsed
        .params()
        .map(|(attribute, value)| {
            json!({
                "attribute": attribute.as_str(),
                "originalAttribute": attribute.as_str(),
                "value": value.as_str(),
            })
        })
        .collect();
    let location = if resource.mapping == ResourceMapping::Direct {
        None
    } else {
        Some(std::str::from_utf8(&resource.name)?)
    };
    Ok(File {
        content_location: location,
        content_type: json!({
            "type": parsed.type_().as_str(),
            "originalType": parsed.type_().as_str(),
            "subtype": parsed.subtype().as_str(),
            "originalSubtype": parsed.subtype().as_str(),
            "parameters": parameters,
        }),
        data_base64: Base64(&resource.data),
    })
}

/// One decoded update becomes one message for `BMLBrowser.emitMessage`.
pub fn encode(update: ServiceUpdate) -> Result<Value, Error> {
    encode_ref(&update)
}

/// Encode shared receive state without copying module payloads.
pub fn encode_ref(update: &ServiceUpdate) -> Result<Value, Error> {
    Ok(match update {
        ServiceUpdate::Components(components) => json!({
            "type": "pmt",
            "components": components.iter().copied().map(component).collect::<Vec<_>>(),
        }),
        ServiceUpdate::Broadcasters(information) => json!({
            "type": "bit",
            "originalNetworkId": information.original_network_id,
            "broadcasters": information.broadcasters.iter().map(|broadcaster| json!({
                "broadcasterId": broadcaster.id,
                "broadcasterName": null,
                "services": broadcaster.services.iter().map(|service| json!({
                    "serviceId": service.service_id,
                    "serviceType": service.service_type,
                })).collect::<Vec<_>>(),
                "affiliations": broadcaster.affiliations,
                "affiliationBroadcasters": broadcaster.affiliation_broadcasters.iter()
                    .map(|related| json!({
                        "originalNetworkId": related.original_network_id,
                        "broadcasterId": related.broadcaster_id,
                    })).collect::<Vec<_>>(),
                "terrestrialBroadcasterId": broadcaster.terrestrial_broadcaster_id,
            })).collect::<Vec<_>>(),
        }),
        ServiceUpdate::ModuleList { component, info } => {
            let return_to_entry = info.bxml_return_to_entry()?;
            json!({
                "type": "moduleListUpdated",
                "componentId": component.tag(),
                "modules": info.modules.iter().map(|module| json!({
                    "id": module.id,
                    "version": module.version,
                    "size": module.size,
                })).collect::<Vec<_>>(),
                "dataEventId": info.download_id >> 28,
                "returnToEntryFlag": return_to_entry,
            })
        }
        ServiceUpdate::Module { .. } => serde_json::to_value(prepare(update)?)?,
        ServiceUpdate::Event { component, section } => {
            let events = section
                .messages
                .iter()
                .map(|message| {
                    let common = json!({
                        "eventMessageGroupId": message.group_id,
                        "eventMessageType": message.message_type,
                        "eventMessageId": message.message_id,
                        "privateDataByte": message.private_data,
                    });
                    let mut value = common;
                    match message.time {
                        EventTime::Immediate => {
                            value["type"] = json!("immediateEvent");
                            value["timeMode"] = json!(0);
                        }
                        EventTime::Npt(npt) => {
                            value["type"] = json!("nptEvent");
                            value["timeMode"] = json!(2);
                            value["eventMessageNPT"] = json!(npt);
                        }
                        EventTime::Absolute { .. } | EventTime::RelativeBcd(_) => {
                            return Err(Error::UnsupportedEventTime);
                        }
                    }
                    Ok(value)
                })
                .collect::<Result<Vec<_>, _>>()?;
            json!({
                "type": "esEventUpdated",
                "componentId": component.tag(),
                "events": events,
                "dataEventId": section.data_event_id,
            })
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use arib_b24::transport::{
        bit::{Broadcaster, BroadcasterInformation},
        data_components_from_pmt,
    };

    #[test]
    fn module_list_preserves_bxml_forced_entry_control() {
        for (control, expected) in [
            (None, Value::Null),
            (Some(0x7f), json!(false)),
            (Some(0xff), json!(true)),
        ] {
            let info = arib_b24::DownloadInfo {
                transaction_id: 1,
                download_id: 0x30000001,
                block_size: 4066,
                modules: Vec::new(),
                descriptors: control
                    .into_iter()
                    .map(|byte| arib_b24::Descriptor {
                        tag: 0xf0,
                        data: vec![byte],
                    })
                    .collect(),
            };
            let message = encode(ServiceUpdate::ModuleList {
                component: component(),
                info,
            })
            .unwrap();
            assert_eq!(message["returnToEntryFlag"], expected);
            assert_eq!(message["dataEventId"], 3);
        }
    }

    #[test]
    fn program_clock_and_event_fields_keep_their_units() {
        let info = ProgramInfo {
            service_id: 42,
            original_network_id: Some(1),
            transport_stream_id: Some(2),
            event: Some(ProgramEvent {
                id: 3,
                name: "ニュース".into(),
                start_unix_ms: Some(1_700_000_000_000),
                duration_seconds: Some(30),
            }),
        };
        let message = program(&info);
        assert_eq!(message["eventId"], 3);
        assert_eq!(message["transportStreamId"], 2);
        assert_eq!(message["startTimeUnixMillis"], 1_700_000_000_000_i64);
        assert_eq!(message["durationSeconds"], 30);
        assert_eq!(message["indefiniteDuration"], false);
        assert_eq!(current_time(1234)["timeUnixMillis"], 1234);
        assert_eq!(pcr(1u64 << 32, 299)["pcrBase"], 1u64 << 32);
        assert_eq!(pcr(1u64 << 32, 299)["pcrExtension"], 299);
    }

    #[test]
    fn bit_message_supplies_affiliation_to_web_bml() {
        let message = encode(ServiceUpdate::Broadcasters(BroadcasterInformation {
            original_network_id: 0x7fd5,
            broadcasters: vec![Broadcaster {
                id: 255,
                services: Vec::new(),
                affiliations: vec![2],
                affiliation_broadcasters: Vec::new(),
                terrestrial_broadcaster_id: Some(0x7fd5),
            }],
        }))
        .unwrap();
        assert_eq!(message["type"], "bit");
        assert_eq!(message["originalNetworkId"], 0x7fd5);
        assert_eq!(message["broadcasters"][0]["affiliations"], json!([2]));
        assert_eq!(
            message["broadcasters"][0]["terrestrialBroadcasterId"],
            0x7fd5
        );
    }

    fn component() -> DataComponent {
        let mut bytes = vec![
            0x02, 0xb0, 0x1d, 0x00, 0x2a, 0xc1, 0x00, 0x00, 0xe1, 0x00, 0xf0, 0x00, 0x0d, 0xe1,
            0x01, 0xf0, 0x0b, 0x52, 0x01, 0x40, 0xfd, 0x06, 0x00, 0x0c, 0x33, 0x70, 0xf8, 0xa0,
        ];
        let mut crc = u32::MAX;
        for byte in &bytes {
            crc ^= u32::from(*byte) << 24;
            for _ in 0..8 {
                crc = (crc << 1)
                    ^ if crc & 0x8000_0000 != 0 {
                        0x04c1_1db7
                    } else {
                        0
                    };
            }
        }
        bytes.extend_from_slice(&crc.to_be_bytes());
        data_components_from_pmt(&bytes, 42).unwrap()[0]
    }

    #[test]
    fn program_info_unblocks_browser_startup_without_inventing_event_metadata() {
        let message = program_info(2088, None);
        assert_eq!(message["type"], "programInfo");
        assert_eq!(message["serviceId"], 2088);
        assert!(message["originalNetworkId"].is_null());
        assert!(message["eventId"].is_null());
        assert!(message["eventName"].is_null());
        let identified = program_info(2088, Some(0x7fd5));
        assert_eq!(identified["originalNetworkId"], 0x7fd5);
    }

    #[test]
    fn component_message_contains_browser_entry_metadata() {
        let message = encode(ServiceUpdate::Components(vec![component()])).unwrap();
        assert_eq!(message["type"], "pmt");
        assert_eq!(message["components"][0]["componentId"], 0x40);
        assert_eq!(message["components"][0]["streamType"], 0x0d);
        assert_eq!(
            message["components"][0]["bxmlInfo"]["entryPointInfo"]["documentResolution"],
            3
        );
    }

    #[test]
    fn module_files_arrive_together_with_original_bytes() {
        let first = Resource {
            mapping: ResourceMapping::Entity,
            component_tag: Some(0x40),
            download_id: 0xf123_4567,
            module_id: 7,
            module_version: 2,
            module_name: b"module".to_vec(),
            name: b"a.bml".to_vec(),
            media_type: Some(b"text/bml".to_vec()),
            data: b"<bml/>".to_vec(),
        };
        let mut second = first.clone();
        second.name = b"image.png".to_vec();
        second.media_type = Some(b"image/png".to_vec());
        second.data = vec![0, 255, 1];
        let message = encode(ServiceUpdate::Module {
            component: component(),
            resources: vec![first, second],
        })
        .unwrap();
        assert_eq!(message["type"], "moduleDownloaded");
        assert_eq!(message["dataEventId"], 15);
        assert_eq!(message["files"].as_array().unwrap().len(), 2);
        assert_eq!(message["files"][0]["contentLocation"], "a.bml");
        assert_eq!(message["files"][0]["dataBase64"], "PGJtbC8+");
        assert_eq!(message["files"][1]["dataBase64"], "AP8B");
    }

    #[test]
    fn direct_serialization_preserves_binary_data_escaping_and_mime_parameters() {
        use base64::Engine as _;
        let data: Vec<u8> = (0..8193).map(|value| value as u8).collect();
        let resource = Resource {
            mapping: ResourceMapping::Entity,
            component_tag: Some(0x40),
            download_id: 1,
            module_id: 7,
            module_version: 0,
            module_name: b"module".to_vec(),
            name: "資料/\"\\\n.bml".as_bytes().to_vec(),
            media_type: Some(b"text/bml; charset=EUC-JP".to_vec()),
            data: data.clone(),
        };
        let mut update = ServiceUpdate::Module {
            component: component(),
            resources: vec![resource],
        };
        for mapping in [ResourceMapping::Entity, ResourceMapping::Direct] {
            let ServiceUpdate::Module { resources, .. } = &mut update else {
                unreachable!()
            };
            resources[0].mapping = mapping;
            let encoded = serde_json::to_string(&prepare(&update).unwrap()).unwrap();
            let message: Value = serde_json::from_str(&encoded).unwrap();
            assert_eq!(message, encode_ref(&update).unwrap());
            let file = &message["files"][0];
            assert_eq!(
                STANDARD
                    .decode(file["dataBase64"].as_str().unwrap())
                    .unwrap(),
                data
            );
            assert_eq!(
                file["contentLocation"],
                match mapping {
                    ResourceMapping::Entity => json!("資料/\"\\\n.bml"),
                    ResourceMapping::Direct => Value::Null,
                }
            );
            assert_eq!(file["contentType"]["parameters"][0]["attribute"], "charset");
            assert_eq!(file["contentType"]["parameters"][0]["value"], "euc-jp");
        }
        let ServiceUpdate::Module { resources, .. } = &mut update else {
            unreachable!()
        };
        let mut invalid = resources[0].clone();
        invalid.media_type = None;
        resources.push(invalid);
        assert!(matches!(prepare(&update), Err(Error::MissingContentType)));
    }

    #[test]
    fn single_entity_keeps_its_location_when_it_matches_the_module_name() {
        let mut resource = Resource {
            mapping: ResourceMapping::Entity,
            component_tag: Some(0x40),
            download_id: 1,
            module_id: 7,
            module_version: 0,
            module_name: b"startup.bml".to_vec(),
            name: b"startup.bml".to_vec(),
            media_type: Some(b"text/bml".to_vec()),
            data: vec![],
        };
        for mapping in [ResourceMapping::Entity, ResourceMapping::Direct] {
            resource.mapping = mapping;
            let message = encode(ServiceUpdate::Module {
                component: component(),
                resources: vec![resource.clone()],
            })
            .unwrap();
            let expected = match mapping {
                ResourceMapping::Entity => json!("startup.bml"),
                ResourceMapping::Direct => Value::Null,
            };
            assert_eq!(message["files"][0]["contentLocation"], expected);
        }
    }
}
