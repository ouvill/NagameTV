//! Change detection prevents re-serializing descriptions on every progress tick.
use super::model::Snapshot;
use crate::channels::BroadcastService;

#[derive(PartialEq, Eq)]
struct Key {
    revision: u64,
    service: Option<BroadcastService>,
    program: Option<(u64, u64, u64)>,
}
#[derive(Default)]
pub struct Projection {
    key: Option<Key>,
}
pub struct Update {
    pub data: Option<String>,
    pub progress: f64,
}
impl Projection {
    pub fn is_stale(&self, revision: u64, service: Option<BroadcastService>) -> bool {
        self.key
            .as_ref()
            .is_none_or(|key| key.revision != revision || key.service != service)
    }
    pub(super) fn update(
        &mut self,
        snapshot: &Snapshot,
        revision: u64,
        service: Option<BroadcastService>,
        now: u64,
    ) -> Result<Update, serde_json::Error> {
        let program = snapshot.current(service, now);
        let key = Key {
            revision,
            service,
            program: program.map(|p| (p.id, p.start_at, p.duration)),
        };
        let data = if self.key.as_ref() != Some(&key) {
            Some(serde_json::to_string(&program)?)
        } else {
            None
        };
        self.key = Some(key);
        Ok(Update {
            data,
            progress: program.map_or(0.0, |p| p.progress(now)),
        })
    }
}
