//! Application lifetime adapter for bounded diagnostics.
use std::{
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock, Weak},
};
use viewer_diagnostics::{
    Snapshot,
    recorder::{Enqueue, Event, GcCategory, GcSink, Recorder},
};
static GC_SINK: OnceLock<GcSink> = OnceLock::new();
static APPLICATION: OnceLock<Weak<Mutex<Option<Recorder>>>> = OnceLock::new();

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("診断機能は既に初期化されています")]
    AlreadyInitialized,
    #[error("診断機能の所有者がありません")]
    NoOwner,
    #[error("診断機能のロックが破損しました")]
    Poisoned,
    #[error(transparent)]
    Recorder(#[from] viewer_diagnostics::recorder::Error),
}

/// Declare before the QML engine so all engine destruction notifications precede join.
/// Static access and Player clients are Weak; only this value owns the recorder.
pub struct Lifetime(Arc<Mutex<Option<Recorder>>>);
impl Lifetime {
    pub fn new() -> Result<Self, Error> {
        let owner = Self(Arc::new(Mutex::new(None)));
        APPLICATION
            .set(Arc::downgrade(&owner.0))
            .map_err(|_| Error::AlreadyInitialized)?;
        Ok(owner)
    }
}
impl Drop for Lifetime {
    fn drop(&mut self) {
        // Recover only to take ownership for cleanup; no application data is reused.
        let recorder = self
            .0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take();
        if let Some(recorder) = recorder
            && let Err(error) = recorder.stop().join()
        {
            tracing::error!("{error}");
        }
    }
}

pub struct Client(Weak<Mutex<Option<Recorder>>>, bool);
impl Client {
    pub fn record(&self, event: Event, snapshot: Snapshot) -> Result<Enqueue, Error> {
        if !self.1 {
            return Ok(Enqueue::Stopped);
        }
        let owner = self.0.upgrade().ok_or(Error::NoOwner)?;
        let guard = owner.lock().map_err(|_| Error::Poisoned)?;
        Ok(guard.as_ref().map_or(Enqueue::Stopped, |recorder| {
            recorder.record(event, snapshot)
        }))
    }
    pub fn is_finished(&self) -> Result<bool, Error> {
        let owner = self.0.upgrade().ok_or(Error::NoOwner)?;
        let guard = owner.lock().map_err(|_| Error::Poisoned)?;
        Ok(guard.as_ref().is_none_or(Recorder::is_finished))
    }
    /// Consume a failed worker once, while the UI can still display its error.
    pub fn finish(self) -> Result<(), Error> {
        let owner = self.0.upgrade().ok_or(Error::NoOwner)?;
        let recorder = owner.lock().map_err(|_| Error::Poisoned)?.take();
        if let Some(recorder) = recorder {
            recorder.stop().join()?;
        }
        Ok(())
    }
}

/// Shared by Qt hook installation and recorder startup so disabled means neither.
pub fn requested(isolated: bool) -> bool {
    let setting = std::env::var("NAGAMETV_DIAGNOSTICS");
    enabled(setting.as_deref().ok(), isolated)
        || std::env::var("NAGAMETV_GC_LOG").as_deref() == Ok("1")
}

pub fn start(directory: PathBuf, isolated: bool) -> Result<Option<Client>, Error> {
    if !requested(isolated) {
        return Ok(None);
    }
    let owner = APPLICATION
        .get()
        .and_then(Weak::upgrade)
        .ok_or(Error::NoOwner)?;
    let mut guard = owner.lock().map_err(|_| Error::Poisoned)?;
    if guard.is_some() {
        return Err(Error::AlreadyInitialized);
    }
    let samples = enabled(
        std::env::var("NAGAMETV_DIAGNOSTICS").as_deref().ok(),
        isolated,
    );
    let recorder = Recorder::start_directory_with_history(
        directory.join("usage"),
        env!("CARGO_PKG_VERSION"),
        samples,
    )?;
    GC_SINK
        .set(recorder.gc_sink())
        .map_err(|_| Error::AlreadyInitialized)?;
    *guard = Some(recorder);
    Ok(Some(Client(Arc::downgrade(&owner), samples)))
}
pub fn record_qt_gc(category: &str, message: &str) {
    if let (Some(sink), Some(category)) = (GC_SINK.get(), GcCategory::parse(category)) {
        let _ = sink.record(category, message);
    }
}

fn enabled(setting: Option<&str>, isolated: bool) -> bool {
    setting != Some("0") && (!isolated || setting == Some("1"))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gc_only_client_does_not_enqueue_measurements() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("gc.jsonl");
        let recorder = Recorder::start(path.clone(), "gc-only")?;
        let gc = recorder.gc_sink();
        let owner = Lifetime(Arc::new(Mutex::new(Some(recorder))));
        let client = Client(Arc::downgrade(&owner.0), false);
        assert_eq!(
            client.record(Event::Sample, Snapshot::default())?,
            Enqueue::Stopped
        );
        assert_eq!(
            gc.record(GcCategory::Statistics, "fixture"),
            Enqueue::Accepted
        );
        drop(owner);
        let text = std::fs::read_to_string(path)?;
        assert_eq!(text.lines().count(), 1);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(text.trim())?["kind"],
            "qt_gc"
        );
        Ok(())
    }
    #[test]
    fn dropping_player_client_keeps_gc_alive_until_application_owner_exits()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("usage.jsonl");
        let recorder = Recorder::start(path.clone(), "lifetime-test")?;
        let callback = recorder.gc_sink();
        // Private owner construction avoids process-global registration in parallel tests.
        let owner = Lifetime(Arc::new(Mutex::new(Some(recorder))));
        let client = Client(Arc::downgrade(&owner.0), true);
        assert_eq!(
            client.record(Event::Sample, Snapshot::default())?,
            Enqueue::Accepted
        );
        drop(client);
        assert_eq!(
            callback.record(GcCategory::Statistics, "engine destruction"),
            Enqueue::Accepted
        );
        let late_client = Client(Arc::downgrade(&owner.0), true);
        drop(owner);
        assert_eq!(
            callback.record(GcCategory::Statistics, "late"),
            Enqueue::Stopped
        );
        assert!(matches!(late_client.is_finished(), Err(Error::NoOwner)));
        let records = std::fs::read_to_string(path)?;
        let records: Vec<serde_json::Value> = records
            .lines()
            .map(serde_json::from_str)
            .collect::<Result<_, _>>()?;
        assert_eq!(records.len(), 2);
        assert_eq!(records[0]["record"]["event"], "sample");
        assert_eq!(records[1]["message"], "engine destruction");
        Ok(())
    }

    #[test]
    fn explicit_disabled_and_isolated_launches_do_not_enable_recording_by_default() {
        assert!(enabled(None, false));
        assert!(!enabled(None, true));
        for isolated in [true, false] {
            assert!(!enabled(Some("0"), isolated));
            assert!(enabled(Some("1"), isolated));
        }
        assert!(!enabled(Some("invalid"), true));
    }
}
