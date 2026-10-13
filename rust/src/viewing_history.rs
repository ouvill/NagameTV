//! Viewing preferences owned independently of Qt and persisted off the UI thread.
mod live;
pub mod recordings;
mod storage;
use recordings::{Entry, Identity, Position, Source};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const MAX_FAVORITES: usize = 4096;

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
#[serde(default)]
struct Document {
    favorites: Vec<u64>,
    recent: Vec<u64>,
    recordings: Vec<Entry>,
    next_key: u64,
}
impl Document {
    fn validate(&self) -> Result<(), storage::Error> {
        if self.recordings.len() > recordings::MAX_RECORDINGS
            || self.recordings.iter().any(|entry| {
                !entry.valid()
                    || entry
                        .key
                        .strip_prefix("recording:")
                        .and_then(|key| key.parse::<u64>().ok())
                        .is_none_or(|key| key == 0 || key > self.next_key)
            })
            || self
                .recordings
                .iter()
                .map(|entry| &entry.identity.source)
                .collect::<std::collections::HashSet<_>>()
                .len()
                != self.recordings.len()
            || self
                .recordings
                .iter()
                .map(|entry| &entry.key)
                .collect::<std::collections::HashSet<_>>()
                .len()
                != self.recordings.len()
            || self.recent.len() > live::MAX_RECENT
            || self
                .recent
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                != self.recent.len()
            || self.favorites.len() > MAX_FAVORITES
            || self
                .favorites
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                != self.favorites.len()
        {
            return Err(storage::Error::Invalid);
        }
        Ok(())
    }
}

enum State {
    Loading,
    Ready(Document),
}

