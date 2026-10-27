//! Bounded SSE frames; only EPG updates are interpreted as program changes.
use crate::{Error, MAX_EVENT_BYTES};
use serde::Deserialize;

#[derive(Default)]
enum State {
    #[default]
    Reading,
    Failed,
}

#[derive(Default)]
pub struct Decoder {
    state: State,
    pending: Vec<u8>,
    line_start: usize,
    after_cr: bool,
}
impl Decoder {
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
            if std::mem::take(&mut self.after_cr) && byte == b'\n' {
                continue;
            }
            if matches!(byte, b'\r' | b'\n') {
                self.after_cr = byte == b'\r';
                if self.pending.len() == self.line_start {
                    changed |= event(&self.pending)?;
                    self.pending.clear();
                    self.line_start = 0;
                    continue;
                }
                self.append(b'\n')?;
                self.line_start = self.pending.len();
            } else {
                self.append(byte)?;
            }
        }
        Ok(changed)
    }
    fn append(&mut self, byte: u8) -> Result<(), Error> {
        if self.pending.len() == MAX_EVENT_BYTES {
            return Err(Error::TooLarge);
        }
        self.pending.push(byte);
        Ok(())
    }
}
fn event(frame: &[u8]) -> Result<bool, Error> {
    let mut kind = b"".as_slice();
    let mut data = Vec::new();
    for line in frame.split(|byte| *byte == b'\n') {
        let (field, value) = match line.iter().position(|byte| *byte == b':') {
            Some(colon) => (&line[..colon], &line[colon + 1..]),
            None => (line, b"".as_slice()),
        };
        let value = value.strip_prefix(b" ").unwrap_or(value);
        match field {
            b"event" => kind = value,
            b"data" => {
                data.extend_from_slice(value);
                data.push(b'\n');
            }
            _ => {}
        }
    }
    if kind != b"epg.programs-updated" || data.is_empty() {
        return Ok(false);
    }
    #[derive(Deserialize)]
    struct Update {
        #[serde(rename = "serviceId")]
        _service_id: u64,
    }
    let _: Update = serde_json::from_slice(&data)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    type TestResult = Result<(), Box<dyn std::error::Error>>;
    #[test]
    fn every_split_handles_crlf_comments_multiline_data_and_large_service_ids() -> TestResult {
        let event = ": keepalive\r\nid: 42\r\nevent: epg.programs-updated\r\ndata: {\r\ndata: \"serviceId\":3272402081,\"extra\":\"日本語\"}\r\n\r\n".as_bytes();
        for split in 0..=event.len() {
            let mut decoder = Decoder::default();
            let first = decoder.push(&event[..split])?;
            let second = decoder.push(&event[split..])?;
            assert_eq!(usize::from(first) + usize::from(second), 1);
            assert!(decoder.pending.is_empty());
        }
        for ending in ["\n", "\r", "\r\n"] {
            let event = format!(
                "event: epg.programs-updated{ending}data: {{\"serviceId\":1}}{ending}{ending}"
            );
            let mut decoder = Decoder::default();
            let mut changes = 0;
            for byte in event.bytes() {
                changes += usize::from(decoder.push(&[byte])?);
            }
            assert_eq!(changes, 1);
        }
        Ok(())
    }
    #[test]
    fn other_events_empty_frames_and_incomplete_frames_do_not_refresh() -> TestResult {
        let mut decoder = Decoder::default();
        assert!(!decoder.push(b": comment\n\nevent: tuner.status-changed\ndata: not our schema\n\nevent: epg.programs-updated\n\n")?);
        assert!(!decoder.push(b"event: epg.programs-updated\ndata: {\"serviceId\":1}\n")?);
        assert!(decoder.push(b"\n")?);
        assert!(
            !decoder.push(b"data: {\"serviceId\":1}\n\n")?,
            "event type leaked into next frame"
        );
        Ok(())
    }
    #[test]
    fn frame_limit_includes_comments_and_releases_storage_after_failure() -> TestResult {
        let mut decoder = Decoder::default();
        assert!(!decoder.push(b":")?);
        for _ in 0..MAX_EVENT_BYTES - 2 {
            assert!(!decoder.push(b"x")?);
        }
        assert!(!decoder.push(b"\n\n")?);
        assert!(decoder.pending.capacity() <= MAX_EVENT_BYTES);
        let capacity = decoder.pending.capacity();
        for _ in 0..1000 {
            assert!(decoder.push(b"event: epg.programs-updated\ndata: {\"serviceId\":1}\n\n")?);
            assert_eq!(decoder.pending.capacity(), capacity);
        }
        assert!(matches!(
            decoder.push(&vec![b'x'; MAX_EVENT_BYTES + 1]),
            Err(Error::TooLarge)
        ));
        assert_eq!(decoder.pending.capacity(), 0);
        assert!(matches!(decoder.push(b"\n\n"), Err(Error::Failed)));
        let mut decoder = Decoder::default();
        assert!(matches!(
            decoder.push(b"event: epg.programs-updated\ndata: {\"serviceId\":null}\n\n"),
            Err(Error::Json(_))
        ));
        assert_eq!(decoder.pending.capacity(), 0);
        Ok(())
    }
}
