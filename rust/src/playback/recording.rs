//! A readable local or HTTP transport stream, validated before replacing playback.
mod http;
#[cfg(test)]
pub(super) use http::tests::serve_ts;
pub(super) mod source;
use source::{Location, Source};
mod loader;
pub use loader::{Loader, Purpose, Request};
use std::{
    fs::File,
    io::Read,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

#[derive(Clone, Debug)]
pub struct Recording {
    source: Source,
    name: String,
    service: u16,
    inspection: Inspection,
}

// The asynchronous loader owns every filesystem operation needed before
// playback starts. Streaming and metadata workers open their own cursors later.
#[derive(Clone, Debug)]
pub(super) struct Inspection {
    pub prefix: Arc<Vec<u8>>,
    pub size: u64,
    pub framing: crate::transport::framing::Framing,
    pub deadline: Instant,
}
impl PartialEq for Recording {
    fn eq(&self, other: &Self) -> bool {
        self.source.location() == other.source.location()
            && self.service == other.service
            && self.name == other.name
    }
}
impl Eq for Recording {}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Select a local TS file.")]
    NotLocal,
    #[error("Enter a local file URL or a recording URL starting with http:// or https://.")]
    InvalidUrl,
    #[error("Invalid recording URL: {0}")]
    ParseUrl(#[from] url::ParseError),
    #[error("{0}")]
    Http(#[from] http::Error),
    #[error("Could not receive the dropped file: {0}. Use Open TS file to select it.")]
    Portal(String),
    #[error("Could not read the TS file: {0}")]
    Read(#[from] std::io::Error),
    #[error("No transport stream program was found in the beginning of this file.")]
    MissingProgram,
    #[error("TS file inspection reached its time limit. Try opening the file again.")]
    TimedOut,
    #[error("TS file inspection was cancelled.")]
    Cancelled,
    #[error("The TS file inspection worker stopped unexpectedly.")]
    WorkerStopped,
}

impl Recording {
    #[cfg(any(test, feature = "native_tests"))]
    pub fn open(path: &std::path::Path) -> Result<Self, Error> {
        Self::inspect(
            &Location::Local(path.to_owned()),
            &Arc::new(AtomicBool::new(false)),
        )
    }
    fn inspect(location: &Location, cancelled: &Arc<AtomicBool>) -> Result<Self, Error> {
        let (source, mut file, size, name) = match location {
            Location::Local(path) => {
                if !std::fs::metadata(path)?.is_file() {
                    return Err(Error::NotLocal);
                }
                let file = File::open(path)?;
                if !file.metadata()?.is_file() {
                    return Err(Error::NotLocal);
                }
                let size = file.metadata()?.len();
                let path = std::fs::canonicalize(path)?;
                let name = path
                    .file_name()
                    .ok_or(Error::NotLocal)?
                    .to_string_lossy()
                    .into_owned();
                (Source::Local(path), source::Reader::Local(file), size, name)
            }
            Location::Http(url) => {
                let file = http::Verified::inspect(url.clone(), cancelled.clone())?;
                let source = file.source().clone();
                let size = source.size();
                // Exclude credentials, query tokens and fragments from presentation.
                let name = format!("{}{}", url.host_str().unwrap_or_default(), url.path());
                (
                    Source::Http(Arc::new(source)),
                    source::Reader::Http(file),
                    size,
                    name,
                )
            }
        };
        let deadline = Instant::now() + super::input::EXPLORATION_TIMEOUT;
        // Bound memory and check cancellation between reads. Filesystem syscalls
        // themselves cannot be interrupted portably; the UI never waits on them.
        let mut prefix = Vec::new();
        const INSPECTION_READ_BYTES: usize = 64 * 1024;
        let mut chunk = [0; INSPECTION_READ_BYTES];
        const PROBE_LIMIT: usize = 4 * 1024 * 1024;
        while prefix.len() < PROBE_LIMIT && Instant::now() < deadline {
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
        let service = crate::transport::recording_service(&prefix).ok_or_else(|| {
            if Instant::now() >= deadline {
                Error::TimedOut
            } else {
                Error::MissingProgram
            }
        })?;
        let framing =
            crate::transport::framing::Framing::detect(&prefix).ok_or(Error::MissingProgram)?;
        Ok(Self {
            source,
            name,
            service,
            inspection: Inspection {
                prefix: Arc::new(prefix),
                size,
                framing,
                deadline,
            },
        })
    }

    pub(super) fn source(&self) -> &Source {
        &self.source
    }
    #[cfg(feature = "native_tests")]
    pub fn local_path(&self) -> Option<&std::path::Path> {
        match &self.source {
            Source::Local(path) => Some(path),
            Source::Http(_) => None,
        }
    }
    pub(super) fn inspection(&self) -> &Inspection {
        &self.inspection
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn service(&self) -> u16 {
        self.service
    }
    pub fn replay(&self) -> Request {
        Request::replay(self.source.location())
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
        assert_eq!(recording.source.location(), Location::Local(path.clone()));
        assert_eq!(recording.name(), "録画 #100%.ts");
        std::fs::write(&path, b"not a TS")?;
        assert!(matches!(Recording::open(&path), Err(Error::MissingProgram)));
        assert!(Recording::open(dir.path()).is_err());
        std::fs::remove_file(&path)?;
        assert!(Recording::open(&path).is_err());
        Ok(())
    }
}
