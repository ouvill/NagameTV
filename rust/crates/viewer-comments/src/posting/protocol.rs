use crate::MAX_COMMENT_BYTES;
use crate::termination::Termination;
use serde::Deserialize;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Comment is empty")]
    Empty,
    #[error("Comment exceeds {MAX_COMMENT_BYTES} UTF-8 bytes")]
    TooLong,
    #[error("WebSocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("Posting timed out")]
    Timeout,
    #[error("Connection closed")]
    Closed,
    #[error("{0}")]
    Terminated(#[from] Termination),
    #[error("Invalid posting response: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Invalid server timestamp: {0}")]
    Timestamp(#[from] time::error::Parse),
    #[error("Invalid posting session")]
    Session,
    #[error("Server rejected the request: {0}")]
    Rejected(String),
    #[error("Posting worker stopped: {0}")]
    Worker(#[source] tokio::task::JoinError),
    #[error("{0}")]
    Blocked(#[from] crate::service::Blocked),
}

pub fn validate_text(text: &str) -> Result<(), Error> {
    if text.trim().is_empty() {
        return Err(Error::Empty);
    }
    if text.len() > MAX_COMMENT_BYTES {
        return Err(Error::TooLong);
    }
    Ok(())
}

#[derive(Deserialize)]
// Tag the envelope: unhandled messages may carry arbitrary `data`. With an
// adjacently tagged enum, serde's unit `other` variant rejects map payloads.
#[serde(tag = "type")]
pub(super) enum Event {
    #[serde(rename = "serverTime")]
    Time { data: ServerTime },
    #[serde(rename = "room")]
    Room { data: Room },
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "postCommentResult")]
    Result { data: PostResult },
    #[serde(rename = "error")]
    Error { data: ServerError },
    #[serde(rename = "disconnect")]
    Disconnect { data: Disconnect },
    #[serde(other)]
    Ignore,
}

#[derive(Deserialize)]
pub(super) struct ServerTime {
    #[serde(rename = "currentMs", alias = "serverTime")]
    pub current: String,
}

#[derive(Deserialize)]
pub(super) struct Room {
    #[serde(rename = "vposBaseTime")]
    pub base: String,
    #[serde(
        default,
        rename = "threadId",
        deserialize_with = "crate::protocol::optional_id"
    )]
    pub thread_id: Option<u64>,
    #[serde(default, rename = "yourPostKey")]
    pub your_post_key: Option<String>,
}

impl Room {
    pub fn identity(&self, vpos: u64) -> Option<crate::CommentIdentity> {
        let key = self.your_post_key.as_ref()?;
        if key.is_empty() || key.len() > 256 {
            return None;
        }
        Some(crate::CommentIdentity {
            thread_id: self.thread_id?,
            user_id: key.as_str().into(),
            vpos,
        })
    }
}

#[derive(Deserialize)]
pub(super) struct PostResult {
    pub chat: Receipt,
}

#[derive(Deserialize)]
pub(super) struct ServerError {
    #[serde(alias = "code")]
    pub message: String,
}

#[derive(Deserialize)]
pub(super) struct Disconnect {
    pub reason: Termination,
}

#[derive(Deserialize)]
pub(super) struct Receipt {
    pub content: String,
    pub restricted: bool,
}

pub(super) fn timestamp(value: &str) -> Result<i128, Error> {
    Ok(OffsetDateTime::parse(value, &Rfc3339)?.unix_timestamp_nanos() / 1_000_000)
}

pub(super) fn vpos(now_ms: i128, base_ms: i128) -> Result<u64, Error> {
    now_ms
        .checked_sub(base_ms)
        .filter(|v| *v >= 0)
        .and_then(|v| u64::try_from(v / 10).ok())
        .ok_or(Error::Session)
}

pub(super) fn request(text: &str, now_ms: i128, base_ms: i128) -> Result<String, Error> {
    let vpos = vpos(now_ms, base_ms)?;
    Ok(serde_json::to_string(&serde_json::json!({
        "type": "postComment",
        "data": {"text": text, "vpos": vpos, "isAnonymous": true,
            "color": "white", "position": "naka", "size": "medium", "font": "defont"}
    }))?)
}
