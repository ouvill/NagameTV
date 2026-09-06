//! Fixed-size messages and one owned IO worker. No GUI, timers or runtime.
use crate::{Snapshot, measurement, storage};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

const QUEUE_SIZE: usize = 32;

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Event {
    Sample,
    PanelsChanged,
    PlaybackOptionsChanged,
    EpgFetchStarted,
    EpgFetchFinished,
    EpgFetchFailed,
    StopRequested,
    PlayRequested,
    ChannelSelected,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Enqueue {
    Accepted,
    Dropped,
    Stopped,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("診断ワーカーを開始できません: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("診断ログ保存が停止しました: {0}")]
    Storage(#[from] storage::Error),
    #[error("診断ワーカーがパニックで停止しました")]
    Panicked,
}

#[derive(Serialize)]
struct Entry {
    schema: u8,
    pid: u32,
    version: &'static str,
    unix_ms: u128,
    elapsed_ms: u128,
    event: Event,
    snapshot: Snapshot,
}

pub struct Recorder {
    sender: Option<mpsc::SyncSender<Entry>>,
    task: Option<JoinHandle<Result<(), Error>>>,
    dropped: Arc<AtomicU64>,
    started: Instant,
    version: &'static str,
}
impl Recorder {
    /// One recorder per process/directory, matching main's usage-<pid>.jsonl naming.
    pub fn start_directory(directory: PathBuf, version: &'static str) -> Result<Self, Error> {
        std::fs::create_dir_all(&directory).map_err(storage::Error::Io)?;
        crate::retention::prune(&directory).map_err(storage::Error::Io)?;
        Self::start(
            directory.join(format!("usage-{}.jsonl", std::process::id())),
            version,
        )
    }

    /// Opens the output at startup; all measurement and record writes run on the worker.
    pub fn start(path: PathBuf, version: &'static str) -> Result<Self, Error> {
        let mut writer = storage::RotatingWriter::new(path)?;
        Self::spawn(version, move |value| writer.write(value))
    }
    fn spawn(
        version: &'static str,
        mut write: impl FnMut(&serde_json::Value) -> Result<(), storage::Error> + Send + 'static,
    ) -> Result<Self, Error> {
        let (sender, receiver) = mpsc::sync_channel::<Entry>(QUEUE_SIZE);
        let dropped = Arc::new(AtomicU64::new(0));
        let worker_dropped = dropped.clone();
        let task = thread::Builder::new()
            .name("viewer-diagnostics".into())
            .spawn(move || {
                for entry in receiver {
                    let value = serde_json::json!({
                        "record": entry,
                        "measured_unix_ms": unix_ms(),
                        "process": measurement::process_memory(),
                        "allocator": measurement::allocator_memory(),
                        "dropped_records": worker_dropped.load(Ordering::Relaxed),
                    });
                    // An IO error is terminal, so no further writes follow a partial line.
                    write(&value)?;
                }
                Ok(())
            })
            .map_err(Error::Spawn)?;
        Ok(Self {
            sender: Some(sender),
            task: Some(task),
            dropped,
            started: Instant::now(),
            version,
        })
    }
    pub fn record(&self, event: Event, snapshot: Snapshot) -> Enqueue {
        let Some(sender) = &self.sender else {
            return Enqueue::Stopped;
        };
        let entry = Entry {
            schema: 1,
            pid: std::process::id(),
            version: self.version,
            unix_ms: unix_ms(),
            elapsed_ms: self.started.elapsed().as_millis(),
            event,
            snapshot,
        };
        match sender.try_send(entry) {
            Ok(()) => Enqueue::Accepted,
            Err(mpsc::TrySendError::Full(_)) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
                Enqueue::Dropped
            }
            Err(mpsc::TrySendError::Disconnected(_)) => Enqueue::Stopped,
        }
    }
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
    pub fn is_finished(&self) -> bool {
        self.task.as_ref().is_none_or(JoinHandle::is_finished)
    }
    /// Stops admission immediately. The finite accepted queue drains on the worker.
    pub fn stop(mut self) -> Stopping {
        self.sender = None;
        Stopping {
            task: self.task.take(),
        }
    }
}

#[must_use = "join the worker at shutdown or poll is_finished before joining"]
pub struct Stopping {
    task: Option<JoinHandle<Result<(), Error>>>,
}
impl Stopping {
    pub fn is_finished(&self) -> bool {
        self.task.as_ref().is_none_or(JoinHandle::is_finished)
    }
    pub fn join(mut self) -> Result<(), Error> {
        join(self.task.take())
    }
}
fn join(task: Option<JoinHandle<Result<(), Error>>>) -> Result<(), Error> {
    match task {
        Some(task) => task.join().map_err(|_| Error::Panicked)?,
        None => Ok(()),
    }
}
impl Drop for Recorder {
    fn drop(&mut self) {
        // Explicit stop allows nonblocking polling. Fallback Drop still owns cleanup;
        // slow filesystem IO can delay shutdown, but never leaves a detached worker.
        self.sender = None;
        let _ = join(self.task.take());
    }
}
impl Drop for Stopping {
    fn drop(&mut self) {
        let _ = join(self.task.take());
    }
}
fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |value| value.as_millis())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slow_writer_bounds_admission_and_stop_drains_the_accepted_queue()
    -> Result<(), Box<dyn std::error::Error>> {
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let writes = Arc::new(AtomicU64::new(0));
        let count = writes.clone();
        let recorder = Recorder::spawn("test", move |_| {
            if count.fetch_add(1, Ordering::Relaxed) == 0 {
                entered_tx.send(()).map_err(std::io::Error::other)?;
                release_rx.recv().map_err(std::io::Error::other)?;
            }
            Ok(())
        })?;
        // Drop this sender before Recorder during assertion unwinding, so the
        // deliberately paused worker is released even when the test fails.
        let release = release_tx;
        assert_eq!(
            recorder.record(Event::Sample, Snapshot::default()),
            Enqueue::Accepted
        );
        entered_rx.recv_timeout(std::time::Duration::from_secs(2))?;
        for _ in 0..QUEUE_SIZE {
            assert_eq!(
                recorder.record(Event::Sample, Snapshot::default()),
                Enqueue::Accepted
            );
        }
        for _ in 0..1000 {
            assert_eq!(
                recorder.record(Event::Sample, Snapshot::default()),
                Enqueue::Dropped
            );
        }
        assert_eq!(recorder.dropped(), 1000);
        let stopping = recorder.stop();
        let release = release; // Also precede Stopping cleanup on assertion failure.
        assert!(!stopping.is_finished());
        release.send(())?;
        stopping.join()?;
        assert_eq!(writes.load(Ordering::Relaxed), QUEUE_SIZE as u64 + 1);
        Ok(())
    }
    #[test]
    fn writes_final_snapshot_before_join_returns() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("usage.jsonl");
        let recorder = Recorder::start(path.clone(), "application-test")?;
        assert_eq!(
            recorder.record(
                Event::ChannelSelected,
                Snapshot {
                    channel_count: 42,
                    ..Default::default()
                }
            ),
            Enqueue::Accepted
        );
        recorder.stop().join()?;
        let record: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
        assert_eq!(record["record"]["event"], "channel_selected");
        assert_eq!(record["record"]["version"], "application-test");
        assert_eq!(record["record"]["snapshot"]["channel_count"], 42);
        assert!(record["process"].is_object());
        Ok(())
    }
    #[test]
    fn worker_panic_is_an_error_instead_of_panicking_during_join()
    -> Result<(), Box<dyn std::error::Error>> {
        let recorder = Recorder::spawn("test", |_| panic!("injected worker failure"))?;
        assert_eq!(
            recorder.record(Event::Sample, Snapshot::default()),
            Enqueue::Accepted
        );
        assert!(matches!(recorder.stop().join(), Err(Error::Panicked)));
        Ok(())
    }

    #[test]
    fn worker_failure_is_returned_at_join() -> Result<(), Box<dyn std::error::Error>> {
        let recorder = Recorder::spawn("test", |_| {
            Err(storage::Error::Io(std::io::Error::other("write failed")))
        })?;
        assert_eq!(
            recorder.record(Event::Sample, Snapshot::default()),
            Enqueue::Accepted
        );
        assert!(matches!(
            recorder.stop().join(),
            Err(Error::Storage(storage::Error::Io(_)))
        ));
        Ok(())
    }
}
