//! Startup policy, independent of Qt, command-line syntax and feature workers.
use std::str::FromStr;

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("invalid or repeated feature: {0} (expected none / subtitles / epg / comments)")]
pub struct FeatureError(String);

/// Independent feature choices. Only a validated allowlist can construct this value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeatureSet {
    subtitles: bool,
    epg: bool,
    comments: bool,
}

impl FromStr for FeatureSet {
    type Err = FeatureError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut features = Self {
            subtitles: false,
            epg: false,
            comments: false,
        };
        if value == "none" {
            return Ok(features);
        }
        for name in value.split(',') {
            match name {
                "subtitles" if !features.subtitles => features.subtitles = true,
                "epg" if !features.epg => features.epg = true,
                "comments" if !features.comments => features.comments = true,
                _ => return Err(FeatureError(name.to_owned())),
            }
        }
        Ok(features)
    }
}

/// Normal startup loads preferences; restricted startup uses transient settings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LaunchPlan {
    #[default]
    Preferences,
    Restricted(FeatureSet),
}

impl LaunchPlan {
    pub fn locked(self) -> bool {
        match self {
            Self::Preferences => false,
            Self::Restricted(_) => true,
        }
    }
    pub fn subtitles(self) -> bool {
        match self {
            Self::Preferences => true,
            Self::Restricted(features) => features.subtitles,
        }
    }
    pub fn epg(self) -> bool {
        match self {
            Self::Preferences => true,
            Self::Restricted(features) => features.epg,
        }
    }
    pub fn comments(self, preference: bool) -> bool {
        match self {
            Self::Preferences => preference,
            Self::Restricted(features) => features.comments,
        }
    }
}
