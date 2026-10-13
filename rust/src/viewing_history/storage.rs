//! One worker owns the file; a coalescing mailbox bounds queued snapshots.
use super::Document;
use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex, mpsc},
    thread,
};
const MAX_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub(super) enum Error {
    #[error("Viewing history file operation failed: {0}")]
    Io(#[from] io::Error),
    #[error("Viewing history could not be decoded: {0}")]
    Parse(#[from] crate::json::Error),
    #[error("Viewing history could not be encoded: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("Viewing history exceeds its limits or contains invalid entries")]
    Invalid,
}
impl Error {
    pub(super) fn diagnostic(&self) -> String {
        match self {
            Self::Parse(error) => {
                let (path, source) = match error {
                    crate::json::Error::Decode { path, source } => (path.to_string(), source),
                    crate::json::Error::Trailing(source) => ("root".into(), source),
                };
                // Serde's message can quote a stored HTTP password or token.
                // Report its category and location without quoting input values.
                format!(
                    "Viewing history JSON is invalid at {path}: {:?}, line {}, column {}",
                    source.classify(),
                    source.line(),
                    source.column()
                )
            }
            Self::Io(_) | Self::Serialize(_) | Self::Invalid => self.to_string(),
        }
    }
    fn report(&self, operation: &str) {
        match self {
            Self::Parse(_) => {
                tracing::error!(operation, cause = %self.diagnostic(), "Viewing history operation failed")
            }
            Self::Io(_) | Self::Serialize(_) | Self::Invalid => tracing::error!(
                operation,
                error = self as &dyn std::error::Error,
                "Viewing history operation failed"
            ),
        }
    }
}
pub(super) enum Update {
    Loaded(Result<Document, Error>),
    Saved(Result<(), Error>),
}
enum Work {
    Active(Option<Document>),
    Closing(Option<Document>),
}
struct Mailbox {
    work: Mutex<Work>,
    wake: Condvar,
}
pub(super) struct Worker {
    mailbox: Arc<Mailbox>,
    updates: mpsc::Receiver<Update>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Worker {
    pub fn start(path: PathBuf) -> Result<Self, Error> {
        let mailbox = Arc::new(Mailbox {
            work: Mutex::new(Work::Active(None)),
            wake: Condvar::new(),
        });
        let (sender, updates) = mpsc::channel();
        let worker_mailbox = mailbox.clone();
        let thread = thread::Builder::new()
            .name("viewing-history".into())
            .spawn(move || {
                let loaded = load(&path);
                let writable = loaded.is_ok();
                if let Err(error) = &loaded {
                    error.report("Load viewing history; preserve existing file on failure");
                }
                if sender.send(Update::Loaded(loaded)).is_err() {
                    // The owner was destroyed before initialization completed.
                    return;
                }
                loop {
                    let (data, closing) = {
                        let mut work = worker_mailbox
                            .work
                            .lock()
                            .expect("history mailbox poisoned");
                        while matches!(*work, Work::Active(None)) {
                            work = worker_mailbox
                                .wake
                                .wait(work)
                                .expect("history mailbox poisoned");
                        }
                        match &mut *work {
                            Work::Active(data) => (data.take(), false),
                            Work::Closing(data) => (data.take(), true),
                        }
                    };
                    if writable && let Some(data) = data {
                        let result = save(&path, &data);
                        if let Err(error) = &result {
                            error.report("Save viewing history");
                        }
                        if sender.send(Update::Saved(result)).is_err() {
                            // Shutdown may release the receiver after its final snapshot.
                        }
                    }
                    if closing {
                        break;
                    }
                }
            })?;
        Ok(Self {
            mailbox,
            updates,
            thread: Some(thread),
        })
    }
    pub fn save(&self, data: Document) {
        let mut work = self.mailbox.work.lock().expect("history mailbox poisoned");
        if let Work::Active(pending) = &mut *work {
            *pending = Some(data);
        }
        self.mailbox.wake.notify_one();
    }
    pub fn poll(&self) -> Option<Update> {
        // No snapshot is normal between saves. Worker panics are reported on join.
        self.updates.try_recv().ok()
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        {
            let mut work = self.mailbox.work.lock().expect("history mailbox poisoned");
            let pending = match &mut *work {
                Work::Active(pending) | Work::Closing(pending) => pending.take(),
            };
            *work = Work::Closing(pending);
            self.mailbox.wake.notify_one();
        }
        // Destruction follows the UI event loop; preserve the last accepted save.
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            tracing::error!("Viewing history worker panicked during shutdown");
        }
    }
}
fn load(path: &Path) -> Result<Document, Error> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Document::default()),
        Err(error) => return Err(error.into()),
    };
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(Error::Invalid);
    }
    let data: Document = crate::json::from_slice(&bytes)?;
    data.validate()?;
    Ok(data)
}
fn save(path: &Path, data: &Document) -> Result<(), Error> {
    data.validate()?;
    let bytes = serde_json::to_vec(data)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(Error::Invalid);
    }
    let parent = path.parent().ok_or(Error::Invalid)?;
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(&bytes)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_saved_values_do_not_leak_into_diagnostics() {
        let error = crate::json::from_slice::<super::super::recordings::Source>(
            br#"{"kind":"private-password"}"#,
        )
        .unwrap_err();
        let message = Error::Parse(error).diagnostic();
        assert!(message.contains("JSON is invalid"));
        assert!(!message.contains("private-password"));
    }
    #[test]
    fn recording_identity_and_position_survive_atomic_replacement() {
        use super::super::recordings::{Entry, Identity, Position, Source};
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.json");
        let identity = Identity::new(
            Source::Epgstation {
                server: "http://example.test".into(),
                recorded_id: u64::MAX,
                video_file_id: 42,
            },
            "Recording".into(),
        );
        let position = Position::new(120_000.0, 600_000.0).unwrap();
        let data = Document {
            next_key: 1,
            recordings: vec![Entry {
                key: "recording:1".into(),
                identity: identity.clone(),
                position,
            }],
            ..Default::default()
        };
        save(&path, &data).unwrap();
        let restored = load(&path).unwrap();
        assert_eq!(restored.recordings[0].identity, identity);
        assert_eq!(restored.recordings[0].position, position);
        assert_eq!(restored.next_key, 1);
    }
    #[test]
    fn last_snapshot_survives_shutdown_and_corrupt_files_are_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        let worker = Worker::start(path.clone()).unwrap();
        assert!(matches!(
            worker.updates.recv().unwrap(),
            Update::Loaded(Ok(_))
        ));
        worker.save(Document {
            favorites: vec![u64::MAX, 1],
            ..Default::default()
        });
        worker.save(Document {
            favorites: vec![1, u64::MAX],
            ..Default::default()
        });
        drop(worker);
        assert_eq!(load(&path).unwrap().favorites, vec![1, u64::MAX]);
        fs::write(&path, b"broken").unwrap();
        let worker = Worker::start(path.clone()).unwrap();
        assert!(matches!(
            worker.updates.recv().unwrap(),
            Update::Loaded(Err(_))
        ));
        worker.save(Document::default());
        drop(worker);
        assert_eq!(fs::read(&path).unwrap(), b"broken");
    }
}
