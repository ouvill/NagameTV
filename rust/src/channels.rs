//! Service decoding and channel ordering. No Qt, IO, EPG or playback resources.
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("チャンネルJSONの解析失敗: {0}")]
    Json(#[from] serde_json::Error),
    #[error("TVチャンネルがありません")]
    NoTvChannels,
}

/// Declaration order is the channel browser's broadcast priority.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Band {
    #[serde(rename = "GR")]
    Terrestrial,
    #[serde(rename = "BS")]
    Bs,
    #[serde(rename = "CS")]
    Cs,
    #[serde(rename = "SKY")]
    Sky,
    #[default]
    #[serde(other, rename = "OTHER")]
    Other,
}

/// Broadcast metadata identifies EPG schedules independently of endpoint IDs.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct BroadcastService {
    pub network_id: u16,
    pub service_id: u16,
}

#[derive(Debug)]
pub struct Channel {
    pub id: u64,
    pub name: String,
    pub label: String,
    pub band: Band,
    pub broadcast: Option<BroadcastService>,
    number: Option<u16>,
    service_id: Option<u16>,
}

// Keep the wire schema separate from the validated, display-ready domain model.
// Optional metadata supports services from older/minimal servers without guessing
// a service ID from Mirakurun's opaque endpoint ID.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Service {
    id: u64,
    name: String,
    #[serde(rename = "type")]
    kind: u32,
    service_id: Option<u16>,
    network_id: Option<u16>,
    remote_control_key_id: Option<u16>,
    #[serde(default)]
    channel: ServiceChannel,
}

#[derive(Default, Deserialize)]
struct ServiceChannel {
    #[serde(rename = "type", default)]
    band: Band,
}

pub fn parse(bytes: &[u8]) -> Result<Vec<Channel>, Error> {
    let services: Vec<Service> = serde_json::from_slice(bytes)?;
    let mut seen = HashSet::new();
    let mut channels: Vec<_> = services
        .into_iter()
        .filter(|s| s.kind == 1 && s.id != 0 && !s.name.trim().is_empty() && seen.insert(s.id))
        .map(|s| {
            let terrestrial = s.channel.band == Band::Terrestrial;
            let number = if terrestrial {
                s.remote_control_key_id
            } else {
                s.service_id
            }
            .filter(|number| *number != 0);
            let label = match number {
                Some(number) if terrestrial => format!("{number:02}   {}", s.name),
                Some(number) => format!("{number:03}   {}", s.name),
                None => format!("--   {}", s.name),
            };
            Channel {
                id: s.id,
                name: s.name,
                label,
                band: s.channel.band,
                broadcast: s
                    .network_id
                    .zip(s.service_id)
                    .map(|(network_id, service_id)| BroadcastService {
                        network_id,
                        service_id,
                    }),
                number,
                service_id: s.service_id,
            }
        })
        .collect();
    if channels.is_empty() {
        return Err(Error::NoTvChannels);
    }
    // Same primary order as main. Compare borrowed strings, never allocate labels
    // in the comparator. Endpoint ID breaks otherwise identical ties independently
    // of the server's response order. Missing/zero numbers follow numbered channels.
    channels.sort_unstable_by(|a, b| {
        (
            a.band,
            a.number.is_none(),
            a.number,
            &a.label,
            a.service_id,
            a.id,
        )
            .cmp(&(
                b.band,
                b.number.is_none(),
                b.number,
                &b.label,
                b.service_id,
                b.id,
            ))
    });
    Ok(channels)
}

#[cfg(test)]
mod tests;
