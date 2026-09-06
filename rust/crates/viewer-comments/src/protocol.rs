use serde::{Deserialize, Serialize};
use std::borrow::Cow;

pub const MAX_MESSAGE_BYTES: usize = 64 * 1024;
pub const MAX_COMMENT_BYTES: usize = 4096;
pub const MAX_THREAD_LIST_BYTES: usize = 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("実況応答がサイズ上限 ({limit} bytes) を超えています")]
    TooLarge { limit: usize },
    #[error("実況応答の解析に失敗しました: {0}")]
    Json(#[from] serde_json::Error),
    #[error("放送中の実況スレッドがありません")]
    NoActiveThread,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThreadId(u64);

impl ThreadId {
    pub fn active_in(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_THREAD_LIST_BYTES {
            return Err(Error::TooLarge {
                limit: MAX_THREAD_LIST_BYTES,
            });
        }
        #[derive(Deserialize)]
        enum Status {
            #[serde(rename = "ACTIVE")]
            Active,
            #[serde(other)]
            Other,
        }
        #[derive(Deserialize)]
        struct Thread {
            id: u64,
            status: Status,
        }
        let threads: Vec<Thread> = serde_json::from_slice(bytes)?;
        threads
            .into_iter()
            .find(|thread| matches!(thread.status, Status::Active))
            .map(|thread| Self(thread.id))
            .ok_or(Error::NoActiveThread)
    }

    /// Subscribe to the last 100 comments, then live updates. Never posts a chat.
    pub fn subscription(self) -> Result<String, Error> {
        Ok(serde_json::to_string(&serde_json::json!([
            {"ping":{"content":"rs:0"}},
            {"ping":{"content":"ps:0"}},
            {"thread":{"thread":self.0.to_string(),"version":"20061206","user_id":"guest",
                "res_from":-100,"with_global":1,"scores":1,"nicoru":0}},
            {"ping":{"content":"pf:0"}},
            {"ping":{"content":"rf:0"}}
        ]))?)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub enum Phase {
    #[default]
    History,
    Live,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Origin {
    #[serde(rename = "ニコ実")]
    Niconico,
    #[serde(rename = "NX")]
    Nx,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct Comment {
    pub text: Box<str>,
    pub origin: Origin,
    pub phase: Phase,
    pub unix_seconds: u64,
}

impl Comment {
    pub fn japan_time(&self) -> String {
        // Reduce first: an untrusted u64 timestamp must not overflow on +9h.
        let seconds = (self.unix_seconds % 86_400 + 9 * 3600) % 86_400;
        format!(
            "{:02}:{:02}:{:02}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60
        )
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Event {
    Ignored,
    HistoryComplete,
    Comment(Comment),
}

/// One decoder per connection. Reconnecting must create a fresh History phase.
#[derive(Default)]
pub struct Decoder {
    phase: Phase,
}

#[derive(Deserialize)]
struct Envelope<'a> {
    #[serde(borrow)]
    ping: Option<Ping<'a>>,
    #[serde(borrow)]
    chat: Option<Chat<'a>>,
}
#[derive(Deserialize)]
struct Ping<'a> {
    #[serde(borrow)]
    content: Cow<'a, str>,
}
#[derive(Deserialize)]
struct Chat<'a> {
    #[serde(borrow)]
    content: Cow<'a, str>,
    #[serde(default, borrow)]
    user_id: Cow<'a, str>,
    #[serde(default)]
    date: u64,
}

impl Decoder {
    pub fn decode(&mut self, bytes: &[u8]) -> Result<Event, Error> {
        if bytes.len() > MAX_MESSAGE_BYTES {
            return Err(Error::TooLarge {
                limit: MAX_MESSAGE_BYTES,
            });
        }
        let envelope: Envelope<'_> = serde_json::from_slice(bytes)?;
        if envelope.ping.is_some_and(|ping| ping.content == "rf:0") {
            if self.phase == Phase::History {
                self.phase = Phase::Live;
                return Ok(Event::HistoryComplete);
            }
            return Ok(Event::Ignored);
        }
        let Some(chat) = envelope.chat else {
            return Ok(Event::Ignored);
        };
        if chat.content.is_empty() || chat.content.len() > MAX_COMMENT_BYTES {
            return Ok(Event::Ignored);
        }
        let origin = if chat.user_id.starts_with("nicolive:") || chat.user_id.starts_with("rekari:")
        {
            Origin::Niconico
        } else {
            Origin::Nx
        };
        Ok(Event::Comment(Comment {
            text: chat.content.into_owned().into_boxed_str(),
            origin,
            phase: self.phase,
            unix_seconds: chat.date,
        }))
    }
}

#[cfg(test)]
mod tests;
