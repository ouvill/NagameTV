//! Bounded Mirakurun open-array event decoding. No Qt, networking or playback resources.
mod gate;
pub use gate::RefreshGate;
use serde::Deserialize;
use std::borrow::Cow;

pub const MAX_EVENT_BYTES: usize = 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("EPG event exceeds {MAX_EVENT_BYTES} bytes")]
    TooLarge,
    #[error("Expected an EPG event object")]
    Framing,
    #[error("Invalid EPG event JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("The EPG event decoder has failed; reconnect with a new decoder")]
    Failed,
}

#[derive(Default)]
enum State {
    #[default]
    Between,
    Object {
        depth: usize,
        quoted: bool,
        escaped: bool,
    },
    Failed,
}

#[derive(Default)]
pub struct Decoder {
    state: State,
    pending: Vec<u8>,
}
impl Decoder {
    /// True means at least one program changed. A transport failure must also trigger
    /// reconciliation: a chunk containing valid events followed by invalid data returns Err.
    pub fn push(&mut self, bytes: &[u8]) -> Result<bool, Error> {
        match self.decode(bytes) {
            Ok(changed) => Ok(changed),
            Err(error) => {
                self.state = State::Failed;
                self.pending = Vec::new();
                Err(error)
            }
        }
    }
    fn decode(&mut self, bytes: &[u8]) -> Result<bool, Error> {
        if matches!(self.state, State::Failed) {
            return Err(Error::Failed);
        }
        let mut changed = false;
        for &byte in bytes {
            match &mut self.state {
                State::Between => {
                    // Mirakurun leaves the surrounding array open between notifications.
                    // Match main's tolerated separators; serde validates each full object.
                    if byte.is_ascii_whitespace() || matches!(byte, b'[' | b']' | b',') {
                        continue;
                    }
                    if byte != b'{' {
                        return Err(Error::Framing);
                    }
                    self.pending.push(byte);
                    self.state = State::Object {
                        depth: 1,
                        quoted: false,
                        escaped: false,
                    };
                }
                State::Object {
                    depth,
                    quoted,
                    escaped,
                } => {
                    if self.pending.len() == MAX_EVENT_BYTES {
                        return Err(Error::TooLarge);
                    }
                    self.pending.push(byte);
                    if *quoted {
                        if *escaped {
                            *escaped = false;
                        } else if byte == b'\\' {
                            *escaped = true;
                        } else if byte == b'"' {
                            *quoted = false;
                        }
                    } else {
                        match byte {
                            b'"' => *quoted = true,
                            // The event byte limit also bounds depth, preventing integer overflow.
                            b'{' | b'[' => *depth += 1,
                            b'}' | b']' => *depth -= 1,
                            _ => {}
                        }
                    }
                    if *depth == 0 {
                        #[derive(Deserialize)]
                        struct Event<'a> {
                            #[serde(borrow)]
                            resource: Cow<'a, str>,
                            #[serde(borrow, rename = "type")]
                            kind: Cow<'a, str>,
                        }
                        let event: Event<'_> = serde_json::from_slice(&self.pending)?;
                        changed |= event.resource == "program"
                            && matches!(event.kind.as_ref(), "create" | "update" | "remove");
                        // One reusable buffer per connection, never a history of event payloads.
                        self.pending.clear();
                        self.state = State::Between;
                    }
                }
                State::Failed => return Err(Error::Failed),
            }
        }
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    type TestResult = Result<(), Box<dyn std::error::Error>>;
    #[test]
    fn every_byte_split_handles_nested_data_utf8_and_escaped_delimiters() -> TestResult {
        let event = serde_json::to_vec(&serde_json::json!({
            "resource":"program", "type":"update", "data":{"name":"日本語[},\\\"", "items":[{"text":"\\"}]}
        }))?;
        for split in 0..=event.len() {
            let mut decoder = Decoder::default();
            assert!(!decoder.push(b"[\n")?);
            let first = decoder.push(&event[..split])?;
            let second = decoder.push(&event[split..])?;
            assert_eq!(usize::from(first) + usize::from(second), 1);
            assert!(!decoder.push(b",\n")?);
            assert!(decoder.pending.is_empty());
        }
        Ok(())
    }
    #[test]
    fn filters_resources_and_unknown_event_types_and_coalesces_bursts() -> TestResult {
        let mut decoder = Decoder::default();
        assert!(!decoder.push(
            br#"[{"resource":"tuner","type":"update"},{"resource":"program","type":"future"},"#
        )?);
        assert!(decoder.push(
            br#"{"resource":"program","type":"create"},{"resource":"program","type":"remove"}]"#
        )?);
        assert!(decoder.pending.is_empty());
        Ok(())
    }
    #[test]
    fn large_fragmented_event_is_only_decoded_when_complete_and_buffer_is_reused() -> TestResult {
        let prefix = br#"{"resource":"program","type":"update","data":""#;
        let suffix = br#""}"#;
        let mut decoder = Decoder::default();
        assert!(!decoder.push(prefix)?);
        for _ in 0..MAX_EVENT_BYTES - prefix.len() - suffix.len() {
            assert!(!decoder.push(b"x")?);
        }
        assert!(decoder.push(suffix)?);
        let capacity = decoder.pending.capacity();
        assert!(capacity <= MAX_EVENT_BYTES);
        for _ in 0..1000 {
            assert!(decoder.push(br#",{"resource":"program","type":"update"}"#)?);
            assert!(decoder.pending.is_empty());
            assert_eq!(decoder.pending.capacity(), capacity);
        }
        Ok(())
    }
    #[test]
    fn invalid_input_fails_closed_and_releases_partial_storage() -> TestResult {
        for invalid in [
            b"[bad".as_slice(),
            b"{]",
            br#"{"resource":3,"type":"update"}"#,
            b"{\"resource\":\"\xff\"}",
        ] {
            let mut decoder = Decoder::default();
            assert!(decoder.push(invalid).is_err());
            assert_eq!(decoder.pending.capacity(), 0);
            assert!(matches!(
                decoder.push(br#"{"resource":"program","type":"update"}"#),
                Err(Error::Failed)
            ));
        }
        let mut decoder = Decoder::default();
        assert!(!decoder.push(b"{\"data\":\"")?);
        assert!(matches!(
            decoder.push(&vec![b'x'; MAX_EVENT_BYTES]),
            Err(Error::TooLarge)
        ));
        assert_eq!(decoder.pending.capacity(), 0);
        Ok(())
    }
}
