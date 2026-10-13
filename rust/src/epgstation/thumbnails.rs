//! Authenticated image requests and a bounded cache; Qt sees only image bytes.
use super::{Endpoint, client};
use crate::services::{FetchError, Job, Network, NetworkError, Progress, Stopping};
use std::collections::{HashSet, VecDeque};

pub(super) const MAX_IMAGE_BYTES: usize = 512 * 1024;
const MAX_CACHE_BYTES: usize = 8 * 1024 * 1024;
const MAX_IMAGES: usize = 64;
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    pub server: String,
    pub id: u64,
}
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Thumbnail is not a JPEG image")]
    Format,
}
#[derive(Default)]
enum Work {
    #[default]
    Idle,
    Loading {
        key: Key,
        job: Job<Option<String>, Error>,
    },
    Stopping(Stopping),
}
#[derive(Default)]
pub(super) struct Cache {
    wanted: Vec<Key>,
    attempted: HashSet<Key>,
    entries: VecDeque<(Key, String)>,
    bytes: usize,
    work: Work,
}
impl Cache {
    pub fn configure(&mut self, wanted: impl IntoIterator<Item = Key>) {
        let mut unique = HashSet::new();
        self.wanted = wanted
            .into_iter()
            .filter(|key| unique.insert(key.clone()))
            .take(MAX_IMAGES)
            .collect();
        self.attempted.retain(|key| self.wanted.contains(key));
    }
    pub fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
        self.attempted.clear();
        self.work = match std::mem::take(&mut self.work) {
            Work::Loading { job, .. } => Work::Stopping(job.cancel()),
            Work::Idle => Work::Idle,
            Work::Stopping(stopping) => Work::Stopping(stopping),
        };
    }
    pub fn image(&self, key: &Key) -> &str {
        self.entries
            .iter()
            .find(|(cached, _)| cached == key)
            .map_or("", |(_, data)| data.as_str())
    }
    pub(super) fn poll(
        &mut self,
        network: &Network,
        session: Option<&(Endpoint, client::Connection)>,
    ) -> bool {
        let mut changed = false;
        self.work = match std::mem::take(&mut self.work) {
            Work::Idle => Work::Idle,
            Work::Stopping(stopping) => match stopping.poll() {
                Progress::Pending(stopping) => Work::Stopping(stopping),
                Progress::Complete(()) => Work::Idle,
            },
            Work::Loading { key, job } => match job.poll() {
                Progress::Pending(job) => Work::Loading { key, job },
                Progress::Complete(result) => {
                    match result {
                        Ok(Some(data)) if self.wanted.contains(&key) => {
                            self.insert(key, data);
                            changed = true;
                        }
                        Ok(Some(_)) | Ok(None) => {} // Replaced requests and absent optional images have no projection.
                        Err(error) => tracing::warn!(
                            error = &error as &dyn std::error::Error,
                            "Fetch recording thumbnail failed"
                        ),
                    }
                    Work::Idle
                }
            },
        };
        if matches!(self.work, Work::Idle)
            && let Some(key) = self
                .wanted
                .iter()
                .find(|key| !self.attempted.contains(*key) && self.image(key).is_empty())
                .cloned()
        {
            self.attempted.insert(key.clone());
            let endpoint = match Endpoint::parse(&key.server) {
                Ok(endpoint) => endpoint,
                Err(error) => {
                    tracing::warn!(
                        error = &error as &dyn std::error::Error,
                        "Prepare recording thumbnail request failed"
                    );
                    return changed;
                }
            };
            let connection = session
                .filter(|(server, _)| server == &endpoint)
                .map(|(_, connection)| connection.clone());
            let id = key.id;
            let job = network.job(async move { client::thumbnail(endpoint, connection, id).await });
            self.work = Work::Loading { key, job };
        }
        changed
    }
    fn insert(&mut self, key: Key, data: String) {
        while self.bytes + data.len() > MAX_CACHE_BYTES || self.entries.len() >= MAX_IMAGES {
            let Some((_, old)) = self.entries.pop_front() else {
                break;
            };
            self.bytes -= old.len();
        }
        self.bytes += data.len();
        self.entries.push_back((key, data));
    }
}
pub(super) fn network(error: reqwest::Error) -> FetchError<Error> {
    FetchError::Network(NetworkError::Http(error.without_url()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cleared_authentication_discards_an_already_completed_image() {
        let network = Network::new().unwrap();
        let key = Key {
            server: "http://example.test".into(),
            id: 1,
        };
        let mut cache = Cache {
            work: Work::Loading {
                key,
                job: network.job(async { Ok(Some("stale".into())) }),
            },
            ..Default::default()
        };
        cache.clear();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while !matches!(cache.work, Work::Idle) {
            assert!(!cache.poll(&network, None));
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert!(cache.entries.is_empty());
    }
    #[test]
    fn image_cache_and_desired_requests_have_fixed_memory_bounds() {
        let mut cache = Cache::default();
        cache.configure((0..1000).map(|id| Key {
            server: "http://example.test".into(),
            id,
        }));
        assert_eq!(cache.wanted.len(), MAX_IMAGES);
        for id in 0..100 {
            cache.insert(
                Key {
                    server: "http://example.test".into(),
                    id,
                },
                "x".repeat(MAX_IMAGE_BYTES),
            );
            assert!(cache.bytes <= MAX_CACHE_BYTES);
            assert!(cache.entries.len() <= MAX_IMAGES);
        }
        assert!(
            cache
                .image(&Key {
                    server: "http://example.test".into(),
                    id: 0
                })
                .is_empty()
        );
        assert_eq!(
            cache
                .image(&Key {
                    server: "http://example.test".into(),
                    id: 99
                })
                .len(),
            MAX_IMAGE_BYTES
        );
        cache.clear();
        assert_eq!(cache.bytes, 0);
    }
}
