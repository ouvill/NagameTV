//! Bounded chapter data and navigation, independent of Qt and playback controls.
mod discovery;
mod mp4;
mod toc;
pub(crate) use discovery::{Controller, Source};
use serde::Serialize;
use std::sync::Arc;

const MAX_CHAPTERS: usize = 255;
const MAX_NAME_BYTES: usize = 255;
const RESTART_AFTER_MS: u64 = 3_000;
const MAX_START_MS: u64 = i64::MAX as u64 / 1_000_000;

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("Invalid chapter data: {0}")]
    Invalid(&'static str),
    #[error("Chapter data exceeds its size or count limit")]
    Limit,
    #[error("Chapter name is not UTF-8: {0}")]
    Utf8(#[from] std::str::Utf8Error),
    #[error("Could not read chapter data: {0}")]
    Read(#[from] std::io::Error),
    #[error("Chapter discovery cancelled")]
    Cancelled,
    #[error("Chapter discovery timed out")]
    TimedOut,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct Chapter {
    start_ms: u64,
    name: String,
    commercial: bool,
}
impl Chapter {
    fn new(start_ms: u64, name: String) -> Result<Self, Error> {
        if start_ms > MAX_START_MS || name.len() > MAX_NAME_BYTES {
            return Err(Error::Limit);
        }
        let commercial = name.starts_with("CM");
        Ok(Self {
            start_ms,
            name,
            commercial,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct List {
    entries: Arc<[Chapter]>,
    json: Arc<str>,
}
impl Default for List {
    fn default() -> Self {
        Self {
            entries: Arc::default(),
            json: "[]".into(),
        }
    }
}
impl List {
    fn new(mut entries: Vec<Chapter>) -> Result<Self, Error> {
        if entries.len() > MAX_CHAPTERS {
            return Err(Error::Limit);
        }
        entries.sort_by_key(|chapter| chapter.start_ms);
        entries.dedup_by_key(|chapter| chapter.start_ms);
        let json =
            serde_json::to_string(&entries).expect("bounded chapter strings and integer times");
        Ok(Self {
            entries: entries.into(),
            json: json.into(),
        })
    }
    pub(crate) fn from_toc(toc: &gstreamer::TocRef) -> Result<Self, Error> {
        toc::parse(toc)
    }
    pub(crate) fn json(&self) -> &str {
        &self.json
    }
    fn current(&self, position_ms: u64) -> Option<usize> {
        self.entries
            .partition_point(|chapter| chapter.start_ms <= position_ms)
            .checked_sub(1)
    }
    fn within(&self, duration_ms: Option<u64>) -> Self {
        let Some(duration) = duration_ms else {
            return self.clone();
        };
        let count = self
            .entries
            .partition_point(|chapter| chapter.start_ms < duration);
        if count == self.entries.len() {
            return self.clone();
        }
        Self::new(self.entries[..count].to_vec()).expect("subset of bounded chapters")
    }
    pub(crate) fn target(&self, offset: i32, position_ms: u64) -> Option<u64> {
        let current = self.current(position_ms);
        let target = match offset {
            1 => {
                let next = current.map_or(0, |index| index + 1);
                self.entries
                    .get(next..)?
                    .iter()
                    .find(|chapter| !chapter.commercial)
                    .or_else(|| self.entries.get(next))
            }
            -1 => {
                let index = current?;
                let chapter = &self.entries[index];
                if position_ms - chapter.start_ms > RESTART_AFTER_MS {
                    Some(chapter)
                } else {
                    self.entries[..index]
                        .iter()
                        .rev()
                        .find(|chapter| !chapter.commercial)
                        .or_else(|| {
                            index
                                .checked_sub(1)
                                .and_then(|index| self.entries.get(index))
                        })
                }
            }
            _ => None,
        }?;
        Some(target.start_ms)
    }
}

#[cfg(test)]
mod tests;
