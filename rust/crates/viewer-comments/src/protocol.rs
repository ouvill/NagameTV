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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    #[default]
    History,
    Live,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Origin {
    #[serde(rename = "ニコ実")]
    Niconico,
    #[serde(rename = "NX")]
    Nx,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Position {
    #[default]
    Right,
    Top,
    Bottom,
}

impl Position {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Right => "right",
            Self::Top => "top",
            Self::Bottom => "bottom",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Style {
    pub position: Position,
    pub color: u32,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            position: Position::Right,
            color: 0xffffff,
        }
    }
}

impl Style {
    // NX-Jikkyo/Niconico mail commands. Ignore unknown commands, never HTML/CSS.
    fn from_mail(mail: &str) -> Self {
        let mut style = Self::default();
        for command in mail.split_ascii_whitespace() {
            match command {
                "naka" => style.position = Position::Right,
                "ue" => style.position = Position::Top,
                "shita" => style.position = Position::Bottom,
                _ => {
                    let color = match command {
                        "white" => Some(0xffffff),
                        "red" => Some(0xff0000),
                        "pink" => Some(0xff8080),
                        "orange" => Some(0xffcc00),
                        "yellow" => Some(0xffff00),
                        "green" => Some(0x00ff00),
                        "cyan" => Some(0x00ffff),
                        "blue" => Some(0x0000ff),
                        "purple" => Some(0xc000ff),
                        "black" => Some(0x000000),
                        "white2" => Some(0xcccc99),
                        "red2" => Some(0xcc0033),
                        "pink2" => Some(0xff33cc),
                        "orange2" => Some(0xff6600),
                        "yellow2" => Some(0x999900),
                        "green2" => Some(0x00cc66),
                        "cyan2" => Some(0x00cccc),
                        "blue2" => Some(0x3399ff),
                        "purple2" => Some(0x6633cc),
                        "black2" => Some(0x666666),
                        _ => command
                            .strip_prefix('#')
                            .filter(|hex| {
                                hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit())
                            })
                            .and_then(|hex| u32::from_str_radix(hex, 16).ok()),
                    };
                    if let Some(color) = color {
                        style.color = color;
                    }
                }
            }
        }
        style
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Comment {
    #[serde(skip)]
    pub identity: Option<CommentIdentity>,
    pub text: Box<str>,
    pub origin: Origin,
    pub phase: Phase,
    pub unix_seconds: u64,
    #[serde(skip)]
    pub timestamp_micros: Option<u64>,
    #[serde(skip)]
    pub source_id: Option<(u64, u64)>,
    pub style: Style,
}

/// Wire identity used to recognize an echo of this app's explicit post.
/// It is never projected into the UI's serialized comment history.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentIdentity {
    pub thread_id: u64,
    pub user_id: Box<str>,
    pub vpos: u64,
}

// Identity is optional decoration. An unsupported representation must not
// discard an otherwise displayable comment (for example, a negative vpos).
pub(crate) fn optional_id<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<u64>, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(value.as_u64().or_else(|| value.as_str()?.parse().ok()))
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
    #[serde(default, deserialize_with = "optional_id")]
    thread: Option<u64>,
    #[serde(default, deserialize_with = "optional_id")]
    vpos: Option<u64>,
    #[serde(borrow)]
    content: Cow<'a, str>,
    #[serde(default, borrow)]
    user_id: Cow<'a, str>,
    #[serde(default, deserialize_with = "timestamp")]
    date: Option<u64>,
    #[serde(default, deserialize_with = "timestamp")]
    date_usec: Option<u64>,
    #[serde(default, deserialize_with = "optional_id")]
    no: Option<u64>,
    #[serde(default, borrow)]
    mail: Cow<'a, str>,
}

fn timestamp<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<u64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Number {
        Integer(u64),
        Text(String),
    }
    Option::<Number>::deserialize(deserializer)?
        .map(|value| match value {
            Number::Integer(value) => Ok(value),
            Number::Text(value) => value.parse().map_err(serde::de::Error::custom),
        })
        .transpose()
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
            identity: chat.thread.zip(chat.vpos).and_then(|(thread_id, vpos)| {
                (!chat.user_id.is_empty() && chat.user_id.len() <= 256).then(|| CommentIdentity {
                    thread_id,
                    vpos,
                    user_id: chat.user_id.into_owned().into_boxed_str(),
                })
            }),
            text: chat.content.into_owned().into_boxed_str(),
            origin,
            phase: self.phase,
            unix_seconds: chat.date.unwrap_or(0),
            timestamp_micros: chat
                .date
                .and_then(|seconds| seconds.checked_mul(1_000_000))
                .zip(
                    chat.date_usec
                        .or(Some(0))
                        .filter(|micros| *micros < 1_000_000),
                )
                .and_then(|(seconds, micros)| seconds.checked_add(micros)),
            source_id: chat.thread.zip(chat.no),
            style: Style::from_mail(&chat.mail),
        }))
    }
}

#[cfg(test)]
mod tests;
