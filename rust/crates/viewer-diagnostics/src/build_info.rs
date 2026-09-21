//! Immutable build identity, shared by the application and diagnostic records.
use serde::Serialize;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeState {
    Clean,
    Dirty,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Source {
    Git {
        commit: &'static str,
        worktree: WorktreeState,
    },
    Unavailable,
}

#[derive(Debug, Serialize)]
pub struct BuildInfo {
    pub version: &'static str,
    pub source: Source,
    pub built_unix_seconds: u64,
    pub target: &'static str,
    pub profile: &'static str,
    pub rustc: &'static str,
    /// Cargo's enabled CARGO_FEATURE_* names (without the prefix), sorted.
    pub features: &'static [&'static str],
}

/// Version-only clients remain supported; the application supplies the full build.
#[derive(Clone, Copy)]
pub enum Identity {
    Version(&'static str),
    Build(&'static BuildInfo),
}

impl Identity {
    pub(crate) fn version(self) -> &'static str {
        match self {
            Self::Version(version) => version,
            Self::Build(info) => info.version,
        }
    }

    pub(crate) fn build_info(self) -> Option<&'static BuildInfo> {
        match self {
            Self::Version(_) => None,
            Self::Build(info) => Some(info),
        }
    }
}

impl From<&'static str> for Identity {
    fn from(version: &'static str) -> Self {
        Self::Version(version)
    }
}

impl From<&'static BuildInfo> for Identity {
    fn from(info: &'static BuildInfo) -> Self {
        Self::Build(info)
    }
}
