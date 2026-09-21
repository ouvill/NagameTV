//! Bounded messages and one owned IO worker. No GUI, timers or runtime.
use crate::{Snapshot, build_info::Identity, measurement, storage};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex, Weak,
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

#[derive(Clone, Copy, Serialize)]
pub enum GcCategory {
    #[serde(rename = "qt.qml.gc.statistics")]
    Statistics,
    #[serde(rename = "qt.qml.gc.allocatorStats")]
    AllocatorStats,
}
impl GcCategory {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "qt.qml.gc.statistics" => Some(Self::Statistics),
            "qt.qml.gc.allocatorStats" => Some(Self::AllocatorStats),
            _ => None,
        }
    }
}
enum Message {
    Sample(Entry),
    Gc {
        unix_ms: u128,
        category: GcCategory,
        text: Box<str>,
    },
}
struct Sink {
    sender: Mutex<Option<mpsc::SyncSender<Message>>>,
    dropped: Arc<AtomicU64>,
}
impl Sink {
    fn close(&self) {
        // Only the sender's ownership is guarded. Recovering a poisoned guard
        // is safe for closing it; no partially updated application state is reused.
        let mut sender = self
            .sender
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        sender.take();
    }
    fn send(&self, message: Message) -> Enqueue {
        let guard = match self.sender.try_lock() {
            Ok(guard) => guard,
            Err(std::sync::TryLockError::WouldBlock) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
                return Enqueue::Dropped;
            }
            Err(std::sync::TryLockError::Poisoned(_)) => return Enqueue::Stopped,
        };
        let Some(sender) = guard.as_ref() else {
            return Enqueue::Stopped;
        };
        match sender.try_send(message) {
            Ok(()) => Enqueue::Accepted,
            Err(mpsc::TrySendError::Full(_)) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
                Enqueue::Dropped
            }
            Err(mpsc::TrySendError::Disconnected(_)) => Enqueue::Stopped,
        }
    }
}
/// May outlive the recorder without keeping its queue or worker alive.
#[derive(Clone)]
pub struct GcSink(Weak<Sink>);
impl GcSink {
    pub fn record(&self, category: GcCategory, text: &str) -> Enqueue {
        let Some(sink) = self.0.upgrade() else {
            return Enqueue::Stopped;
        };
        // Match main's 4096 Unicode scalar limit; no unbounded source clone.
        let text: String = text.chars().take(4096).collect();
        sink.send(Message::Gc {
            unix_ms: unix_ms(),
            category,
            text: text.into_boxed_str(),
        })
    }
}

pub struct Recorder {
    sender: Option<Arc<Sink>>,
    task: Option<JoinHandle<Result<(), Error>>>,
    dropped: Arc<AtomicU64>,
    started: Instant,
    identity: Identity,
}
impl Recorder {
    /// One recorder per process/directory, matching main's usage-<pid>.jsonl naming.
    pub fn start_directory(
        directory: PathBuf,
        identity: impl Into<Identity>,
    ) -> Result<Self, Error> {
        Self::start_directory_with_history(directory, identity, true)
    }

    pub fn start_directory_with_history(
        directory: PathBuf,
        identity: impl Into<Identity>,
        history_enabled: bool,
    ) -> Result<Self, Error> {
        std::fs::create_dir_all(&directory).map_err(storage::Error::Io)?;
        crate::retention::prune(&directory).map_err(storage::Error::Io)?;
        if !history_enabled {
            return Self::start(
                directory.join(format!("usage-{}.jsonl", std::process::id())),
                identity,
            );
        }
        // Keep sparse samples separately: a burst of GC diagnostics must not
        // erase the history needed to investigate a long viewing session.
        let history = directory.join("history");
        std::fs::create_dir_all(&history).map_err(storage::Error::Io)?;
        crate::retention::prune(&history).map_err(storage::Error::Io)?;
        let name = format!("usage-{}.jsonl", std::process::id());
        let mut detail = storage::RotatingWriter::new(directory.join(&name))?;
        let mut history = storage::RotatingWriter::new(history.join(&name))?;
        let mut last_sample = None;
        Self::spawn(identity, move |value| {
            detail.write(value)?;
            if history_due(value, last_sample) {
                history.write(value)?;
                last_sample = value["record"]["elapsed_ms"].as_u64();
            }
            Ok(())
        })
    }

