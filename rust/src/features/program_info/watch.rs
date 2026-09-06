//! Resolve a guide selection against current broadcast data, never an old row index.
use super::{ProgramInfo, model::Program};
use crate::channels::{BroadcastService, Channel};
use serde::{Deserialize, Serialize, Serializer};

#[derive(Deserialize, Serialize)]
pub(super) struct Identity {
    endpoint: u64,
    service: BroadcastService,
    program: u64,
    start: u64,
    duration: u64,
}
impl Identity {
    pub(super) fn new(endpoint: u64, program: &Program) -> Self {
        Self {
            endpoint,
            service: program.service(),
            program: program.id,
            start: program.start_at,
            duration: program.duration,
        }
    }
    pub(super) fn serialize_key<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Qt treats this as an opaque string; u64 IDs never pass through a JS number.
        let key = serde_json::to_string(self).map_err(serde::ser::Error::custom)?;
        serializer.serialize_str(&key)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("番組情報が更新されています。番組表から選び直してください")]
    Unavailable,
    #[error("この番組は現在放送されていません")]
    NotLive,
}
impl ProgramInfo {
    pub fn watch_channel(&self, key: &str, channels: &[Channel], now: u64) -> Result<usize, Error> {
        if key.len() > 512 {
            return Err(Error::Unavailable);
        }
        let identity: Identity = serde_json::from_str(key).map_err(|_| Error::Unavailable)?;
        let index = channels
            .iter()
            .position(|channel| {
                channel.id == identity.endpoint && channel.broadcast == Some(identity.service)
            })
            .ok_or(Error::Unavailable)?;
        let program = self
            .snapshot
            .current(Some(identity.service), now)
            .ok_or(Error::NotLive)?;
        if program.id != identity.program
            || program.start_at != identity.start
            || program.duration != identity.duration
        {
            return Err(Error::NotLive);
        }
        Ok(index)
    }
}
