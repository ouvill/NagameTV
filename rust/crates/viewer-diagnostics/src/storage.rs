//! Complete JSON lines, bounded before serialization reaches disk.
use serde::Serialize;
use std::{
    fs::{self, File},
    io::{self, Write},
    path::PathBuf,
};

pub const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_RECORD_BYTES: usize = 64 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("診断ログの操作に失敗しました: {0}")]
    Io(#[from] io::Error),
    #[error("診断レコードの変換に失敗しました: {0}")]
    Json(#[from] serde_json::Error),
    #[error("診断レコードがサイズ上限を超えています")]
    RecordTooLarge,
    #[error("既存の診断ログがサイズ上限を超えています")]
    ExistingFileTooLarge,
}

pub struct RotatingWriter {
    path: PathBuf,
    file: File,
    bytes: u64,
    limit: u64,
}
impl RotatingWriter {
    pub fn new(path: PathBuf) -> Result<Self, Error> {
        Self::with_limit(path, MAX_FILE_BYTES)
    }
    fn with_limit(path: PathBuf, limit: u64) -> Result<Self, Error> {
        let file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        let bytes = file.metadata()?.len();
        // Do not copy an arbitrarily large preexisting file into the previous segment.
        if bytes > limit {
            return Err(Error::ExistingFileTooLarge);
        }
        Ok(Self {
            path,
            file,
            bytes,
            limit,
        })
    }
    /// IO failure is terminal: drop this writer and stop the recording worker.
    /// Serialization/size errors leave the file unchanged and may be skipped.
    pub fn write(&mut self, value: &impl Serialize) -> Result<(), Error> {
        let mut line = BoundedLine {
            bytes: Vec::new(),
            limit: self.limit.min(MAX_RECORD_BYTES as u64) as usize,
            exceeded: false,
        };
        if let Err(error) = serde_json::to_writer(&mut line, value) {
            return Err(if line.exceeded {
                Error::RecordTooLarge
            } else {
                Error::Json(error)
            });
        }
        line.write_all(b"\n").map_err(|_| Error::RecordTooLarge)?;
        let length = line.bytes.len() as u64;
        if self.bytes.saturating_add(length) > self.limit {
            // Only the logging worker performs this bounded copy. Keep one prior segment.
            self.file.flush()?;
            fs::copy(&self.path, self.path.with_extension("previous.jsonl"))?;
            self.file.set_len(0)?;
            self.bytes = 0;
        }
        self.file.write_all(&line.bytes)?;
        self.bytes += length;
        Ok(())
    }
}

struct BoundedLine {
    bytes: Vec<u8>,
    limit: usize,
    exceeded: bool,
}
impl Write for BoundedLine {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            self.exceeded = true;
            return Err(io::Error::other("Diagnostic record exceeds limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rotation_preserves_complete_recent_records_within_both_limits()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("usage.jsonl");
        let mut writer = RotatingWriter::with_limit(path.clone(), 80)?;
        for sample in 0..20 {
            writer.write(&serde_json::json!({"sample": sample}))?;
        }
        drop(writer);
        let mut samples = Vec::new();
        for path in [path.with_extension("previous.jsonl"), path] {
            let text = fs::read_to_string(path)?;
            assert!(text.len() <= 80);
            assert!(text.ends_with('\n'));
            for line in text.lines() {
                let record: serde_json::Value = serde_json::from_str(line)?;
                samples.push(record["sample"].as_u64().ok_or("missing sample")?);
            }
        }
        assert_eq!(samples.last(), Some(&19));
        assert!(samples.windows(2).all(|pair| pair[1] == pair[0] + 1));
        assert_eq!(fs::read_dir(dir.path())?.count(), 2);
        Ok(())
    }
    #[test]
    fn oversized_serialization_does_not_change_the_existing_log()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("usage.jsonl");
        let mut writer = RotatingWriter::with_limit(path.clone(), 80)?;
        writer.write(&"valid")?;
        let before = fs::read(&path)?;
        // JSON escaping expands this input beyond the limit.
        assert!(matches!(
            writer.write(&"\n".repeat(50)),
            Err(Error::RecordTooLarge)
        ));
        assert_eq!(fs::read(&path)?, before);
        assert_eq!(fs::read_dir(dir.path())?.count(), 1);
        drop(writer);
        assert!(matches!(
            RotatingWriter::with_limit(path.clone(), 1),
            Err(Error::ExistingFileTooLarge)
        ));
        assert_eq!(fs::read(path)?, before);
        Ok(())
    }
}
