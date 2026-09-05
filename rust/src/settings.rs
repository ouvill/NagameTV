use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;

const APPLICATION_DIRECTORY: &str = "mirakurun-viewer";
const SETTINGS_FILE: &str = "settings.toml";

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

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("could not determine the user configuration directory")]
    ConfigDirectoryUnavailable,
    #[error("could not read {path}: {source}")]
    Read { path: PathBuf, source: io::Error },
    #[error("invalid TOML in {path}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("could not encode settings as TOML: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("could not create {path}: {source}")]
    CreateDirectory { path: PathBuf, source: io::Error },
    #[error("could not write {path}: {source}")]
    Write { path: PathBuf, source: io::Error },
    #[error("could not replace {path}: {source}")]
    Replace { path: PathBuf, source: io::Error },
}

impl Settings {
    pub fn load() -> Result<Self, SettingsError> {
        load_from(&settings_path()?)
    }
    pub fn save(&self) -> Result<(), SettingsError> {
        save_to(&settings_path()?, self)
    }
}

pub fn settings_path() -> Result<PathBuf, SettingsError> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|value| !value.is_empty())
                .map(|home| PathBuf::from(home).join(".config"))
        })
        .ok_or(SettingsError::ConfigDirectoryUnavailable)?;
    Ok(base.join(APPLICATION_DIRECTORY).join(SETTINGS_FILE))
}

fn load_from(path: &Path) -> Result<Settings, SettingsError> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(Settings::default()),
        Err(source) => {
            return Err(SettingsError::Read {
                path: path.into(),
                source,
            });
        }
    };
    let mut settings =
        toml::from_str::<Settings>(&contents).map_err(|source| SettingsError::Parse {
            path: path.into(),
            source,
        })?;
    settings.volume = if settings.volume.is_finite() {
        settings.volume.clamp(0.0, 100.0)
    } else {
        Settings::default().volume
    };
    settings.comment_font_size = finite_clamped(settings.comment_font_size, 12.0, 48.0, 21.0);
    settings.comment_opacity = finite_clamped(settings.comment_opacity, 0.1, 1.0, 1.0);
    settings.comment_speed = finite_clamped(settings.comment_speed, 0.5, 2.0, 1.0);
    settings.language = normalize_language(&settings.language).to_owned();
    Ok(settings)
}

pub fn normalize_language(language: &str) -> &'static str {
    match language {
        "system" => "system",
        "ja" => "ja",
        _ => "en",
    }
}

fn finite_clamped(value: f64, minimum: f64, maximum: f64, fallback: f64) -> f64 {
    if value.is_finite() {
        value.clamp(minimum, maximum)
    } else {
        fallback
    }
}

fn save_to(path: &Path, settings: &Settings) -> Result<(), SettingsError> {
    let parent = path
        .parent()
        .ok_or(SettingsError::ConfigDirectoryUnavailable)?;
    fs::create_dir_all(parent).map_err(|source| SettingsError::CreateDirectory {
        path: parent.into(),
        source,
    })?;
    let contents = toml::to_string_pretty(settings)?;
    let temporary = path.with_extension("toml.tmp");
    fs::write(&temporary, contents).map_err(|source| SettingsError::Write {
        path: temporary.clone(),
        source,
    })?;
    fs::rename(&temporary, path).map_err(|source| SettingsError::Replace {
        path: path.into(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "mirakurun-viewer-{name}-{}-{}.toml",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ))
    }

    #[test]
    fn unsupported_language_falls_back_to_english() {
        assert_eq!(normalize_language("system"), "system");
        assert_eq!(normalize_language("ja"), "ja");
        assert_eq!(normalize_language("en"), "en");
        assert_eq!(normalize_language("fr"), "en");
        assert_eq!(normalize_language(""), "en");
    }

    #[test]
    fn missing_file_uses_defaults() {
        let path = test_path("missing");
        let _ = fs::remove_file(&path);
        assert_eq!(load_from(&path).unwrap(), Settings::default());
    }

    #[test]
    fn settings_round_trip_as_toml() {
        let path = test_path("round-trip");
        let expected = Settings {
            language: "ja".to_owned(),
            server: "http://mirakurun:40772".to_owned(),
            service_id: "3203246080".to_owned(),
            volume: 42.5,
            danmaku_enabled: true,
            comment_font_size: 28.0,
            comment_opacity: 0.7,
            comment_speed: 1.25,
            subtitles_enabled: true,
        };
        save_to(&path, &expected).unwrap();
        assert_eq!(load_from(&path).unwrap(), expected);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn old_file_can_omit_new_fields() {
        let path = test_path("partial");
        fs::write(&path, "server = 'http://example.test:40772'\n").unwrap();
        let settings = load_from(&path).unwrap();
        assert_eq!(settings.server, "http://example.test:40772");
        assert_eq!(settings.language, "system");
        assert_eq!(settings.volume, 70.0);
        assert!(!settings.danmaku_enabled);
        assert_eq!(settings.comment_font_size, 21.0);
        let _ = fs::remove_file(path);
    }
}
