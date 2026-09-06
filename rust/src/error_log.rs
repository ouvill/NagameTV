//! Latest playback failure on disk. Qt supplies the platform state directory.
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

const MAX_BYTES: usize = 64 * 1024;
const TRUNCATED: &str = "\n[truncated]\n";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("ログ保存先の絶対パスを取得できません")]
    InvalidDirectory,
    #[error("ログの操作に失敗しました ({path}): {source}")]
    Io { path: PathBuf, source: io::Error },
}

pub struct ErrorLog {
    directory: PathBuf,
}
impl ErrorLog {
    pub fn new(directory: PathBuf) -> Result<Self, Error> {
        if !directory.is_absolute() {
            return Err(Error::InvalidDirectory);
        }
        let log = Self { directory };
        log.prepare()?;
        Ok(log)
    }
    pub fn directory(&self) -> &Path {
        &self.directory
    }
    pub fn prepare(&self) -> Result<(), Error> {
        fs::create_dir_all(&self.directory).map_err(|source| Error::Io {
            path: self.directory.clone(),
            source,
        })
    }
    pub fn save(&self, text: &str) -> Result<(), Error> {
        self.prepare()?;
        let path = self.directory.join("playback-error.log");
        let write = || -> io::Result<()> {
            // Replacing one file bounds disk use. A failed write leaves the last
            // complete report intact, and concurrent instances have unique temps.
            let mut file = tempfile::NamedTempFile::new_in(&self.directory)?;
            if text.len() < MAX_BYTES {
                file.write_all(text.as_bytes())?;
                file.write_all(b"\n")?;
            } else {
                let end = text.floor_char_boundary(MAX_BYTES - TRUNCATED.len());
                file.write_all(&text.as_bytes()[..end])?;
                file.write_all(TRUNCATED.as_bytes())?;
            }
            file.flush()?;
            file.persist(&path).map_err(|error| error.error)?;
            Ok(())
        };
        write().map_err(|source| Error::Io { path, source })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replaces_latest_report_and_bounds_utf8_without_temporary_residue()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let log = ErrorLog::new(dir.path().join("state"))?;
        log.save("first")?;
        log.save(&"日本語".repeat(MAX_BYTES))?;
        let path = log.directory().join("playback-error.log");
        let text = fs::read_to_string(&path)?;
        assert!(text.len() <= MAX_BYTES);
        assert!(text.ends_with(TRUNCATED));
        log.save("second")?;
        assert_eq!(fs::read_to_string(path)?, "second\n");
        assert_eq!(fs::read_dir(log.directory())?.count(), 1);
        Ok(())
    }
    #[test]
    fn invalid_destination_is_reported_and_failed_replace_cleans_temp()
    -> Result<(), Box<dyn std::error::Error>> {
        assert!(matches!(
            ErrorLog::new(PathBuf::new()),
            Err(Error::InvalidDirectory)
        ));
        assert!(matches!(
            ErrorLog::new("relative".into()),
            Err(Error::InvalidDirectory)
        ));
        let dir = tempfile::tempdir()?;
        let log = ErrorLog::new(dir.path().to_owned())?;
        let target = dir.path().join("playback-error.log");
        fs::create_dir(&target)?;
        fs::write(target.join("keep"), "existing")?;
        assert!(matches!(log.save("failure"), Err(Error::Io { .. })));
        assert_eq!(fs::read_to_string(target.join("keep"))?, "existing");
        assert_eq!(fs::read_dir(dir.path())?.count(), 1);
        Ok(())
    }
}