    /// Opens the output at startup; all measurement and record writes run on the worker.
    pub fn start(path: PathBuf, identity: impl Into<Identity>) -> Result<Self, Error> {
        let mut writer = storage::RotatingWriter::new(path)?;
        Self::spawn(identity, move |value| writer.write(value))
    }
    fn spawn(
        identity: impl Into<Identity>,
        mut write: impl FnMut(&serde_json::Value) -> Result<(), storage::Error> + Send + 'static,
    ) -> Result<Self, Error> {
        let identity = identity.into();
        let (sender, receiver) = mpsc::sync_channel::<Message>(QUEUE_SIZE);
        let dropped = Arc::new(AtomicU64::new(0));
        let worker_dropped = dropped.clone();
        let task = thread::Builder::new()
            .name("viewer-diagnostics".into())
            .spawn(move || {
                for message in receiver {
                    let mut value = match message {
                        Message::Sample(entry) => serde_json::json!({
                        "record": entry,
                        "measured_unix_ms": unix_ms(),
                        "process": measurement::process_memory(),
                        "allocator": measurement::allocator_memory(),
                        "dropped_records": worker_dropped.load(Ordering::Relaxed),
                        }),
                        Message::Gc {
                            unix_ms,
                            category,
                            text,
                        } => serde_json::json!({
                            "kind": "qt_gc", "schema": 1, "pid": std::process::id(),
                            "unix_ms": unix_ms, "category": category, "message": text,
                            "dropped_records": worker_dropped.load(Ordering::Relaxed),
                        }),
                    };
                    // Include identity in every record, including sparse history
                    // and GC-only logs, so rotation cannot discard provenance.
                    if let Some(info) = identity.build_info() {
                        value["build_info"] =
                            serde_json::to_value(info).map_err(storage::Error::from)?;
                    }
                    // An IO error is terminal, so no further writes follow a partial line.
                    write(&value)?;
                }
                Ok(())
            })
            .map_err(Error::Spawn)?;
        Ok(Self {
            sender: Some(Arc::new(Sink {
                sender: Mutex::new(Some(sender)),
                dropped: dropped.clone(),
            })),
            task: Some(task),
            dropped,
            started: Instant::now(),
            identity,
        })
    }
    pub fn record(&self, event: Event, snapshot: Snapshot) -> Enqueue {
        let Some(sender) = &self.sender else {
            return Enqueue::Stopped;
        };
        let entry = Entry {
            schema: 1,
            pid: std::process::id(),
            version: self.identity.version(),
            unix_ms: unix_ms(),
            elapsed_ms: self.started.elapsed().as_millis(),
            event,
            snapshot,
        };
        sender.send(Message::Sample(entry))
    }
    pub fn gc_sink(&self) -> GcSink {
        GcSink(self.sender.as_ref().map(Arc::downgrade).unwrap_or_default())
    }
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
    pub fn is_finished(&self) -> bool {
        self.task.as_ref().is_none_or(JoinHandle::is_finished)
    }
    /// Stops admission immediately. The finite accepted queue drains on the worker.
    pub fn stop(mut self) -> Stopping {
        if let Some(sink) = self.sender.take() {
            sink.close();
        }
        Stopping {
            task: self.task.take(),
        }
    }
}

