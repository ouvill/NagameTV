//! Persisted, validated TS retention budgets (not a limit on decoder/process RSS).
use super::Retention;
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const MIN_CAPACITY_MIB: u32 = 16;
pub const MAX_CAPACITY_MIB: u32 = 65_536;
pub const MIN_RETENTION_MINUTES: u32 = 1;
pub const MAX_RETENTION_MINUTES: u32 = 180;
const MIB_BYTES: u64 = 1024 * 1024;
const SECONDS_PER_MINUTE: u64 = 60;
const DEFAULT_MEMORY_MIB: u32 = 256;
const DEFAULT_FILESYSTEM_MIB: u32 = 2048;
const DEFAULT_MINUTES: u32 = 30;
const FORWARD_BUFFER_BYTES: u64 = 2 * MIB_BYTES;
const FORWARD_BUFFER_TIME: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "SavedLimits", into = "SavedLimits")]
pub struct Limits(SavedLimits);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct SavedLimits {
    memory_mib: u32,
    filesystem_mib: u32,
    minutes: u32,
}
impl Default for SavedLimits {
    fn default() -> Self {
        Self {
            memory_mib: DEFAULT_MEMORY_MIB,
            filesystem_mib: DEFAULT_FILESYSTEM_MIB,
            minutes: DEFAULT_MINUTES,
        }
    }
}
impl Limits {
    pub fn new(memory_mib: u32, filesystem_mib: u32, minutes: u32) -> Option<Self> {
        Self::try_from(SavedLimits {
            memory_mib,
            filesystem_mib,
            minutes,
        })
        .ok()
    }
    pub fn memory_mib(self) -> u32 {
        self.0.memory_mib
    }
    pub fn filesystem_mib(self) -> u32 {
        self.0.filesystem_mib
    }
    pub fn minutes(self) -> u32 {
        self.0.minutes
    }
}
impl TryFrom<SavedLimits> for Limits {
    type Error = &'static str;
    fn try_from(value: SavedLimits) -> Result<Self, Self::Error> {
        if !(MIN_CAPACITY_MIB..=MAX_CAPACITY_MIB).contains(&value.memory_mib)
            || !(MIN_CAPACITY_MIB..=MAX_CAPACITY_MIB).contains(&value.filesystem_mib)
            || !(MIN_RETENTION_MINUTES..=MAX_RETENTION_MINUTES).contains(&value.minutes)
        {
            return Err("TS retention limits are out of range");
        }
        Ok(Self(value))
    }
}
impl From<Limits> for SavedLimits {
    fn from(value: Limits) -> Self {
        value.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Policy {
    storage: Retention,
    limits: Limits,
}
impl Policy {
    pub fn new(storage: Retention, limits: Limits) -> Self {
        Self { storage, limits }
    }
    pub fn storage(self) -> Retention {
        self.storage
    }
    pub(super) fn budget(self) -> (u64, Duration) {
        match self.storage {
            Retention::Off => (FORWARD_BUFFER_BYTES, FORWARD_BUFFER_TIME),
            Retention::Memory | Retention::Filesystem => {
                let mib = match self.storage {
                    Retention::Memory => self.limits.memory_mib(),
                    Retention::Filesystem => self.limits.filesystem_mib(),
                    Retention::Off => unreachable!(),
                };
                (
                    u64::from(mib) * MIB_BYTES,
                    Duration::from_secs(u64::from(self.limits.minutes()) * SECONDS_PER_MINUTE),
                )
            }
        }
    }
}
impl From<Retention> for Policy {
    fn from(storage: Retention) -> Self {
        Self::new(storage, Limits::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_saved_and_ui_budgets_cannot_construct_a_policy() {
        assert!(Limits::new(0, DEFAULT_FILESYSTEM_MIB, DEFAULT_MINUTES).is_none());
        assert!(Limits::new(DEFAULT_MEMORY_MIB, DEFAULT_FILESYSTEM_MIB, 0).is_none());
        assert!(serde_json::from_str::<Limits>(r#"{"memory_mib":0}"#).is_err());
        let limits: Limits = serde_json::from_str("{}").unwrap();
        assert_eq!(limits, Limits::default());
        assert_eq!(
            serde_json::from_str::<Limits>(&serde_json::to_string(&limits).unwrap()).unwrap(),
            limits
        );
    }
}
