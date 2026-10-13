//! Stable input identity and validated progress, independent of Qt and the decoder.
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

pub(super) const MAX_RECORDINGS: usize = 500;
pub const MAX_CONTINUE: usize = 100;
pub const SAVE_INTERVAL: Duration = Duration::from_secs(15);
const WATCHED_FRACTION: f64 = 0.95;
const WATCHED_REMAINING_MS: f64 = 30_000.0;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Source {
    Local {
        path: PathBuf,
    },
    Http {
        url: String,
    },
    Epgstation {
        server: String,
        recorded_id: u64,
        video_file_id: u64,
    },
}
impl Source {
    pub fn http(mut url: url::Url) -> Self {
        url.set_fragment(None);
        Self::Http { url: url.into() }
    }
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Local { .. } => "local",
            Self::Http { .. } => "http",
            Self::Epgstation { .. } => "epgstation",
        }
    }
    fn valid(&self) -> bool {
        match self {
            Self::Local { path } => path.is_absolute(),
            Self::Http { url } => url::Url::parse(url).is_ok_and(|url| {
                matches!(url.scheme(), "http" | "https")
                    && url.host_str().is_some()
                    && url.fragment().is_none()
            }),
            Self::Epgstation { server, .. } => crate::epgstation::Endpoint::parse(server)
                .is_ok_and(|endpoint| endpoint.as_str() == server),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BroadcastHint {
    pub service: crate::channels::BroadcastService,
    pub start_ms: i64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    pub source: Source,
    #[serde(default)]
    pub thumbnail_id: Option<u64>,
    pub title: String,
    pub channel: String,
    pub broadcast: Option<BroadcastHint>,
}
impl Identity {
    pub fn new(source: Source, title: String) -> Self {
        Self {
            source,
            thumbnail_id: None,
            title,
            channel: String::new(),
            broadcast: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Position {
    position_ms: f64,
    duration_ms: f64,
}
impl Position {
    pub fn new(position_ms: f64, duration_ms: f64) -> Option<Self> {
        (position_ms.is_finite()
            && duration_ms.is_finite()
            && position_ms >= 0.0
            && duration_ms > 0.0)
            .then(|| Self {
                position_ms: position_ms.min(duration_ms),
                duration_ms,
            })
    }
    pub fn position_ms(self) -> f64 {
        self.position_ms
    }
    pub fn duration_ms(self) -> f64 {
        self.duration_ms
    }
    pub fn progress(self) -> f64 {
        self.position_ms / self.duration_ms
    }
    pub fn remaining_ms(self) -> f64 {
        self.duration_ms - self.position_ms
    }
    pub fn watched(self) -> bool {
        self.position_ms > 0.0
            && (self.progress() >= WATCHED_FRACTION || self.remaining_ms() <= WATCHED_REMAINING_MS)
    }
    pub fn resumable(self) -> bool {
        self.position_ms > 0.0 && !self.watched()
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub key: String,
    pub identity: Identity,
    pub position: Position,
}
impl Entry {
    pub(super) fn valid(&self) -> bool {
        const MAX_TEXT_BYTES: usize = 16 * 1024;
        !self.key.is_empty()
            && self.key.len() <= 64
            && self.identity.source.valid()
            && self.identity.title.len() <= MAX_TEXT_BYTES
            && self.identity.channel.len() <= MAX_TEXT_BYTES
            && self.identity.broadcast.as_ref().is_none_or(|hint| {
                hint.start_ms > 0 && hint.start_ms.checked_mul(1_000_000).is_some()
            })
            && Position::new(self.position.position_ms, self.position.duration_ms)
                == Some(self.position)
    }
}

// Waiting consumes the right to perform the automatic seek exactly once.
#[derive(Default)]
enum Resume {
    #[default]
    Waiting,
    Seeking,
    Running,
}
pub struct Session {
    identity: Identity,
    resume: Resume,
    latest: Option<Position>,
    next_save: Instant,
}
impl Session {
    pub fn new(identity: Identity, now: Instant) -> Self {
        Self {
            identity,
            resume: Resume::Waiting,
            latest: None,
            next_save: now + SAVE_INTERVAL,
        }
    }
    pub fn resume(&mut self, history: &super::History, duration_ms: f64) -> Option<f64> {
        if !matches!(self.resume, Resume::Waiting) || !history.ready() || duration_ms <= 0.0 {
            return None;
        }
        let position = history
            .entry(&self.identity.source)
            .and_then(|entry| Position::new(entry.position.position_ms(), duration_ms));
        if let Some(position) = position.filter(|position| position.resumable()) {
            self.resume = Resume::Seeking;
            Some(position.position_ms())
        } else {
            self.resume = Resume::Running;
            None
        }
    }
    pub fn seek_rejected(&mut self) {
        self.resume = Resume::Running;
    }
    pub fn sample(&mut self, position: Option<Position>, seeking: bool) {
        match self.resume {
            Resume::Waiting => return,
            Resume::Seeking if seeking => return,
            Resume::Seeking => self.resume = Resume::Running,
            Resume::Running if seeking => return,
            Resume::Running => {}
        }
        if let Some(position) = position {
            self.latest = Some(position);
        }
    }
    pub fn save(&mut self, history: &mut super::History, now: Instant, force: bool) -> bool {
        if !force && now < self.next_save {
            return false;
        }
        self.next_save = now + SAVE_INTERVAL;
        match self.latest {
            Some(position) => history.record(self.identity.clone(), position),
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn identity() -> Identity {
        Identity::new(
            Source::Local {
                path: "/recording.ts".into(),
            },
            "Recording".into(),
        )
    }
    #[test]
    fn watched_boundaries_and_invalid_samples_preserve_resume_rules() {
        assert!(!Position::new(949_999.0, 1_000_000.0).unwrap().watched());
        assert!(Position::new(950_000.0, 1_000_000.0).unwrap().watched());
        assert!(!Position::new(269_999.0, 300_000.0).unwrap().watched());
        assert!(Position::new(270_000.0, 300_000.0).unwrap().watched());
        assert!(!Position::new(0.0, 20_000.0).unwrap().watched());
        for (position, duration) in [
            (f64::NAN, 100.0),
            (-1.0, 100.0),
            (1.0, 0.0),
            (1.0, f64::INFINITY),
        ] {
            assert!(Position::new(position, duration).is_none());
        }
    }
    #[test]
    fn automatic_seek_is_once_and_unconfirmed_positions_never_overwrite_history() {
        let now = Instant::now();
        let mut history = super::super::History::transient();
        let saved = Position::new(120_000.0, 600_000.0).unwrap();
        history.record(identity(), saved);
        let mut session = Session::new(identity(), now);
        session.sample(Position::new(100.0, 600_000.0), false);
        assert!(!session.save(&mut history, now, true));
        assert_eq!(session.resume(&history, 600_000.0), Some(120_000.0));
        assert_eq!(session.resume(&history, 600_000.0), None);
        session.sample(Position::new(100.0, 600_000.0), true);
        assert!(!session.save(&mut history, now, true));
        session.sample(Some(saved), false);
        assert!(!session.save(&mut history, now, false));
        session.sample(Position::new(121_000.0, 600_000.0), false);
        assert!(session.save(&mut history, now + SAVE_INTERVAL, false));
        session.sample(Position::new(130_000.0, 600_000.0), true);
        assert!(!session.save(&mut history, now, true));
        assert_eq!(
            history
                .entry(&identity().source)
                .unwrap()
                .position
                .position_ms(),
            121_000.0
        );
    }
    #[test]
    fn completed_recording_restarts_at_the_beginning() {
        let mut history = super::super::History::transient();
        history.record(identity(), Position::new(599_000.0, 600_000.0).unwrap());
        assert!(history.continue_watching().is_empty());
        let mut session = Session::new(identity(), Instant::now());
        assert_eq!(session.resume(&history, 600_000.0), None);
    }
    #[test]
    fn http_fragments_are_ignored_but_distinct_queries_remain_distinct() {
        let source = Source::http(url::Url::parse("https://EXAMPLE.test/a?q=1#title").unwrap());
        assert_eq!(
            source,
            Source::http(url::Url::parse("https://example.test/a?q=1#other").unwrap())
        );
        assert_ne!(
            source,
            Source::http(url::Url::parse("https://example.test/a?q=2").unwrap())
        );
    }
}
