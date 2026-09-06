use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct Settings {
    pub language: String,
    pub server: String,
    pub service_id: String,
    pub volume: f64,
    pub danmaku_enabled: bool,
    pub comment_font_size: f64,
    pub comment_opacity: f64,
    pub comment_speed: f64,
    pub subtitles_enabled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: "system".to_owned(),
            server: "http://127.0.0.1:40772".to_owned(),
            service_id: String::new(),
            volume: 70.0,
            danmaku_enabled: false,
            comment_font_size: 21.0,
            comment_opacity: 1.0,
            comment_speed: 1.0,
            subtitles_enabled: false,
        }
    }
}

pub fn normalize_language(language: &str) -> &'static str {
    match language {
        "system" => "system",
        "ja" => "ja",
        _ => "en",
    }
}

impl Settings {
    /// Validate persisted or externally supplied preferences without performing IO.
    pub fn normalized(mut self) -> Self {
        self.volume = finite_clamped(self.volume, 0.0, 100.0, 70.0);
        self.comment_font_size = finite_clamped(self.comment_font_size, 12.0, 48.0, 21.0);
        self.comment_opacity = finite_clamped(self.comment_opacity, 0.1, 1.0, 1.0);
        self.comment_speed = finite_clamped(self.comment_speed, 0.5, 2.0, 1.0);
        self.language = normalize_language(&self.language).to_owned();
        self
    }
}

fn finite_clamped(value: f64, minimum: f64, maximum: f64, fallback: f64) -> f64 {
    if value.is_finite() {
        value.clamp(minimum, maximum)
    } else {
        fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalizes_all_preferences_without_io() {
        let settings = Settings {
            volume: f64::NAN,
            comment_font_size: 500.0,
            comment_opacity: f64::INFINITY,
            comment_speed: -1.0,
            language: "unknown".into(),
            ..Default::default()
        }
        .normalized();
        assert_eq!(settings.volume, 70.0);
        assert_eq!(settings.comment_font_size, 48.0);
        assert_eq!(settings.comment_opacity, 1.0);
        assert_eq!(settings.comment_speed, 0.5);
        assert_eq!(settings.language, "en");
    }
}
