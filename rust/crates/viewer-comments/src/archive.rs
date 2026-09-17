//! Read-only kakolog import. Network limits are enforced before this boundary.
//! Wire documentation: https://jikkyo.tsukumijima.net/
use crate::{Comment, Decoder, Event, Origin};
use serde::Deserialize;
pub const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_COMMENTS: usize = 20_000;
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("過去ログの解析に失敗しました: {0}")]
    Json(#[from] serde_json::Error),
    #[error("過去ログ応答が上限を超えています")]
    TooLarge,
    #[error("過去ログ API: {0}")]
    Remote(String),
}
pub fn parse(bytes: &[u8]) -> Result<Vec<Comment>, Error> {
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(Error::TooLarge);
    }
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Response {
        Success { packet: Vec<serde_json::Value> },
        Failure { error: String },
    }
    let entries = match serde_json::from_slice(bytes)? {
        Response::Success { packet } => packet,
        Response::Failure { error } => {
            return Err(Error::Remote(error.chars().take(256).collect()));
        }
    };
    if entries.len() > MAX_COMMENTS {
        return Err(Error::TooLarge);
    }
    let mut decoder = Decoder::default();
    let mut comments = Vec::new();
    for entry in entries {
        let Some(chat) = entry.get("chat") else {
            continue;
        };
        let flag = |key| {
            chat.get(key).is_some_and(|value| {
                value.as_u64().is_some_and(|value| value != 0)
                    || value.as_str().is_some_and(|value| value != "0")
            })
        };
        if flag("deleted")
            || chat
                .get("content")
                .and_then(|value| value.as_str())
                .is_some_and(|text| text.starts_with('/'))
        {
            continue;
        }
        let bytes = serde_json::to_vec(&entry)?;
        let Ok(Event::Comment(mut comment)) = decoder.decode(&bytes) else {
            continue;
        };
        if comment.timestamp_micros.is_none() {
            continue;
        }
        comment.origin = if flag("nx_jikkyo") {
            Origin::Nx
        } else {
            Origin::Niconico
        };
        comments.push(comment);
    }
    comments.sort_by_key(|comment| comment.timestamp_micros);
    Ok(comments)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn archive_requires_real_timestamps_preserves_microseconds_and_ignores_commands() {
        let comments = parse(br#"{"packet":[
            {"chat":{"content":"later","date":"100","date_usec":"500000","thread":"5","no":"1","nx_jikkyo":"1"}},
            {"chat":{"content":"first","date":100,"date_usec":1234}},
            {"chat":{"content":"missing"}},
            {"chat":{"content":"invalid","date":100,"date_usec":1000000}},
            {"chat":{"content":"/nicoad test","date":100}},
            {"chat":{"content":"deleted","date":100,"deleted":"1"}}
        ]}"#).unwrap();
        assert_eq!(comments.len(), 2);
        assert_eq!(comments[0].timestamp_micros, Some(100_001_234));
        assert_eq!(comments[0].origin, Origin::Niconico);
        assert_eq!(comments[1].origin, Origin::Nx);
        assert_eq!(comments[1].source_id, Some((5, 1)));
        assert!(parse(br#"{"packet":[]}"#).unwrap().is_empty());
        assert!(matches!(
            parse(br#"{"error":"unavailable"}"#),
            Err(Error::Remote(_))
        ));
        assert!(parse(b"{}").is_err());
    }
}
