//! Preferences and normalization without filesystem, Qt or playback dependencies.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Persisted as main's 0..100 percentage; the playback/UI boundary uses 0..1.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "f64", into = "f64")]
pub struct Volume(f64);
impl Volume {
    pub fn fraction(self) -> f64 {
        self.0 / 100.0
    }
    pub fn from_fraction(value: f64) -> Option<Self> {
        value
            .is_finite()
            .then(|| Self(value.clamp(0.0, 1.0) * 100.0))
    }
}
impl Default for Volume {
    fn default() -> Self {
        Self(70.0)
    }
}
impl From<f64> for Volume {
    fn from(value: f64) -> Self {
        if value.is_finite() {
            Self(value.clamp(0.0, 100.0))
        } else {
            Self::default()
        }
    }
}
impl From<Volume> for f64 {
    fn from(value: Volume) -> Self {
        value.0
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub server: String,
    pub service_id: String,
    pub volume: Volume,
    pub subtitles_enabled: bool,
    pub epg_enabled: bool,
    pub comments_enabled: bool,
    // Preserve main's language/comment settings and future fields until their
    // features are migrated; opening this version must not erase preferences.
    #[serde(flatten)]
    pub extra: BTreeMap<String, toml::Value>,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            server: "http://127.0.0.1:40772".into(),
            service_id: String::new(),
            volume: Volume::default(),
            subtitles_enabled: false,
            epg_enabled: true,
            comments_enabled: false,
            extra: BTreeMap::new(),
        }
    }
}
impl Preferences {
    pub fn selected_index(&self, ids: impl Iterator<Item = u64>) -> Option<usize> {
        let requested = self.service_id.parse::<u64>().ok();
        let mut first = None;
        for (index, id) in ids.enumerate() {
            first.get_or_insert(index);
            if Some(id) == requested {
                return Some(index);
            }
        }
        first
    }
    pub fn apply_overrides(&mut self, server: Option<String>, service: Option<String>) {
        if let Some(server) = server {
            if self.server != server {
                self.service_id.clear();
            }
            self.server = server;
        }
        if let Some(service) = service {
            self.service_id = service;
        }
    }
}
