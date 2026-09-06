use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;

const APPLICATION_DIRECTORY: &str = "mirakurun-viewer";
const SETTINGS_FILE: &str = "settings.toml";

pub use viewer_core::settings::Settings;
#[cfg(test)]
use viewer_core::settings::normalize_language;

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

pub fn load() -> Result<Settings, SettingsError> {
    load_from(&settings_path()?)
}
pub fn save(settings: &Settings) -> Result<(), SettingsError> {
    save_to(&settings_path()?, settings)
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
    let settings =
        toml::from_str::<Settings>(&contents).map_err(|source| SettingsError::Parse {
            path: path.into(),
            source,
        })?;
    Ok(settings.normalized())
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
    // In tests, unwrap/expect assert successful setup or an expected result.
    // Failures intentionally fail the test; they are not assumed impossible IO.
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
