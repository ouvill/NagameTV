//! Startup policy, independent of Qt, command-line syntax and feature workers.
use std::str::FromStr;

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error(
    "invalid or repeated feature: {0} (expected none / subtitles / epg / comments / data-broadcast / recording-reservations / remote-web / screen-navigation)"
)]
pub struct FeatureError(String);

/// Independent feature choices. Only a validated allowlist can construct this value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeatureSet {
    subtitles: bool,
    epg: bool,
    comments: bool,
    data_broadcast: bool,
    recording_reservations: bool,
    remote_web: bool,
}

impl FromStr for FeatureSet {
    type Err = FeatureError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut features = Self {
            subtitles: false,
            epg: false,
            comments: false,
            data_broadcast: false,
            recording_reservations: false,
            remote_web: false,
        };
        if value == "none" {
            return Ok(features);
        }
        // Keep old launch scripts valid without retaining a navigation feature gate.
        let mut navigation_alias_seen = false;
        for name in value.split(',') {
            match name {
                "subtitles" if !features.subtitles => features.subtitles = true,
                "epg" if !features.epg => features.epg = true,
                "comments" if !features.comments => features.comments = true,
                "data-broadcast" if !features.data_broadcast => features.data_broadcast = true,
                "recording-reservations" if !features.recording_reservations => {
                    features.recording_reservations = true
                }
                "screen-navigation" if !navigation_alias_seen => navigation_alias_seen = true,
                "remote-web" if !features.remote_web => features.remote_web = true,
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
    pub fn remote_web(self, preference: bool) -> bool {
        match self {
            Self::Preferences => preference,
            Self::Restricted(features) => features.remote_web,
        }
    }
    pub fn recording_reservations(self, preference: bool) -> bool {
        match self {
            Self::Preferences => preference,
            Self::Restricted(features) => features.recording_reservations,
        }
    }
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
    pub fn data_broadcast(self, preference: bool) -> bool {
        match self {
            Self::Preferences => preference,
            Self::Restricted(features) => features.data_broadcast,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retired_navigation_name_does_not_change_the_feature_set() {
        for (names, equivalent) in [
            ("screen-navigation", "none"),
            ("epg,screen-navigation", "epg"),
            ("screen-navigation,epg,comments", "epg,comments"),
        ] {
            assert_eq!(
                names.parse::<FeatureSet>().unwrap(),
                equivalent.parse::<FeatureSet>().unwrap()
            );
        }
        for invalid in [
            "screen-navigation,screen-navigation",
            "none,screen-navigation",
            "screen-navigation,unknown",
            "screen-navigation,epg,epg",
        ] {
            assert!(invalid.parse::<FeatureSet>().is_err());
        }
    }
}
