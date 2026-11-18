//! Explicit application storage roots for installations with a read-only OS.
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::OnceLock,
};

const ENV: &str = "NAGAMETV_DATA_DIR";
static OVERRIDE: OnceLock<DataDirectories> = OnceLock::new();

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{ENV} must be an absolute path (got {0:?})")]
    Relative(PathBuf),
    #[error("Could not prepare writable directory {path:?} for {ENV}: {source}")]
    Directory {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{ENV} was already initialized")]
    AlreadyInitialized,
}

pub struct DataDirectories {
    config: PathBuf,
    state: PathBuf,
    comment_archive: PathBuf,
    timeshift: PathBuf,
}

impl DataDirectories {
    fn prepare(root: &Path) -> Result<Self, Error> {
        if !root.is_absolute() {
            return Err(Error::Relative(root.into()));
        }
        let cache = root.join("cache");
        let directories = Self {
            config: root.join("config"),
            state: root.join("state"),
            comment_archive: cache.join("comment-archive"),
            timeshift: cache.join("timeshift"),
        };
        for path in [
            root,
            &directories.config,
            &directories.state,
            &cache,
            &directories.comment_archive,
            &directories.timeshift,
        ] {
            let prepare = || -> io::Result<()> {
                fs::create_dir_all(path)?;
                // The mount may be read-only even if its mode bits grant write access.
                let mut probe = tempfile::NamedTempFile::new_in(path)?;
                probe.write_all(b"\n")?;
                probe.close()
            };
            prepare().map_err(|source| Error::Directory {
                path: path.into(),
                source,
            })?;
        }
        Ok(directories)
    }

    pub fn config(&self) -> &Path {
        &self.config
    }

    pub fn state(&self) -> &Path {
        &self.state
    }

    pub fn comment_archive(&self) -> &Path {
        &self.comment_archive
    }

    pub fn timeshift(&self) -> &Path {
        &self.timeshift
    }
}

pub fn initialize() -> Result<(), Error> {
    if let Some(root) = std::env::var_os(ENV) {
        OVERRIDE
            .set(DataDirectories::prepare(Path::new(&root))?)
            .map_err(|_| Error::AlreadyInitialized)?;
    }
    Ok(())
}

pub fn get() -> Option<&'static DataDirectories> {
    OVERRIDE.get()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepares_the_layout_without_leaving_probe_files() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let root = directory.path().join("appliance");
        let paths = DataDirectories::prepare(&root)?;
        for (path, relative) in [
            (paths.config(), "config"),
            (paths.state(), "state"),
            (paths.comment_archive(), "cache/comment-archive"),
            (paths.timeshift(), "cache/timeshift"),
        ] {
            assert_eq!(path, root.join(relative));
            assert_eq!(fs::read_dir(path)?.count(), 0);
        }
        assert_eq!(fs::read_dir(&root)?.count(), 3);
        assert_eq!(fs::read_dir(root.join("cache"))?.count(), 2);
        Ok(())
    }

    #[test]
    fn rejects_an_unusable_cache_subdirectory() -> Result<(), Box<dyn std::error::Error>> {
        let root = tempfile::tempdir()?;
        let timeshift = root.path().join("cache/timeshift");
        fs::create_dir(root.path().join("cache"))?;
        fs::write(&timeshift, b"keep")?;
        assert!(matches!(
            DataDirectories::prepare(root.path()),
            Err(Error::Directory { path, .. }) if path == timeshift
        ));
        assert_eq!(fs::read(timeshift)?, b"keep");
        Ok(())
    }
}
