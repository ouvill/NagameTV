//! One worker owns the file; a coalescing mailbox bounds queued snapshots.
use super::Document;
use std::{fs, io::{self, Read, Write}, path::{Path, PathBuf}, sync::{Arc, Condvar, Mutex, mpsc}, thread};
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
pub(super) enum Update { Loaded(Result<Document, Error>), Saved(Result<(), Error>) }
enum Work { Active(Option<Document>), Closing(Option<Document>) }
struct Mailbox { work: Mutex<Work>, wake: Condvar }
pub(super) struct Worker {
    mailbox: Arc<Mailbox>,
    updates: mpsc::Receiver<Update>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Worker {
    pub fn start(path: PathBuf) -> Result<Self, Error> {
        let mailbox = Arc::new(Mailbox { work: Mutex::new(Work::Active(None)), wake: Condvar::new() });
        let (sender, updates) = mpsc::channel();
        let worker_mailbox = mailbox.clone();
        let thread = thread::Builder::new().name("viewing-history".into()).spawn(move || {
            let loaded = load(&path);
            let writable = loaded.is_ok();
            if let Err(error) = &loaded {
                tracing::error!(error = error as &dyn std::error::Error, "Load viewing history failed; preserving existing file");
            }
            if sender.send(Update::Loaded(loaded)).is_err() {
                // The owner was destroyed before initialization completed.
                return;
            }
            loop {
                let (data, closing) = {
                    let mut work = worker_mailbox.work.lock().expect("history mailbox poisoned");
                    while matches!(*work, Work::Active(None)) {
                        work = worker_mailbox.wake.wait(work).expect("history mailbox poisoned");
                    }
                    match &mut *work {
                        Work::Active(data) => (data.take(), false),
                        Work::Closing(data) => (data.take(), true),
                    }
                };
                if writable && let Some(data) = data {
                    let result = save(&path, &data);
                    if let Err(error) = &result {
                        tracing::error!(error = error as &dyn std::error::Error, "Save viewing history failed");
                    }
                    if sender.send(Update::Saved(result)).is_err() {
                        // Shutdown may release the receiver after its final snapshot.
                    }
                }
                if closing { break; }
            }
        })?;
        Ok(Self { mailbox, updates, thread: Some(thread) })
    }
    pub fn save(&self, data: Document) {
        let mut work = self.mailbox.work.lock().expect("history mailbox poisoned");
        if let Work::Active(pending) = &mut *work { *pending = Some(data); }
        self.mailbox.wake.notify_one();
    }
    pub fn poll(&self) -> Option<Update> {
        match self.updates.try_recv() {
            Ok(update) => Some(update),
            Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected) => None,
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        {
            let mut work = self.mailbox.work.lock().expect("history mailbox poisoned");
            let pending = match &mut *work { Work::Active(pending) | Work::Closing(pending) => pending.take() };
            *work = Work::Closing(pending);
            self.mailbox.wake.notify_one();
        }
        // Destruction follows the UI event loop; preserve the last accepted save.
        if let Some(thread) = self.thread.take() && thread.join().is_err() {
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
    if bytes.len() as u64 > MAX_BYTES { return Err(Error::Invalid); }
    let data: Document = crate::json::from_slice(&bytes)?;
    data.validate()?;
    Ok(data)
}
fn save(path: &Path, data: &Document) -> Result<(), Error> {
    data.validate()?;
    let bytes = serde_json::to_vec(data)?;
    if bytes.len() as u64 > MAX_BYTES { return Err(Error::Invalid); }
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
    fn last_snapshot_survives_shutdown_and_corrupt_files_are_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        let worker = Worker::start(path.clone()).unwrap();
        assert!(matches!(worker.updates.recv().unwrap(), Update::Loaded(Ok(_))));
        worker.save(Document { favorites: vec![u64::MAX, 1] });
        worker.save(Document { favorites: vec![1, u64::MAX] });
        drop(worker);
        assert_eq!(load(&path).unwrap().favorites, vec![1, u64::MAX]);
        fs::write(&path, b"broken").unwrap();
        let worker = Worker::start(path.clone()).unwrap();
        assert!(matches!(worker.updates.recv().unwrap(), Update::Loaded(Err(_))));
        worker.save(Document::default());
        drop(worker);
        assert_eq!(fs::read(&path).unwrap(), b"broken");
    }
}
