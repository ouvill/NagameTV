//! A readable local transport stream, validated before replacing playback.
mod loader;
pub use loader::{Loader, Purpose, Request};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recording {
    path: PathBuf,
    uri: String,
    name: String,
    service: u16,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Select a local TS file.")]
    NotLocal,
    #[error("Could not read the TS file: {0}")]
    Read(#[from] std::io::Error),
    #[error("No transport stream program was found in the beginning of this file.")]
    MissingProgram,
    #[error("TS file inspection was cancelled.")]
    Cancelled,
    #[error("The TS file inspection worker stopped unexpectedly.")]
    WorkerStopped,
}

impl Recording {
    #[cfg(any(test, feature = "native_tests"))]
    pub fn open(path: &Path) -> Result<Self, Error> {
        Self::inspect(path, &AtomicBool::new(false))
    }
    fn inspect(path: &Path, cancelled: &AtomicBool) -> Result<Self, Error> {
        if !std::fs::metadata(path)?.is_file() {
            return Err(Error::NotLocal);
        }
        let mut file = File::open(path)?;
        if !file.metadata()?.is_file() {
            return Err(Error::NotLocal);
        }
        // Bound memory and check cancellation between reads. Filesystem syscalls
        // themselves cannot be interrupted portably; the UI never waits on them.
        let mut prefix = Vec::new();
        const INSPECTION_READ_BYTES: usize = 64 * 1024;
        let mut chunk = [0; INSPECTION_READ_BYTES];
        const PROBE_LIMIT: usize = 4 * 1024 * 1024;
        while prefix.len() < PROBE_LIMIT {
            if cancelled.load(Ordering::Relaxed) {
                return Err(Error::Cancelled);
            }
            let remaining = (PROBE_LIMIT - prefix.len()).min(chunk.len());
            let size = match file.read(&mut chunk[..remaining]) {
                Ok(size) => size,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.into()),
            };
            if size == 0 {
                break;
            }
            prefix.extend_from_slice(&chunk[..size]);
            if crate::transport::recording_service(&prefix).is_some() {
                break;
            }
        }
        if cancelled.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        let service = crate::transport::recording_service(&prefix).ok_or(Error::MissingProgram)?;
        let path = std::fs::canonicalize(path)?;
        let uri = url::Url::from_file_path(&path)
            .map_err(|_| Error::NotLocal)?
            .to_string();
        let name = path
            .file_name()
            .ok_or(Error::NotLocal)?
            .to_string_lossy()
            .into_owned();
        Ok(Self {
            path,
            uri,
            name,
            service,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
    #[cfg(test)]
    pub fn uri(&self) -> &str {
        &self.uri
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn service(&self) -> u16 {
        self.service
    }
    pub fn replay(&self) -> Request {
        Request::replay(self.path.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_ts_validation_and_url_escaping() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("録画 #100%.ts");
        std::fs::write(
            &path,
            include_bytes!("../../../tests/fixtures/subtitle-clock.ts"),
        )?;
        let recording = Recording::open(&path)?;
        assert_eq!(
            url::Url::parse(recording.uri())?.to_file_path().unwrap(),
            path
        );
        assert_eq!(recording.name(), "録画 #100%.ts");
        std::fs::write(&path, b"not a TS")?;
        assert!(matches!(Recording::open(&path), Err(Error::MissingProgram)));
        assert!(Recording::open(dir.path()).is_err());
        std::fs::remove_file(&path)?;
        assert!(Recording::open(&path).is_err());
        Ok(())
    }
}
