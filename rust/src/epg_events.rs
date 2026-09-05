//! Mirakurun's open JSON-array event stream, separate from finite API requests.
use futures_util::StreamExt;
use serde::Deserialize;
use std::{sync::mpsc::SyncSender, time::Duration};
use tokio::time::Instant;

const REFRESH_INTERVAL: Duration = Duration::from_secs(60);
const MAX_EVENT_BYTES: usize = 1024 * 1024;

#[derive(Deserialize)]
struct Event {
    resource: String,
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Default)]
struct EventDecoder {
    pending: Vec<u8>,
}

impl EventDecoder {
    // Decode complete objects only, regardless of HTTP chunk boundaries. Ignore
    // the surrounding array punctuation and unknown fields (including event data).
    fn push(&mut self, bytes: &[u8]) -> Result<bool, &'static str> {
        let mut changed = false;
        for part in bytes.chunks(16 * 1024) {
            if self.pending.len() + part.len() > MAX_EVENT_BYTES {
                return Err("EPG event exceeds buffer limit");
            }
            self.pending.extend_from_slice(part);
            let mut consumed = 0;
            loop {
                while self
                    .pending
                    .get(consumed)
                    .is_some_and(|b| b.is_ascii_whitespace() || matches!(b, b'[' | b']' | b','))
                {
                    consumed += 1;
                }
                if consumed == self.pending.len() {
                    break;
                }
                let mut values = serde_json::Deserializer::from_slice(&self.pending[consumed..])
                    .into_iter::<Event>();
                match values.next() {
                    Some(Ok(event)) => {
                        changed |= event.resource == "program"
                            && matches!(event.kind.as_str(), "create" | "update" | "remove");
                        consumed += values.byte_offset();
                    }
                    Some(Err(error)) if error.is_eof() => break,
                    _ => return Err("Invalid EPG event JSON"),
                }
            }
            self.pending.drain(..consumed);
        }
        Ok(changed)
    }
}

struct RefreshGate {
    dirty: bool,
    last: Instant,
}

impl RefreshGate {
    fn flush(&mut self, events: &SyncSender<u64>, generation: u64) {
        if self.dirty && self.last.elapsed() >= REFRESH_INTERVAL {
            // A full queue already represents a pending refresh of the same server.
            let _ = events.try_send(generation);
            self.dirty = false;
            self.last = Instant::now();
        }
    }
}

pub async fn receive(
    client: reqwest::Client,
    server: String,
    generation: u64,
    events: SyncSender<u64>,
) {
    let url = format!("{server}/api/events/stream?resource=program");
    let mut gate = RefreshGate {
        dirty: false,
        last: Instant::now(),
    };
    loop {
        // Limit the wait for response headers, never the lifetime of the body.
        let response = tokio::time::timeout(Duration::from_secs(10), client.get(&url).send()).await;
        if let Ok(Ok(response)) = response {
            if let Ok(response) = response.error_for_status() {
                let mut stream = response.bytes_stream();
                let mut decoder = EventDecoder::default();
                loop {
                    gate.flush(&events, generation);
                    match tokio::time::timeout(Duration::from_secs(1), stream.next()).await {
                        Err(_) => continue, // Quiet streams are healthy; flush trailing changes.
                        Ok(Some(Ok(bytes))) => match decoder.push(&bytes) {
                            Ok(changed) => gate.dirty |= changed,
                            Err(error) => {
                                tracing::warn!(error, "Reconnecting EPG stream");
                                break;
                            }
                        },
                        Ok(Some(Err(error))) => {
                            tracing::warn!(%error, "EPG stream interrupted");
                            break;
                        }
                        Ok(None) => break,
                    }
                }
            }
        }
        // Reconcile changes potentially lost while disconnected, including a
        // reconnect with no subsequent events. Preserve the gate across retries.
        gate.dirty = true;
        gate.flush(&events, generation);
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

#[cfg(test)]
mod tests {
    // In tests, unwrap/expect assert successful setup or an expected result.
    // Failures intentionally fail the test; they are not assumed impossible IO.
    use super::*;

    #[test]
    fn handles_every_byte_boundary_and_ignores_array_header() {
        let mut decoder = EventDecoder::default();
        assert!(!decoder.push(b"[\n").unwrap());
        let bytes = br#"{"resource":"program","type":"update","data":{"name":"a,[]\"b"}},\n"#;
        // Leave off the literal backslash-n at the end of the fixture.
        let mut notifications = 0;
        for byte in &bytes[..bytes.len() - 2] {
            notifications += usize::from(decoder.push(&[*byte]).unwrap());
        }
        assert_eq!(notifications, 1);
        assert!(decoder.pending.is_empty());
    }

    #[test]
    fn handles_multiple_events_and_filters_other_resources() {
        let mut decoder = EventDecoder::default();
        assert!(
            !decoder
                .push(br#"[{"resource":"tuner","type":"update"},"#)
                .unwrap()
        );
        assert!(decoder.push(br#"{"resource":"program","type":"create"},{"resource":"program","type":"remove"}]"#).unwrap());
        assert!(decoder.pending.is_empty());
    }

    #[test]
    fn malformed_and_oversized_events_are_rejected() {
        assert!(EventDecoder::default().push(b"[bad").is_err());
        let mut decoder = EventDecoder::default();
        decoder.push(b"[{\"resource\":\"").unwrap();
        assert!(decoder.push(&vec![b'a'; MAX_EVENT_BYTES]).is_err());
    }

    #[test]
    fn coalesces_bursts_and_flushes_without_another_event() {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let mut gate = RefreshGate {
            dirty: true,
            last: Instant::now(),
        };
        gate.flush(&tx, 7);
        assert!(rx.try_recv().is_err());
        assert!(gate.dirty);
        gate.last -= REFRESH_INTERVAL;
        gate.flush(&tx, 7);
        assert_eq!(rx.try_recv().unwrap(), 7);
        assert!(!gate.dirty);
        gate.flush(&tx, 7);
        assert!(rx.try_recv().is_err());
    }
}
