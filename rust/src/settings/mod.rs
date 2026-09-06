//! Bounded, compatible preference storage. IO occurs at startup and explicit save.
mod comment_style;
mod language;
mod model;
pub use comment_style::{CommentFontSize, CommentOpacity, CommentSpeed};
pub use language::Language;
pub use model::{Preferences, Volume};
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
impl Session {
    pub fn open(path: PathBuf) -> Result<Self, Error> {
        let preferences = load(&path)?;
        Ok(Self {
            persistence: Persistence::File {
                path,
                saved: preferences.clone(),
            },
            preferences,
        })
    }
    pub fn transient(preferences: Preferences) -> Self {
        Self {
            preferences,
            persistence: Persistence::Transient,
        }
    }
    pub fn preferences(&self) -> &Preferences {
        &self.preferences
    }
    pub fn preferences_mut(&mut self) -> &mut Preferences {
        &mut self.preferences
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
