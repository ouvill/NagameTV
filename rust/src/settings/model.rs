//! Preferences and normalization without filesystem, Qt or playback dependencies.
use super::{
    CommentFontSize, CommentOpacity, CommentSpeed, Language, ScreenshotDirectory, ScreenshotFormat,
    ScreenshotOptions,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// An explicit environment value overrides the saved preference for this launch.
/// Preserve the convention that only exactly "0" disables an explicit override.
pub fn autoplay_requested(saved: bool, value: Option<&str>) -> bool {
    value.map_or(saved, |value| value != "0")
}

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
        Self(100.0)
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_size: Option<super::WindowSize>,
    pub language: Language,
    pub server: String,
    pub service_id: String,
    pub autoplay: bool,
    pub live_buffer_ms: super::LiveBuffer,
    pub timeshift: crate::playback::input::Retention,
    pub timeshift_limits: crate::playback::input::Limits,
    pub screenshot_directory: ScreenshotDirectory,
    pub screenshot_format: ScreenshotFormat,
    pub screenshot_options: ScreenshotOptions,
    pub volume: Volume,
    // Keep the existing on-disk key, now solely a viewer's display preference.
    // Subtitle processing and EPG availability are selected by LaunchPlan.
    #[serde(rename = "subtitles_enabled")]
    pub show_subtitles: bool,
    pub subtitle_force_outline: bool,
    pub comments_enabled: bool,
    pub comment_cache_limit_mib: super::CommentCacheLimit,
    pub danmaku_enabled: bool,
    pub comment_font_size: CommentFontSize,
    pub comment_opacity: CommentOpacity,
    pub comment_speed: CommentSpeed,
    pub comment_presentation: viewer_comments::danmaku::Presentation,
    pub comment_shadow_enabled: bool,
    pub comment_send_on_enter: bool,
    // Preserve future settings until their
    // features are migrated; opening this version must not erase preferences.
    #[serde(flatten)]
    pub extra: BTreeMap<String, toml::Value>,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            window_size: None,
            language: Language::default(),
            server: String::new(),
            service_id: String::new(),
            autoplay: false,
            live_buffer_ms: Default::default(),
            timeshift: Default::default(),
            timeshift_limits: Default::default(),
            screenshot_directory: ScreenshotDirectory::default(),
            screenshot_format: ScreenshotFormat::default(),
            screenshot_options: ScreenshotOptions::default(),
            volume: Volume::default(),
            show_subtitles: false,
            subtitle_force_outline: false,
            // main receives history independently of the scrolling overlay.
            // Its settings have no comments_enabled field; preserve reception
            // when importing them, including danmaku_enabled=true preferences.
            comments_enabled: true,
            comment_cache_limit_mib: Default::default(),
            danmaku_enabled: false,
            comment_font_size: Default::default(),
            comment_opacity: Default::default(),
            comment_speed: Default::default(),
            comment_presentation: Default::default(),
            comment_shadow_enabled: true,
            comment_send_on_enter: false,
            extra: BTreeMap::new(),
        }
    }
}
impl Preferences {
    pub fn timeshift_policy(&self) -> crate::playback::input::Policy {
        crate::playback::input::Policy::new(self.timeshift, self.timeshift_limits)
    }
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
    pub(super) fn apply_overrides(&mut self, server: Option<String>, service: Option<String>) {
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
