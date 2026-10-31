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
    screen_navigation: bool,
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
            screen_navigation: false,
        };
        if value == "none" {
            return Ok(features);
        }
        for name in value.split(',') {
            match name {
                "subtitles" if !features.subtitles => features.subtitles = true,
                "epg" if !features.epg => features.epg = true,
                "comments" if !features.comments => features.comments = true,
                "data-broadcast" if !features.data_broadcast => features.data_broadcast = true,
                "recording-reservations" if !features.recording_reservations => {
                    features.recording_reservations = true
                }
                "screen-navigation" if !features.screen_navigation => {
                    features.screen_navigation = true
                }
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
    pub fn screen_navigation(self, preference: bool) -> bool {
        match self {
            Self::Preferences => preference,
            Self::Restricted(features) => features.screen_navigation,
        }
    }
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
    fn navigation_respects_saved_defaults_and_explicit_allowlists() {
        assert!(!LaunchPlan::default().screen_navigation(false));
        assert!(LaunchPlan::default().screen_navigation(true));
        for names in ["none", "epg,recording-reservations"] {
            let plan = LaunchPlan::Restricted(names.parse().unwrap());
            assert!(!plan.screen_navigation(true));
            assert!(plan.locked());
        }
        let plan = LaunchPlan::Restricted("epg,screen-navigation".parse().unwrap());
        assert!(plan.screen_navigation(false) && plan.epg() && plan.locked());
        assert!(
            "screen-navigation,screen-navigation"
                .parse::<FeatureSet>()
                .is_err()
        );
    }
}
