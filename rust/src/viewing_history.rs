//! Viewing preferences owned independently of Qt and persisted off the UI thread.
mod storage;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const MAX_FAVORITES: usize = 4096;

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
#[serde(default)]
struct Document {
    favorites: Vec<u64>,
}
impl Document {
    fn validate(&self) -> Result<(), storage::Error> {
        if self.favorites.len() > MAX_FAVORITES
            || self.favorites.iter().collect::<std::collections::HashSet<_>>().len()
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
    storage: Option<storage::Worker>,
    error: String,
}
impl History {
    pub fn transient() -> Self {
        Self { state: State::Ready(Document::default()), storage: None, error: String::new() }
    }
    pub fn open(path: PathBuf) -> Self {
        match storage::Worker::start(path) {
            Ok(storage) => Self { state: State::Loading, storage: Some(storage), error: String::new() },
            Err(error) => {
                tracing::error!(error = &error as &dyn std::error::Error, "Start viewing history worker failed");
                Self { error: error.to_string(), ..Self::transient() }
            }
        }
    }
    pub fn ready(&self) -> bool { matches!(self.state, State::Ready(_)) }
    pub fn error(&self) -> &str { &self.error }
    pub fn favorites(&self) -> &[u64] {
        match &self.state { State::Loading => &[], State::Ready(data) => &data.favorites }
    }
    pub fn toggle_favorite(&mut self, service: u64) -> bool {
        let State::Ready(data) = &mut self.state else { return false };
        if let Some(index) = data.favorites.iter().position(|id| *id == service) {
            data.favorites.remove(index);
        } else {
            if data.favorites.len() == MAX_FAVORITES { return false; }
            data.favorites.push(service);
        }
        self.save();
        true
    }
    pub fn move_favorite(&mut self, service: u64, position: usize) -> bool {
        let State::Ready(data) = &mut self.state else { return false };
        if position >= data.favorites.len() { return false; }
        let Some(index) = data.favorites.iter().position(|id| *id == service) else { return false };
        data.favorites.remove(index);
        data.favorites.insert(position, service);
        self.save();
        true
    }
    fn save(&mut self) {
        if let (State::Ready(data), Some(worker)) = (&self.state, &mut self.storage) {
            worker.save(data.clone());
        }
    }
    pub fn poll(&mut self) -> bool {
        let Some(worker) = &mut self.storage else { return false };
        let mut changed = false;
        while let Some(result) = worker.poll() {
            changed = true;
            match result {
                storage::Update::Loaded(Ok(data)) => self.state = State::Ready(data),
                storage::Update::Loaded(Err(error)) => {
                    self.state = State::Ready(Document::default());
                    self.error = error.to_string();
                }
                storage::Update::Saved(Ok(())) => self.error.clear(),
                storage::Update::Saved(Err(error)) => self.error = error.to_string(),
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
