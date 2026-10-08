//! Persistent update-check times, separate from user preferences.
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsString,
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub(super) const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const MAX_HISTORY_BYTES: u64 = 4096;
const MAX_UNIX_SECONDS: u64 = 253_402_300_799; // Last second representable in year 9999.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Update history directory is unavailable")]
    Directory,
    #[error("Update history operation failed ({path}): {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("Could not parse update history: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("Could not serialize update history: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("Update history is too large")]
    TooLarge,
    #[error("Update history contains invalid timestamps")]
    InvalidTimes,
    #[error("System clock is before the Unix epoch: {0}")]
    Clock(#[from] std::time::SystemTimeError),
}

#[derive(Clone, Default, Debug, PartialEq, Eq, Deserialize, Serialize)]
struct Record {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_attempt_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_success_at: Option<u64>,
}

#[derive(Default)]
enum Storage {
    #[default]
    Memory,
    File(PathBuf),
    Blocked,
}

#[derive(Default)]
pub(super) struct History {
    record: Record,
    storage: Storage,
    error: Option<Error>,
}

pub(super) fn path() -> Result<PathBuf, Error> {
    resolve_path(std::env::var_os("XDG_STATE_HOME"), std::env::var_os("HOME"))
}

fn resolve_path(state: Option<OsString>, home: Option<OsString>) -> Result<PathBuf, Error> {
    let base = state
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            home.map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .map(|home| home.join(".local/state"))
        })
        .ok_or(Error::Directory)?;
    Ok(base.join("nagametv/updates.toml"))
}

impl History {
    pub(super) fn open(path: Result<PathBuf, Error>) -> Self {
        let mut history = Self::default();
        match path.and_then(|path| read(&path).map(|record| (path, record))) {
            Ok((path, record)) => {
                history.record = record;
                history.storage = Storage::File(path);
            }
            Err(error) => history.block(error),
        }
        history
    }

    fn block(&mut self, error: Error) {
        tracing::error!(
            error = &error as &dyn std::error::Error,
            "Update history persistence failed"
        );
        // Preserve unreadable files; a default record must never overwrite them.
        self.storage = Storage::Blocked;
        self.error = Some(error);
    }

    pub(super) fn due(&self, now: SystemTime) -> bool {
        if !matches!(self.storage, Storage::File(_)) {
            return false;
        }
        self.record.last_attempt_at.is_none_or(|last| {
            let previous = UNIX_EPOCH + Duration::from_secs(last);
            // A clock correction must not postpone checking until a future date.
            now < previous || now >= previous + CHECK_INTERVAL
        })
    }

    pub(super) fn attempt(&mut self, now: SystemTime) {
        match now.duration_since(UNIX_EPOCH) {
            Ok(now) => {
                let now = now.as_secs();
                self.record.last_attempt_at = Some(now);
                if self
                    .record
                    .last_success_at
                    .is_some_and(|success| success > now)
                {
                    self.record.last_success_at = None;
                }
                self.persist();
            }
            Err(error) => self.block(error.into()),
        }
    }

    pub(super) fn succeeded(&mut self, now: SystemTime) {
        match now.duration_since(UNIX_EPOCH) {
            Ok(now) => {
                let now = now.as_secs();
                if self
                    .record
                    .last_attempt_at
                    .is_some_and(|attempt| attempt > now)
                {
                    self.record.last_attempt_at = Some(now);
                }
                self.record.last_success_at = Some(now);
                self.persist();
            }
            Err(error) => self.block(error.into()),
        }
    }

    fn persist(&mut self) {
        if let Storage::File(path) = &self.storage
            && let Err(error) = write(path, &self.record)
        {
            self.block(error);
        }
    }

    pub(super) fn last_success(&self) -> Option<u64> {
        self.record.last_success_at
    }
    pub(super) fn error(&self) -> Option<&Error> {
        self.error.as_ref()
    }
}

fn read(path: &Path) -> Result<Record, Error> {
    let io_error = |source| Error::Io {
        path: path.into(),
        source,
    };
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Record::default()),
        Err(error) => return Err(io_error(error)),
    };
    let mut text = String::new();
    file.take(MAX_HISTORY_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(io_error)?;
    if text.len() as u64 > MAX_HISTORY_BYTES {
        return Err(Error::TooLarge);
    }
    let record: Record = toml::from_str(&text)?;
    if record
        .last_attempt_at
        .is_some_and(|time| time > MAX_UNIX_SECONDS)
        || record
            .last_success_at
            .is_some_and(|time| time > MAX_UNIX_SECONDS)
        || (record.last_success_at.is_some() && record.last_attempt_at.is_none())
    {
        return Err(Error::InvalidTimes);
    }
    Ok(record)
}

fn write(path: &Path, record: &Record) -> Result<(), Error> {
    let parent = path.parent().ok_or(Error::Directory)?;
    let io_error = |source| Error::Io {
        path: path.into(),
        source,
    };
    let text = toml::to_string_pretty(record)?;
    fs::create_dir_all(parent).map_err(io_error)?;
    // Replacing a complete sibling file preserves the old history if saving fails.
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(io_error)?;
    temporary.write_all(text.as_bytes()).map_err(io_error)?;
    temporary.as_file().sync_all().map_err(io_error)?;
    temporary
        .persist(path)
        .map_err(|error| io_error(error.error))?;
    Ok(())
}

#[cfg(test)]
mod tests;