pub struct History {
    state: State,
    live: live::Live,
    storage: Option<storage::Worker>,
    error: String,
}
impl History {
    pub fn transient() -> Self {
        Self {
            state: State::Ready(Document::default()),
            live: Default::default(),
            storage: None,
            error: String::new(),
        }
    }
    pub fn open(path: PathBuf) -> Self {
        match storage::Worker::start(path) {
            Ok(storage) => Self {
                state: State::Loading,
                live: Default::default(),
                storage: Some(storage),
                error: String::new(),
            },
            Err(error) => {
                tracing::error!(
                    error = &error as &dyn std::error::Error,
                    "Start viewing history worker failed"
                );
                Self {
                    error: error.to_string(),
                    ..Self::transient()
                }
            }
        }
    }
    pub fn ready(&self) -> bool {
        matches!(self.state, State::Ready(_))
    }
    pub fn error(&self) -> &str {
        &self.error
    }
    pub fn favorites(&self) -> &[u64] {
        match &self.state {
            State::Loading => &[],
            State::Ready(data) => &data.favorites,
        }
    }
    pub fn toggle_favorite(&mut self, service: u64) -> bool {
        let State::Ready(data) = &mut self.state else {
            return false;
        };
        if let Some(index) = data.favorites.iter().position(|id| *id == service) {
            data.favorites.remove(index);
        } else {
            if data.favorites.len() == MAX_FAVORITES {
                return false;
            }
            data.favorites.push(service);
        }
        self.save();
        true
    }
    pub fn move_favorite(&mut self, service: u64, position: usize) -> bool {
        let State::Ready(data) = &mut self.state else {
            return false;
        };
        if position >= data.favorites.len() {
            return false;
        }
        let Some(index) = data.favorites.iter().position(|id| *id == service) else {
            return false;
        };
        data.favorites.remove(index);
        data.favorites.insert(position, service);
        self.save();
        true
    }
    pub fn recent(&self) -> &[u64] {
        match &self.state {
            State::Loading => &[],
            State::Ready(data) => &data.recent,
        }
    }
    pub fn observe_live(&mut self, service: Option<u64>, now: std::time::Instant) -> bool {
        if !self.ready() {
            return false;
        }
        let before = self
            .live
            .previous(service.unwrap_or_default(), self.recent(), |_| true);
        let qualified = self.live.observe(service, now);
        if let Some(service) = qualified {
            if let State::Ready(data) = &mut self.state {
                data.recent.retain(|id| *id != service);
                data.recent.insert(0, service);
                data.recent.truncate(live::MAX_RECENT);
            }
            self.save();
        }
        qualified.is_some()
            || before
                != self
                    .live
                    .previous(service.unwrap_or_default(), self.recent(), |_| true)
    }
    pub fn previous(&self, current: u64, exists: impl Fn(u64) -> bool) -> Option<u64> {
        self.live.previous(current, self.recent(), exists)
    }
    pub fn entry(&self, source: &Source) -> Option<&Entry> {
        match &self.state {
            State::Loading => None,
            State::Ready(data) => data
                .recordings
                .iter()
                .find(|entry| &entry.identity.source == source),
        }
    }
    pub fn recordings(&self) -> &[Entry] {
        match &self.state {
            State::Loading => &[],
            State::Ready(data) => &data.recordings,
        }
    }
    pub fn continue_watching(&self) -> Vec<Entry> {
        self.recordings()
            .iter()
            .filter(|entry| entry.position.resumable())
            .take(recordings::MAX_CONTINUE)
            .cloned()
            .collect()
    }
    pub fn record(&mut self, identity: Identity, position: Position) -> bool {
        let State::Ready(data) = &mut self.state else {
            return false;
        };
        let previous = data
            .recordings
            .iter()
            .position(|entry| entry.identity.source == identity.source);
        if previous.is_some_and(|index| {
            data.recordings[index].position == position
                && data.recordings[index].identity == identity
        }) {
            return false;
        }
        let key = match previous {
            Some(index) => data.recordings.remove(index).key,
            None => {
                let Some(next) = data.next_key.checked_add(1) else {
                    tracing::error!(
                        "Allocate viewing history identity failed: key space exhausted"
                    );
                    return false;
                };
                data.next_key = next;
                format!("recording:{next}")
            }
        };
        data.recordings.insert(
            0,
            Entry {
                key,
                identity,
                position,
            },
        );
        data.recordings.truncate(recordings::MAX_RECORDINGS);
        self.save();
        true
    }
    fn save(&mut self) {
        if let (State::Ready(data), Some(worker)) = (&self.state, &mut self.storage) {
            worker.save(data.clone());
        }
    }
    pub fn poll(&mut self) -> bool {
        let Some(worker) = &mut self.storage else {
            return false;
        };
        let mut changed = false;
        while let Some(result) = worker.poll() {
            match result {
                storage::Update::Loaded(Ok(data)) => {
                    self.state = State::Ready(data);
                    changed = true;
                }
                storage::Update::Loaded(Err(error)) => {
                    changed = true;
                    self.state = State::Ready(Document::default());
                    self.error = error.to_string();
                }
                storage::Update::Saved(Ok(())) => {
                    changed |= !self.error.is_empty();
                    self.error.clear();
                }
                storage::Update::Saved(Err(error)) => {
                    let error = error.to_string();
                    changed |= self.error != error;
                    self.error = error;
                }
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recent_stations_are_unique_bounded_and_newest_first() {
        let mut history = History::transient();
        let mut now = std::time::Instant::now();
        for service in 0..25 {
            history.observe_live(Some(service), now);
            now += live::QUALIFY_AFTER;
            history.observe_live(Some(service), now);
        }
        assert_eq!(history.recent().len(), live::MAX_RECENT);
        assert_eq!(history.recent()[0], 24);
        history.observe_live(Some(10), now);
        history.observe_live(Some(10), now + live::QUALIFY_AFTER);
        assert_eq!(history.recent()[0], 10);
        assert_eq!(history.recent().iter().filter(|id| **id == 10).count(), 1);
    }
    #[test]
    fn favorites_keep_explicit_order_and_readding_appends() {
        let mut history = History::transient();
        assert!(history.toggle_favorite(u64::MAX));
        assert!(history.toggle_favorite(10));
        assert!(history.move_favorite(10, 0));
        assert_eq!(history.favorites(), &[10, u64::MAX]);
        assert!(!history.move_favorite(11, 0));
        assert!(!history.move_favorite(10, 2));
        history.toggle_favorite(10);
        history.toggle_favorite(10);
        assert_eq!(history.favorites(), &[u64::MAX, 10]);
        history.toggle_favorite(10);
        history.toggle_favorite(u64::MAX);
        assert!(history.favorites().is_empty());
    }
}
