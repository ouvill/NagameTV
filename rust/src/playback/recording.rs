//! Inspected local/HTTP media. Only transport streams carry a TS service and index.
mod broadcast;
mod http;
pub use broadcast::Broadcast;
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Recording {
    Transport(TransportStream),
    Media(MediaFile),
}

#[derive(Clone, Debug)]
pub struct TransportStream {
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
impl PartialEq for TransportStream {
    fn eq(&self, other: &Self) -> bool {
        self.source.location() == other.source.location()
            && self.service == other.service
            && self.name == other.name
    }
}
impl Eq for TransportStream {}

#[derive(Clone, Debug)]
pub struct MediaFile {
    source: Source,
    name: String,
    size: u64,
    caps: gstreamer::Caps,
    broadcast: Option<Broadcast>,
    external_subtitle: Option<crate::media_subtitles::Script>,
}
impl PartialEq for MediaFile {
    fn eq(&self, other: &Self) -> bool {
        self.source.location() == other.source.location()
            && self.name == other.name
            && self.size == other.size
            && self.caps == other.caps
            && self.broadcast == other.broadcast
            && self.external_subtitle == other.external_subtitle
    }
}
impl Eq for MediaFile {}
impl MediaFile {
    pub fn external_subtitle(&self) -> Option<&crate::media_subtitles::Script> {
        self.external_subtitle.as_ref()
    }
    pub(crate) fn broadcast(&self) -> Option<&Broadcast> {
        self.broadcast.as_ref()
    }
    pub(in crate::playback) fn source(&self) -> &Source {
        &self.source
    }
    pub(in crate::playback) fn size(&self) -> u64 {
        self.size
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Select a local video file.")]
    NotLocal,
    #[error("Enter a local file URL or a recording URL starting with http:// or https://.")]
    InvalidUrl,
    #[error("Invalid recording URL: {0}")]
    ParseUrl(#[from] url::ParseError),
    #[error("{0}")]
    Http(#[from] http::Error),
    #[error("Could not receive the dropped file: {0}. Use Open video file to select it.")]
    Portal(String),
    #[error("Could not read the video file: {0}")]
    Read(#[from] std::io::Error),
    #[error("No supported TS, MP4 or Matroska video was found in this file.")]
    MissingProgram,
    #[error("Video inspection reached its time limit. Try opening the file again.")]
    TimedOut,
    #[error("Video inspection was cancelled.")]
    Cancelled,
    #[error("The video inspection worker stopped unexpectedly.")]
    WorkerStopped,
    #[error("Could not initialize media inspection: {0}")]
    Initialization(#[from] gstreamer::glib::Error),
    #[error("The video file is too large")]
    TooLarge,
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
        if size > i64::MAX as u64 {
            return Err(Error::TooLarge);
        }
        let file_size = size;
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
            // Inspect content, never a filename or the server's MIME header. EPGStation
            // URLs contain only an ID, and a .ts filename can contain encoded media.
            if let Some(caps) = media_caps(&prefix)? {
                if cancelled.load(Ordering::Relaxed) {
                    return Err(Error::Cancelled);
                }
                return Ok(Self::Media(MediaFile {
                    source,
                    name,
                    size: file_size,
                    caps,
                    broadcast: None,
                    external_subtitle: None,
                }));
            }
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
        Ok(Self::Transport(TransportStream {
            source,
            name,
            service,
            inspection: Inspection {
                prefix: Arc::new(prefix),
                size,
                framing,
                deadline,
            },
        }))
    }
    pub(crate) fn retain_subtitle(&mut self, script: Option<crate::media_subtitles::Script>) {
        if let Self::Media(file) = self {
            file.external_subtitle = script;
        }
    }
    pub fn name(&self) -> &str {
        match self {
            Self::Transport(file) => &file.name,
            Self::Media(file) => &file.name,
        }
    }
    fn source(&self) -> &Source {
        match self {
            Self::Transport(file) => &file.source,
            Self::Media(file) => &file.source,
        }
    }
    pub fn replay(&self) -> Request {
        let mut request = Request::replay(
            self.source().location(),
            match self {
                Self::Media(file) => file.broadcast.clone(),
                Self::Transport(_) => None,
            },
        );
        if let Self::Media(file) = self {
            request.external_subtitle = file.external_subtitle.clone();
        }
        request
    }
    #[cfg(feature = "native_tests")]
    pub fn local_path(&self) -> Option<&std::path::Path> {
        match self.source() {
            Source::Local(path) => Some(path),
            Source::Http(_) => None,
        }
    }
}

// Typefinding parses headers only; no decoder or hardware resource is started.
fn media_caps(prefix: &[u8]) -> Result<Option<gstreamer::Caps>, Error> {
    gstreamer::init()?;
    let Ok((caps, probability)) =
        gstreamer_base::type_find_helper_for_data(None::<&gstreamer::Object>, prefix)
    else {
        return Ok(None);
    };
    let supported = caps.structure(0).is_some_and(|s| {
        matches!(
            s.name().as_str(),
            "video/quicktime" | "video/x-matroska" | "video/webm"
        )
    });
    Ok((supported && probability >= gstreamer::TypeFindProbability::Likely).then_some(caps))
}

impl TransportStream {
    pub(super) fn source(&self) -> &Source {
        &self.source
    }
    pub(super) fn inspection(&self) -> &Inspection {
        &self.inspection
    }
    pub fn service(&self) -> u16 {
        self.service
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
        assert_eq!(recording.source().location(), Location::Local(path.clone()));
        assert_eq!(recording.name(), "録画 #100%.ts");
        std::fs::write(&path, b"not a TS")?;
        assert!(matches!(Recording::open(&path), Err(Error::MissingProgram)));
        assert!(Recording::open(dir.path()).is_err());
        std::fs::remove_file(&path)?;
        assert!(Recording::open(&path).is_err());
        Ok(())
    }
}
