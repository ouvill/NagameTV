//! A guide action is revalidated against the current server generation and schedule.
use super::{
    ProgramInfo,
    model::Snapshot,
    schedule::{End, Resolution, Segment},
};
use crate::channels::{BroadcastService, Channel};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, PartialEq, Eq)]
enum Target {
    Program { id: u64, start: u64, duration: u64 },
    Channel,
}
#[derive(Deserialize, Serialize)]
pub(super) struct Identity {
    generation: u64,
    endpoint: u64,
    service: BroadcastService,
    start: u64,
    end: u64,
    target: Target,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Program,
    Channel,
}
impl Identity {
    pub(super) fn segment(
        generation: u64,
        channel: &Channel,
        segment: &Segment,
        snapshot: &Snapshot,
    ) -> Self {
        Self {
            generation,
            endpoint: channel.id,
            service: channel
                .broadcast
                .expect("segment requires a broadcast service"),
            start: segment.start,
            end: segment.end,
            target: target(snapshot, segment),
        }
    }
    pub(super) fn key(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}
fn target(snapshot: &Snapshot, segment: &Segment) -> Target {
    match segment.resolution {
        Resolution::Single(index) => {
            let p = snapshot.program(index);
            match p.end() {
                End::Known(_) => Target::Program {
                    id: p.id,
                    start: p.start_at,
                    duration: p.duration,
                },
                End::Unknown => Target::Channel,
            }
        }
        Resolution::Conflict(_) | Resolution::Gap => Target::Channel,
    }
}
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("番組情報が更新されています。番組表から選び直してください")]
    Unavailable,
    #[error("この番組は現在放送されていません")]
    NotLive,
}
pub(super) fn resolve(
    snapshot: &Snapshot,
    generation: u64,
    key: &str,
    channels: &[Channel],
    now: u64,
) -> Result<(usize, Action), Error> {
    if key.len() > 512 {
        return Err(Error::Unavailable);
    }
    let identity: Identity = serde_json::from_str(key).map_err(|_| Error::Unavailable)?;
    if identity.generation != generation {
        return Err(Error::Unavailable);
    }
    let index = channels
        .iter()
        .position(|c| c.id == identity.endpoint && c.broadcast == Some(identity.service))
        .ok_or(Error::Unavailable)?;
    let segment = snapshot
        .segment(Some(identity.service), now)
        .ok_or(Error::NotLive)?;
    if segment.start != identity.start
        || segment.end != identity.end
        || target(snapshot, segment) != identity.target
    {
        return Err(Error::NotLive);
    }
    Ok((
        index,
        match identity.target {
            Target::Program { .. } => Action::Program,
            Target::Channel => Action::Channel,
        },
    ))
}
impl ProgramInfo {
    pub fn watch_channel(&self, key: &str, channels: &[Channel], now: u64) -> Result<usize, Error> {
        resolve(&self.snapshot, self.generation, key, channels, now).map(|(index, _)| index)
    }
}
