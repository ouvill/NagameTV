//! Bounded, compatible preference storage. IO occurs at startup and explicit save.
mod comment_style;
mod language;
mod model;
mod screenshot_directory;
pub use comment_style::{CommentFontSize, CommentOpacity, CommentSpeed};
pub use language::Language;
pub use model::{Preferences, Volume, autoplay_requested};
pub use screenshot_directory::ScreenshotDirectory;
use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

const MAX_SETTINGS_BYTES: u64 = 64 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("設定ディレクトリーが見つかりません")]
    MissingDirectory,
    #[error("設定ファイルの操作に失敗しました ({path}): {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("設定ファイルが上限の64KiBを超えています")]
    TooLarge,
    #[error("設定ファイルの解析に失敗しました ({path}): {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("設定ファイルの変換に失敗しました: {0}")]
    Serialize(#[from] toml::ser::Error),
}

pub fn settings_path() -> Result<PathBuf, Error> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|s| !s.is_empty())
                .map(|home| PathBuf::from(home).join(".config"))
        })
        .ok_or(Error::MissingDirectory)?;
    Ok(base.join("mirakurun-viewer/settings.toml"))
}

fn load(path: &Path) -> Result<Preferences, Error> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Preferences::default()),
        Err(source) => {
            return Err(Error::Io {
                path: path.into(),
                source,
            });
        }
    };
    let mut bytes = Vec::new();
    file.take(MAX_SETTINGS_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|source| Error::Io {
            path: path.into(),
            source,
        })?;
    if bytes.len() as u64 > MAX_SETTINGS_BYTES {
        return Err(Error::TooLarge);
    }
    let text = std::str::from_utf8(&bytes).map_err(|error| Error::Io {
        path: path.into(),
        source: io::Error::new(io::ErrorKind::InvalidData, error),
    })?;
    toml::from_str(text).map_err(|source| Error::Parse {
        path: path.into(),
        source,
    })
}

fn save(path: &Path, preferences: &Preferences) -> Result<(), Error> {
    let text = toml::to_string_pretty(preferences)?;
    if text.len() as u64 > MAX_SETTINGS_BYTES {
        return Err(Error::TooLarge);
    }
    let parent = path.parent().ok_or(Error::MissingDirectory)?;
    let io_error = |source| Error::Io {
        path: path.into(),
        source,
    };
    fs::create_dir_all(parent).map_err(io_error)?;
    // A unique file in the same directory avoids partial settings on failure and
    // collisions with another instance's temporary file. Persist consumes it;
    // any earlier failure removes the temporary file through RAII.
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(io_error)?;
    temporary.write_all(text.as_bytes()).map_err(io_error)?;
    temporary.as_file().sync_all().map_err(io_error)?;
    temporary
        .persist(path)
        .map_err(|error| io_error(error.error))?;
    Ok(())
}

/// A transient session has no writable path, including after a load error.
/// This prevents fallback defaults from replacing an unreadable/corrupt file.
pub struct Session {
    preferences: Preferences,
    persistence: Persistence,
}
/// Startup is the only phase accepting unverified addresses from disk/env.
/// Once activated, a server change requires proof from a successful probe.
pub struct Loaded(Session);

pub enum Change {
    Language(Language),
    Service(String),
    Autoplay(bool),
    Timeshift(crate::playback::input::Retention),
    TimeshiftLimits(crate::playback::input::Limits),
    ScreenshotDirectory(ScreenshotDirectory),
    Volume(Volume),
    SubtitleDisplay(bool),
    Comments(bool),
    Danmaku {
        enabled: bool,
        size: CommentFontSize,
        opacity: CommentOpacity,
        speed: CommentSpeed,
    },
    CommentShadow(bool),
    CommentSendOnEnter(bool),
}
#[derive(Debug, PartialEq, Eq)]
pub enum SaveStatus {
    Transient,
    Unchanged,
    Saved,
}
enum Persistence {
    Transient,
    File { path: PathBuf, saved: Preferences },
}
impl Loaded {
    pub fn open(path: PathBuf) -> Result<Self, Error> {
        let preferences = load(&path)?;
        Ok(Self(Session {
            persistence: Persistence::File {
                path,
                saved: preferences.clone(),
            },
            preferences,
        }))
    }
    pub fn transient(preferences: Preferences) -> Self {
        Self(Session {
            preferences,
            persistence: Persistence::Transient,
        })
    }
    pub fn preferences(&self) -> &Preferences {
        &self.0.preferences
    }
    pub fn activate(mut self, server: Option<String>, service: Option<String>) -> Session {
        self.0.preferences.apply_overrides(server, service);
        self.0
    }
}
impl Session {
    pub fn preferences(&self) -> &Preferences {
        &self.preferences
    }
    pub fn confirm_server(&mut self, server: &crate::services::VerifiedServer) {
        self.preferences
            .apply_overrides(Some(server.url().as_str().to_owned()), None);
    }
    pub fn change(&mut self, change: Change) {
        let preferences = &mut self.preferences;
        match change {
            Change::Language(language) => preferences.language = language,
            Change::Service(service) => preferences.service_id = service,
            Change::Autoplay(enabled) => preferences.autoplay = enabled,
            Change::Timeshift(retention) => preferences.timeshift = retention,
            Change::TimeshiftLimits(limits) => preferences.timeshift_limits = limits,
            Change::ScreenshotDirectory(directory) => preferences.screenshot_directory = directory,
            Change::Volume(volume) => preferences.volume = volume,
            Change::SubtitleDisplay(display) => preferences.show_subtitles = display,
            Change::Comments(enabled) => preferences.comments_enabled = enabled,
            Change::Danmaku {
                enabled,
                size,
                opacity,
                speed,
            } => {
                preferences.danmaku_enabled = enabled;
                preferences.comment_font_size = size;
                preferences.comment_opacity = opacity;
                preferences.comment_speed = speed;
            }
            Change::CommentShadow(enabled) => preferences.comment_shadow_enabled = enabled,
            Change::CommentSendOnEnter(enabled) => preferences.comment_send_on_enter = enabled,
        }
    }
    pub fn flush(&mut self) -> Result<SaveStatus, Error> {
        match &mut self.persistence {
            Persistence::Transient => Ok(SaveStatus::Transient),
            Persistence::File { path, saved } => {
                if *saved == self.preferences {
                    return Ok(SaveStatus::Unchanged);
                }
                save(path, &self.preferences)?;
                saved.clone_from(&self.preferences);
                Ok(SaveStatus::Saved)
            }
        }
    }
}

#[cfg(test)]
mod tests;
