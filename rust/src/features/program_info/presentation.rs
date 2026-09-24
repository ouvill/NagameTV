//! Change detection prevents re-serializing descriptions on every progress tick.
use super::{
    model::Snapshot,
    schedule::{End, Resolution, Segment},
};
use crate::channels::BroadcastService;

#[derive(PartialEq, Eq)]
struct Key {
    revision: u64,
    service: Option<BroadcastService>,
    program: Option<(u64, u64, u64)>,
    segment: Option<Segment>,
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
            segment: snapshot.segment(service, now).copied(),
            program: program.map(|p| (p.id, p.start_at, p.duration)),
        };
        let data = if self.key.as_ref() != Some(&key) {
            Some(match snapshot.resolution(service, now) {
                Resolution::Conflict(_) => serde_json::to_string(
                    &serde_json::json!({"name":null,"scheduleState":"conflict","startAt":null,"duration":null}),
                )?,
                Resolution::Single(index) if snapshot.program(index).end() == End::Unknown => {
                    let mut value = serde_json::to_value(snapshot.program(index))?;
                    value["scheduleState"] = "unknownEnd".into();
                    value["endUnknown"] = true.into();
                    serde_json::to_string(&value)?
                }
                Resolution::Single(_) | Resolution::Gap => serde_json::to_string(&program)?,
            })
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
