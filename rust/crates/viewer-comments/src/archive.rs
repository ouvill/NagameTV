//! Incremental kakolog import. The response is read from disk, never collected
//! in memory. Only an individual protocol message has a working-buffer limit.
//! Wire documentation: https://jikkyo.tsukumijima.net/
use crate::{Comment, Decoder, Event, Origin};
use std::io::Read;
mod json;
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("過去ログの解析に失敗しました: {0}")]
    Json(#[from] serde_json::Error),
    #[error("過去ログの読込みに失敗しました: {0}")]
    Io(#[from] std::io::Error),
    #[error("過去ログのJSON形式が不正です: {0}")]
    Syntax(&'static str),
    #[error("過去ログ API: {0}")]
    Remote(String),
}
#[derive(Debug, thiserror::Error)]
pub enum VisitError<E> {
    #[error("{0}")]
    Input(#[from] Error),
    #[error("{0}")]
    Sink(E),
}

/// A visitor must stage its writes: success is known only after the closing
/// object and EOF, including an `error` field following the packet array.
pub fn visit<R: Read, E>(
    reader: R,
    mut sink: impl FnMut(Comment) -> Result<(), E>,
) -> Result<u64, VisitError<E>> {
    let mut json = json::Reader::new(reader);
    json.expect(b'{')?;
    let mut seen_packet = false;
    let mut remote = None;
    let mut count = 0;
    let mut decoder = Decoder::default();
    if !json.consume(b'}')? {
        loop {
            let key = json.key()?;
            json.expect(b':')?;
            match key.as_deref() {
                Some("packet") => {
                    if seen_packet {
                        return Err(Error::Syntax("duplicate packet").into());
                    }
                    seen_packet = true;
                    json.expect(b'[')?;
                    if !json.consume(b']')? {
                        loop {
                            if let Some(bytes) = json.captured_value()?
                                && let Some(comment) = decode(&bytes, &mut decoder)?
                            {
                                sink(comment).map_err(VisitError::Sink)?;
                                count += 1;
                            }
                            if json.consume(b']')? {
                                break;
                            }
                            json.expect(b',')?;
                        }
                    }
                }
                Some("error") => {
                    remote = Some(
                        json.captured_value()?
                            .and_then(|bytes| serde_json::from_slice::<String>(&bytes).ok())
                            .map(|message| message.chars().take(256).collect())
                            .unwrap_or_else(|| "invalid error response".into()),
                    );
                }
                _ => json.skip_value()?,
            }
            if json.consume(b'}')? {
                break;
            }
            json.expect(b',')?;
        }
    }
    json.end()?;
    if let Some(error) = remote {
        return Err(Error::Remote(error).into());
    }
    if !seen_packet {
        return Err(Error::Syntax("missing packet").into());
    }
    Ok(count)
}

fn decode(bytes: &[u8], decoder: &mut Decoder) -> Result<Option<Comment>, Error> {
    let entry: serde_json::Value = serde_json::from_slice(bytes)?;
    let Some(chat) = entry.get("chat") else {
        return Ok(None);
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
        return Ok(None);
    }
    let Ok(Event::Comment(mut comment)) = decoder.decode(bytes) else {
        return Ok(None);
    };
    if comment.timestamp_micros.is_none() {
        return Ok(None);
    }
    comment.origin = if flag("nx_jikkyo") {
        Origin::Nx
    } else {
        Origin::Niconico
    };
    Ok(Some(comment))
}

#[cfg(test)]
fn parse(bytes: &[u8]) -> Result<Vec<Comment>, VisitError<std::convert::Infallible>> {
    let mut comments = Vec::new();
    visit(bytes, |comment| {
        comments.push(comment);
        Ok(())
    })?;
    comments.sort_by_key(|comment| comment.timestamp_micros);
    Ok(comments)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn oversized_single_entries_and_unknown_fields_do_not_accumulate_or_discard_other_comments() {
        let huge = "あ".repeat(crate::MAX_MESSAGE_BYTES);
        let json = format!(
            r#"{{"unused":"{huge}","packet":[{{"chat":{{"content":"{huge}","date":100}}}},{{"chat":{{"content":"kept","date":101}}}}]}}"#
        );
        let comments = parse(json.as_bytes()).unwrap();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].text.as_ref(), "kept");
        assert!(parse(br#"{"packet":[],"packet":[]}"#).is_err());
        assert!(parse(br#"{"packet":[]} trailing"#).is_err());
    }
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
            Err(VisitError::Input(Error::Remote(_)))
        ));
        assert!(parse(b"{}").is_err());
    }
}
