//! Bounded caches for program bodies. Playback position and progress stay outside
//! the cache key; text, station, clock mapping and EPG revisions invalidate it.
use super::Program;
use serde::{Serialize, Serializer};
use serde_json::{Value, value::RawValue};
use std::{collections::VecDeque, sync::Arc};

// Watching, receive edge and oldest retained program. Never retain a day's EPG.
const MAX_PROGRAM_BODIES: usize = 3;

#[derive(Debug)]
struct Body {
    value: Value,
    encoded: Box<RawValue>,
}
#[derive(Clone, Debug)]
pub(crate) struct Data(Arc<Body>);
impl Data {
    fn new(value: Value) -> Self {
        let encoded = serde_json::value::to_raw_value(&value).expect("finite program body");
        Self(Arc::new(Body { value, encoded }))
    }
    pub fn json(&self) -> &str {
        self.0.encoded.get()
    }
    pub fn title(&self) -> &str {
        self.0.value["name"].as_str().unwrap_or_default()
    }
    fn same_body(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl PartialEq for Data {
    fn eq(&self, other: &Self) -> bool {
        self.same_body(other) || self.json() == other.json()
    }
}
impl Serialize for Data {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.encoded.serialize(serializer)
    }
}

#[derive(PartialEq)]
struct Key {
    program: Program,
    station: String,
    provider: String,
    range: Option<(i64, i64)>,
    progress_known: bool,
}
#[derive(Default)]
pub(crate) struct Cache {
    bodies: VecDeque<(Key, Data)>,
}
impl Cache {
    pub fn project(
        &mut self,
        program: Program,
        station: String,
        provider: String,
        range: Option<(i64, i64)>,
        progress_known: bool,
    ) -> Data {
        let key = Key {
            program,
            station,
            provider,
            range,
            progress_known,
        };
        if let Some((_, data)) = self.bodies.iter().find(|(old, _)| *old == key) {
            return data.clone();
        }
        let mut value = serde_json::to_value(&key.program).expect("finite program data");
        value["source"] = "broadcast_ts".into();
        value["station"] = key.station.clone().into();
        value["provider"] = key.provider.clone().into();
        value["progressKnown"] = progress_known.into();
        value["playbackStartMs"] = range.map(|range| range.0).into();
        value["playbackEndMs"] = range.map(|range| range.1).into();
        if !key.program.extended.is_empty() {
            value["description"] =
                format!("{}\n\n{}", key.program.description, key.program.extended)
                    .trim()
                    .into();
        }
        let data = Data::new(value);
        if self.bodies.len() == MAX_PROGRAM_BODIES {
            self.bodies.pop_front();
        }
        self.bodies.push_back((key, data.clone()));
        data
    }
}

#[derive(Default)]
pub(crate) struct Enrichment {
    bodies: VecDeque<(Data, u64, Data)>,
}
impl Enrichment {
    pub fn apply(
        &mut self,
        original: &Data,
        revision: u64,
        enrich: impl FnOnce(&mut Value),
    ) -> Data {
        if let Some((_, _, result)) = self
            .bodies
            .iter()
            .find(|(data, old, _)| *old == revision && data.same_body(original))
        {
            return result.clone();
        }
        let mut value = original.0.value.clone();
        enrich(&mut value);
        let result = if value == original.0.value {
            original.clone()
        } else {
            Data::new(value)
        };
        if self.bodies.len() == MAX_PROGRAM_BODIES {
            self.bodies.pop_front();
        }
        self.bodies
            .push_back((original.clone(), revision, result.clone()));
        result
    }
}

#[derive(Default)]
enum Published {
    #[default]
    Unpublished,
    Current(Option<Data>),
}
#[derive(Default)]
pub(crate) struct Publication {
    current: Published,
}
impl Publication {
    pub fn update(&mut self, current: Option<Data>) -> bool {
        if matches!(&self.current, Published::Current(previous) if *previous == current) {
            return false;
        }
        self.current = Published::Current(current);
        true
    }
    pub fn json(&self) -> &str {
        match &self.current {
            Published::Unpublished | Published::Current(None) => "null",
            Published::Current(Some(data)) => data.json(),
        }
    }
}

#[cfg(test)]
mod tests;
