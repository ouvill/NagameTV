//! Disk-backed comments and exact successful intervals. All I/O belongs to the
//! worker; GUI callers exchange bounded data and the latest demand only.
#[cfg(feature = "network")]
mod download;
mod spool;
mod store;
#[cfg(feature = "network")]
mod worker;
use crate::{Comment, CommentIdentity, Origin, Phase, Style};
use serde::{Deserialize, Serialize};
#[cfg(feature = "network")]
pub use worker::{Controller, Snapshot, State};

pub const WORKING_BYTES: usize = 16 * 1024 * 1024;
pub const WORKING_COMMENTS: usize = 50_000;
pub const LOOKBACK_SECONDS: i64 = crate::danmaku::MAX_LIFETIME.as_secs() as i64;
pub const FALLBACK_SECONDS: i64 = 30 * 60;
pub const ARCHIVE_DELAY_SECONDS: i64 = 10 * 60;
const COLLECTION_SECONDS: i64 = 5 * 60;
const MAX_REQUEST_SECONDS: i64 = 3 * 24 * 60 * 60;
const RECHECK_SECONDS: i64 = 60 * 60;
const SETTLED_SECONDS: i64 = 24 * 60 * 60;
const REQUEST_SPACING_SECONDS: i64 = 30;
const REQUEST_WINDOW_SECONDS: i64 = 10 * 60;
const REQUESTS_PER_WINDOW: usize = 6;
const CACHE_BYTES: i64 = 256 * 1024 * 1024;
const CACHE_INTERVALS: i64 = 1024;
const CACHE_AGE_SECONDS: i64 = 30 * 24 * 60 * 60;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("コメントを保存できません: {0}")]
    Io(#[from] std::io::Error),
    #[error("コメントDBを操作できません: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("{0}")]
    Archive(#[from] crate::archive::Error),
    #[error("コメントキャッシュの形式が不正です: {0}")]
    Format(String),
    #[error("コメント処理を終了しました")]
    Cancelled,
}

/// Integer UTC seconds, half-open. Bounds also guarantee SQLite microseconds
/// cannot overflow. Deserialization uses the same checked constructor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "(i64, i64)", into = "(i64, i64)")]
pub struct Interval {
    start: i64,
    end: i64,
}
impl Interval {
    pub fn new(start: i64, end: i64) -> Option<Self> {
        (start >= 0 && end > start && end <= i64::MAX / 1_000_000).then_some(Self { start, end })
    }
    pub fn start(self) -> i64 {
        self.start
    }
    pub fn end(self) -> i64 {
        self.end
    }
    pub fn contains(self, seconds: i64) -> bool {
        self.start <= seconds && seconds < self.end
    }
    pub fn intersection(self, other: Self) -> Option<Self> {
        Self::new(self.start.max(other.start), self.end.min(other.end))
    }
    pub fn missing(self, covered: impl IntoIterator<Item = Self>) -> Vec<Self> {
        let mut spans: Vec<_> = covered
            .into_iter()
            .filter_map(|span| span.intersection(self))
            .collect();
        spans.sort_by_key(|span| span.start);
        let mut next = self.start;
        let mut gaps = Vec::new();
        for span in spans {
            if let Some(gap) = Self::new(next, span.start) {
                gaps.push(gap);
            }
            next = next.max(span.end);
        }
        if let Some(gap) = Self::new(next, self.end) {
            gaps.push(gap);
        }
        gaps
    }
    fn request_prefix(self) -> Self {
        Self {
            start: self.start,
            end: self.end.min(self.start + MAX_REQUEST_SECONDS),
        }
    }
}
impl TryFrom<(i64, i64)> for Interval {
    type Error = &'static str;
    fn try_from((start, end): (i64, i64)) -> Result<Self, Self::Error> {
        Self::new(start, end).ok_or("invalid UTC interval")
    }
}
impl From<Interval> for (i64, i64) {
    fn from(value: Interval) -> Self {
        (value.start, value.end)
    }
}

pub fn archive_end(now: i64) -> i64 {
    (now - ARCHIVE_DELAY_SECONDS).div_euclid(COLLECTION_SECONDS) * COLLECTION_SECONDS
}

/// Verified playback clock segment, constructed by the application's Catalog
/// adapter. Retention uses media position, never one UTC lower bound across a
/// clock rollback. `key` identifies the original clock anchor/epoch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClockSpan {
    pub key: String,
    pub channel: u16,
    pub media_start_ms: i64,
    pub media_end_ms: i64,
    pub utc_start_ms: i64,
}
impl ClockSpan {
    pub fn interval(&self) -> Option<Interval> {
        let end = self
            .utc_start_ms
            .checked_add(self.media_end_ms.checked_sub(self.media_start_ms)?)?;
        Interval::new(
            self.utc_start_ms.div_euclid(1000),
            end.checked_add(999)?.div_euclid(1000),
        )
    }
    pub fn media_ms(&self, micros: u64) -> Option<i64> {
        let ms = i64::try_from(micros / 1000).ok()?;
        let media = self
            .media_start_ms
            .checked_add(ms.checked_sub(self.utc_start_ms)?)?;
        (self.media_start_ms <= media && media < self.media_end_ms).then_some(media)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecordingRange {
    Discovering(Vec<ClockSpan>),
    Known(Vec<ClockSpan>),
}
impl RecordingRange {
    pub fn spans(&self) -> &[ClockSpan] {
        match self {
            Self::Discovering(spans) | Self::Known(spans) => spans,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reception {
    Receiving(u64),
    Interrupted,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    Pending,
    Recording(RecordingRange),
    Live {
        earliest_media_ms: i64,
        spans: Vec<ClockSpan>,
        reception: Reception,
        at_edge: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    pub clock_key: String,
    pub interval: Interval,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Demand {
    pub source: u64,
    pub channel: u16,
    pub view: Option<View>,
    pub source_range: Source,
    pub fetch: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordOrigin {
    Live,
    Archive,
}
#[derive(Clone, Debug)]
pub struct Record {
    pub id: u64,
    pub comment: Comment,
    pub own: bool,
    pub origin: RecordOrigin,
    pub media_ms: Option<i64>,
}

#[derive(Serialize, Deserialize)]
struct StoredComment {
    text: Box<str>,
    origin: Origin,
    phase: Phase,
    timestamp: Option<u64>,
    unix_seconds: u64,
    source_id: Option<(u64, u64)>,
    identity: Option<CommentIdentity>,
    style: Style,
}
impl From<Comment> for StoredComment {
    fn from(c: Comment) -> Self {
        Self {
            text: c.text,
            origin: c.origin,
            phase: c.phase,
            timestamp: c.timestamp_micros,
            unix_seconds: c.unix_seconds,
            source_id: c.source_id,
            identity: c.identity,
            style: c.style,
        }
    }
}
impl From<StoredComment> for Comment {
    fn from(c: StoredComment) -> Self {
        Self {
            text: c.text,
            origin: c.origin,
            phase: c.phase,
            timestamp_micros: c.timestamp,
            unix_seconds: c.unix_seconds,
            source_id: c.source_id,
            identity: c.identity,
            style: c.style,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_intervals_find_gaps_across_arbitrary_recordings() {
        let minute = |h: i64, m: i64| h * 3600 + m * 60;
        let range = |h1, m1, h2, m2| Interval::new(minute(h1, m1), minute(h2, m2)).unwrap();
        let received = [range(22, 10, 22, 15), range(22, 20, 22, 30)];
        assert_eq!(
            range(22, 12, 22, 25).missing(received),
            vec![range(22, 15, 22, 20)]
        );
        assert_eq!(
            range(20, 13, 22, 50).missing(received),
            vec![
                range(20, 13, 22, 10),
                range(22, 15, 22, 20),
                range(22, 30, 22, 50)
            ]
        );
        assert!(range(22, 10, 22, 15).missing(received).is_empty());
    }
}