fn history_due(value: &serde_json::Value, last_sample: Option<u64>) -> bool {
    let Some(elapsed) = value["record"]["elapsed_ms"].as_u64() else {
        return false;
    };
    last_sample.is_none_or(|last| elapsed.saturating_sub(last) >= 60_000)
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
        if let Some(sink) = self.sender.take() {
            sink.close();
        }
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
    use crate::build_info::{BuildInfo, Source, WorktreeState};

    static BUILD: BuildInfo = BuildInfo {
        version: "test-build",
        source: Source::Git {
            commit: "0123456789abcdef0123456789abcdef01234567",
            worktree: WorktreeState::Dirty,
        },
        built_unix_seconds: 1_700_000_000,
        target: "test-target",
        profile: "release",
        rustc: "test-compiler",
        features: &["DISTRIBUTION"],
    };
    #[test]
    fn gc_only_directory_does_not_create_history() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let recorder =
            Recorder::start_directory_with_history(dir.path().to_owned(), &BUILD, false)?;
        recorder.gc_sink().record(GcCategory::Statistics, "fixture");
        recorder.stop().join()?;
        assert!(!dir.path().join("history").exists());
        assert_eq!(std::fs::read_dir(dir.path())?.count(), 1);
        let text = std::fs::read_to_string(
            dir.path()
                .join(format!("usage-{}.jsonl", std::process::id())),
        )?;
        let value: serde_json::Value = serde_json::from_str(text.trim())?;
        assert_eq!(value["build_info"], serde_json::to_value(&BUILD)?);
        Ok(())
    }
    #[test]
    fn sparse_history_ignores_gc_and_keeps_one_sample_per_minute() {
        let sample = |elapsed| serde_json::json!({"record": {"elapsed_ms": elapsed}});
        assert!(history_due(&sample(0), None));
        assert!(!history_due(&serde_json::json!({"kind": "qt_gc"}), None));
        assert!(!history_due(&sample(59_999), Some(0)));
        assert!(history_due(&sample(60_000), Some(0)));
        assert!(!history_due(&sample(60_001), Some(60_000)));
        assert!(!history_due(&sample(0), Some(60_000)));
    }

    #[test]
    fn directory_recording_preserves_history_apart_from_gc()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let recorder = Recorder::start_directory(dir.path().to_owned(), &BUILD)?;
        recorder.record(Event::Sample, Snapshot::default());
        recorder
            .gc_sink()
            .record(GcCategory::Statistics, "gc fixture");
        recorder.record(Event::Sample, Snapshot::default());
        recorder.stop().join()?;
        let name = format!("usage-{}.jsonl", std::process::id());
        assert_eq!(
            std::fs::read_to_string(dir.path().join(&name))?
                .lines()
                .count(),
            3
        );
        let history = std::fs::read_to_string(dir.path().join("history").join(name))?;
        assert_eq!(history.lines().count(), 1);
        let value: serde_json::Value = serde_json::from_str(history.trim())?;
        assert_eq!(value["build_info"], serde_json::to_value(&BUILD)?);
        assert_eq!(value["record"]["version"], BUILD.version);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(history.trim())?["record"]["event"],
            "sample"
        );
        Ok(())
    }
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
        assert_eq!(
            recorder.gc_sink().record(GcCategory::Statistics, "full"),
            Enqueue::Dropped
        );
        assert_eq!(recorder.dropped(), 1001);
        let stopping = recorder.stop();
        let release = release; // Also precede Stopping cleanup on assertion failure.
        assert!(!stopping.is_finished());
        release.send(())?;
        stopping.join()?;
        assert_eq!(writes.load(Ordering::Relaxed), QUEUE_SIZE as u64 + 1);
        Ok(())
    }
    #[test]
    fn competing_callback_drops_instead_of_waiting_for_sender_lock()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let recorder = Recorder::start(dir.path().join("usage.jsonl"), "test")?;
        let callback = recorder.gc_sink();
        let sink = callback.0.upgrade().ok_or("missing sink")?;
        let guard = sink.sender.lock().map_err(|_| "poisoned test lock")?;
        assert_eq!(
            callback.record(GcCategory::Statistics, "contention"),
            Enqueue::Dropped
        );
        assert_eq!(recorder.dropped(), 1);
        drop(guard);
        recorder.stop().join()?;
        Ok(())
    }

    #[test]
    fn gc_records_are_bounded_and_weak_callbacks_do_not_keep_the_worker_alive()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("usage.jsonl");
        let recorder = Recorder::start(path.clone(), "test")?;
        let callback = recorder.gc_sink();
        let late_callback = callback.clone();
        // Simulate a callback that upgraded Weak just before stop closes admission.
        let in_flight = callback.0.upgrade().ok_or("missing sink")?;
        assert!(GcCategory::parse("unrelated.category").is_none());
        assert!(matches!(
            GcCategory::parse("qt.qml.gc.statistics"),
            Some(GcCategory::Statistics)
        ));
        assert!(matches!(
            GcCategory::parse("qt.qml.gc.allocatorStats"),
            Some(GcCategory::AllocatorStats)
        ));
        let input = "𠮷\n\"".repeat(2000);
        assert_eq!(
            callback.record(GcCategory::Statistics, &input),
            Enqueue::Accepted
        );
        recorder.stop().join()?;
        assert_eq!(
            late_callback.record(GcCategory::AllocatorStats, "late"),
            Enqueue::Stopped
        );
        assert!(
            in_flight
                .sender
                .lock()
                .map_err(|_| "poisoned sink")?
                .is_none()
        );
        let text = std::fs::read_to_string(path)?;
        assert_eq!(text.lines().count(), 1);
        let record: serde_json::Value = serde_json::from_str(&text)?;
        assert_eq!(record["kind"], "qt_gc");
        assert_eq!(record["category"], "qt.qml.gc.statistics");
        assert_eq!(
            record["message"],
            input.chars().take(4096).collect::<String>()
        );
        assert!(record.get("process").is_none()); // GC callbacks don't trigger process sampling.
        Ok(())
    }

    #[test]
    fn production_size_rotation_keeps_complete_gc_records_and_final_snapshot()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("usage.jsonl");
        let mut writer = storage::RotatingWriter::new(path.clone())?;
        let (written_tx, written_rx) = mpsc::channel();
        let recorder = Recorder::spawn(&BUILD, move |value| {
            writer.write(value)?;
            written_tx.send(()).map_err(std::io::Error::other)?;
            Ok(())
        })?;
        let callback = recorder.gc_sink();
        // Over 8 MiB forces two rotations at the production 4 MiB limit.
        // Acknowledge disk writes to exercise rotation without scheduler-dependent
        // queue overflow. Queue saturation is covered separately above.
        for sequence in 0..2100 {
            let text = format!("{sequence:04}:{}", "x".repeat(4091));
            assert_eq!(
                callback.record(GcCategory::Statistics, &text),
                Enqueue::Accepted
            );
            written_rx.recv_timeout(std::time::Duration::from_secs(5))?;
        }
        assert_eq!(
            recorder.record(Event::StopRequested, Snapshot::default()),
            Enqueue::Accepted
        );
        assert_eq!(recorder.dropped(), 0);
        recorder.stop().join()?;
        assert_eq!(
            callback.record(GcCategory::Statistics, "late"),
            Enqueue::Stopped
        );

        let mut first_sequence = None;
        let mut next_sequence = None;
        let mut final_snapshot = false;
        for segment in [path.with_extension("previous.jsonl"), path] {
            let bytes = std::fs::read(segment)?;
            assert!(!bytes.is_empty());
            assert!(bytes.len() as u64 <= storage::MAX_FILE_BYTES);
            assert_eq!(bytes.last(), Some(&b'\n'));
            for line in std::str::from_utf8(&bytes)?.lines() {
                assert!(!final_snapshot, "snapshot must be the final record");
                let value: serde_json::Value = serde_json::from_str(line)?;
                assert_eq!(value["build_info"], serde_json::to_value(&BUILD)?);
                assert_eq!(value["dropped_records"], 0);
                if value["kind"] == "qt_gc" {
                    let message = value["message"].as_str().ok_or("missing GC message")?;
                    assert_eq!(message.len(), 4096);
                    let (sequence, _) = message.split_once(':').ok_or("missing sequence")?;
                    let sequence = sequence.parse::<usize>()?;
                    first_sequence.get_or_insert(sequence);
                    if let Some(expected) = next_sequence {
                        assert_eq!(sequence, expected);
                    }
                    next_sequence = Some(sequence + 1);
                } else {
                    assert_eq!(value["record"]["event"], "stop_requested");
                    final_snapshot = true;
                }
            }
        }
        assert!(first_sequence.is_some_and(|sequence| sequence > 0));
        assert_eq!(next_sequence, Some(2100));
        assert!(final_snapshot);
        assert_eq!(std::fs::read_dir(dir.path())?.count(), 2);
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
