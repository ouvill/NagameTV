//! One blocking inspection at a time. A replacement waits for its cancelled
//! predecessor; only the newest request can publish. No Qt objects cross threads.
use super::{Error, Recording, source::Location};
use crate::services::Progress;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    Open,
    Replay,
}

pub struct Request {
    location: Location,
    purpose: Purpose,
    broadcast: Option<super::Broadcast>,
    metadata: Option<crate::epgstation::PlaybackMetadata>,
    pub(super) external_subtitle: Option<crate::media_subtitles::Script>,
}
impl Request {
    pub fn open(path: PathBuf) -> Self {
        Self {
            location: Location::Local(path),
            purpose: Purpose::Open,
            broadcast: None,
            external_subtitle: None,
            metadata: None,
        }
    }
    pub fn from_url(url: &str) -> Result<Self, Error> {
        Ok(Self {
            location: Location::from_url(url)?,
            purpose: Purpose::Open,
            broadcast: None,
            external_subtitle: None,
            metadata: None,
        })
    }
    pub(crate) fn epgstation(
        url: &str,
        broadcast: Option<super::Broadcast>,
        metadata: Option<crate::epgstation::PlaybackMetadata>,
    ) -> Result<Self, Error> {
        let mut request = Self::from_url(url)?;
        request.broadcast = broadcast;
        request.metadata = metadata;
        Ok(request)
    }
    /// Resolve before acknowledging the drop: KDE retires its transfer when
    /// the drag ends. Only the received path can enter the asynchronous loader.
    pub fn portal_transfer(key: &str) -> Result<Self, Error> {
        retrieve_transfer(key).map(Self::open)
    }
    pub(super) fn replay(location: Location, broadcast: Option<super::Broadcast>) -> Self {
        Self {
            location,
            purpose: Purpose::Replay,
            broadcast,
            external_subtitle: None,
            metadata: None,
        }
    }
}

// FileTransfer belongs to the Linux document portal, not the FileChooser
// interface. The resulting sandbox path stays with the inspected Recording.
fn retrieve_transfer(key: &str) -> Result<PathBuf, Error> {
    // GTK serializes its transfer key with a trailing NUL. D-Bus strings cannot
    // contain NULs; reject embedded NULs instead of silently truncating a key.
    let key = key.strip_suffix('\0').unwrap_or(key);
    if key.is_empty() || key.contains('\0') {
        return Err(Error::Portal("Invalid file transfer key".into()));
    }
    #[cfg(target_os = "linux")]
    {
        crate::qt::ffi::portal_retrieve_recording(&cxx_qt_lib::QString::from(key))
            .map(|path| PathBuf::from(path.to_string()))
            .map_err(|error| Error::Portal(error.to_string()))
    }
    #[cfg(not(target_os = "linux"))]
    Err(Error::Portal("File transfer portals require Linux".into()))
}
type Outcome = (Purpose, Result<Recording, Error>);

#[derive(Default)]
pub struct Loader {
    state: State,
}
#[derive(Default)]
enum State {
    #[default]
    Idle,
    Working {
        job: Job,
        purpose: Purpose,
    },
    Retiring {
        job: Stopping,
        next: Option<Request>,
    },
    Ready(Outcome),
}
impl Loader {
    pub fn loading(&self) -> bool {
        match &self.state {
            State::Working { .. } | State::Ready(_) => true,
            State::Retiring { next, .. } => next.is_some(),
            State::Idle => false,
        }
    }
    pub fn begin(&mut self, request: Request) {
        self.state = match std::mem::take(&mut self.state) {
            State::Working { job, .. } => State::Retiring {
                job: job.cancel(),
                next: Some(request),
            },
            State::Retiring { job, .. } => State::Retiring {
                job,
                next: Some(request),
            },
            State::Idle | State::Ready(_) => Self::start(request),
        };
    }
    fn start(request: Request) -> State {
        let purpose = request.purpose;
        match Job::spawn(move |cancelled| {
            let mut recording = Recording::inspect(&request.location, cancelled)?;
            if let Recording::Media(file) = &mut recording {
                file.broadcast = request.broadcast.map(|broadcast| {
                    match request
                        .metadata
                        .and_then(|metadata| metadata.start_ms(cancelled))
                    {
                        Some(start) => broadcast.with_start(start),
                        None => broadcast,
                    }
                });
                file.external_subtitle = request.external_subtitle;
            }
            if cancelled.load(Ordering::Acquire) {
                return Err(Error::Cancelled);
            }
            Ok(recording)
        }) {
            Ok(job) => State::Working { job, purpose },
            Err(error) => State::Ready((purpose, Err(error.into()))),
        }
    }
    pub fn cancel(&mut self) {
        self.state = match std::mem::take(&mut self.state) {
            State::Working { job, .. } => State::Retiring {
                job: job.cancel(),
                next: None,
            },
            State::Retiring { job, .. } => State::Retiring { job, next: None },
            State::Idle | State::Ready(_) => State::Idle,
        };
    }
    pub fn poll(&mut self) -> Option<Outcome> {
        let mut outcome = None;
        self.state = match std::mem::take(&mut self.state) {
            State::Idle => State::Idle,
            State::Ready(result) => {
                outcome = Some(result);
                State::Idle
            }
            State::Working { job, purpose } => match job.poll() {
                Progress::Pending(job) => State::Working { job, purpose },
                Progress::Complete(result) => {
                    outcome = Some((purpose, result));
                    State::Idle
                }
            },
            State::Retiring { job, next } => match job.poll() {
                Progress::Pending(job) => State::Retiring { job, next },
                Progress::Complete(()) => next.map(Self::start).unwrap_or_default(),
            },
        };
        outcome
    }
}

// Dropping the owner cancels further reads without joining on the UI thread.
// A syscall already in progress may finish later; its result has no consumer.
struct Cancellation(Arc<AtomicBool>);
impl Drop for Cancellation {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}
struct Job {
    worker: thread::JoinHandle<Result<Recording, Error>>,
    cancellation: Cancellation,
}
impl Job {
    fn spawn(
        inspect: impl FnOnce(&Arc<AtomicBool>) -> Result<Recording, Error> + Send + 'static,
    ) -> std::io::Result<Self> {
        let cancellation = Cancellation(Arc::new(AtomicBool::new(false)));
        let flag = cancellation.0.clone();
        let worker = thread::Builder::new()
            .name("media-inspection".into())
            .spawn(move || inspect(&flag))?;
        Ok(Self {
            worker,
            cancellation,
        })
    }
    fn poll(self) -> Progress<Self, Result<Recording, Error>> {
        if !self.worker.is_finished() {
            return Progress::Pending(self);
        }
        Progress::Complete(self.worker.join().unwrap_or(Err(Error::WorkerStopped)))
    }
    fn cancel(self) -> Stopping {
        self.cancellation.0.store(true, Ordering::Relaxed);
        Stopping(self)
    }
}
// Cancellation consumes the result API, retaining only completion ownership.
struct Stopping(Job);
impl Stopping {
    fn poll(self) -> Progress<Self, ()> {
        match self.0.poll() {
            Progress::Pending(job) => Progress::Pending(Self(job)),
            Progress::Complete(_) => Progress::Complete(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::mpsc,
        time::{Duration, Instant},
    };

    fn fixture(path: &std::path::Path) {
        std::fs::write(
            path,
            include_bytes!("../../../../tests/fixtures/subtitle-clock.ts"),
        )
        .unwrap();
    }
    fn finish(loader: &mut Loader) -> Outcome {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(outcome) = loader.poll() {
                return outcome;
            }
            assert!(Instant::now() < deadline, "inspection did not finish");
            thread::sleep(Duration::from_millis(1));
        }
    }
    #[test]
    fn replacement_waits_for_worker_and_only_latest_request_can_publish() {
        let directory = tempfile::tempdir().unwrap();
        let old = directory.path().join("old.ts");
        let newest = directory.path().join("new.ts");
        fixture(&old);
        fixture(&newest);
        let (release, wait) = mpsc::channel();
        let job = Job::spawn(move |cancelled| {
            wait.recv_timeout(Duration::from_secs(5)).unwrap();
            assert!(cancelled.load(Ordering::Relaxed));
            // An OS read can succeed after cancellation was requested.
            Recording::open(&old)
        })
        .unwrap();
        let mut loader = Loader {
            state: State::Working {
                job,
                purpose: Purpose::Open,
            },
        };
        loader.begin(Request::open(directory.path().join("superseded.ts")));
        loader.begin(Request::open(newest));
        assert!(loader.loading());
        assert!(loader.poll().is_none());
        assert!(
            matches!(loader.state, State::Retiring { .. }),
            "never overlap filesystem workers"
        );
        release.send(()).unwrap();
        let (purpose, file) = finish(&mut loader);
        assert_eq!(purpose, Purpose::Open);
        assert_eq!(file.unwrap().name(), "new.ts");
        assert!(!loader.loading());
        assert!(loader.poll().is_none());
    }
    #[test]
    fn cancelling_discards_late_success_and_queued_replacement() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("recording.ts");
        fixture(&path);
        let file = Recording::open(&path).unwrap();
        let (release, wait) = mpsc::channel();
        let job = Job::spawn(move |_| {
            wait.recv_timeout(Duration::from_secs(5)).unwrap();
            Ok(file)
        })
        .unwrap();
        let mut loader = Loader {
            state: State::Working {
                job,
                purpose: Purpose::Open,
            },
        };
        loader.begin(Request::open(path));
        loader.cancel();
        assert!(!loader.loading());
        assert!(loader.poll().is_none());
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !matches!(loader.state, State::Idle) {
            assert!(
                loader.poll().is_none(),
                "cancelled input must never publish"
            );
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        }
    }
    #[test]
    fn replay_revalidates_and_reports_deleted_file_asynchronously() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("recording.ts");
        fixture(&path);
        let file = Recording::open(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        let mut loader = Loader::default();
        loader.begin(file.replay());
        let (purpose, result) = finish(&mut loader);
        assert_eq!(purpose, Purpose::Replay);
        assert!(matches!(result, Err(Error::Read(_))));
    }
    #[test]
    fn dropping_owner_cancels_without_joining_blocked_io() {
        let (release, wait) = mpsc::channel();
        let (finished, observed) = mpsc::channel();
        let job = Job::spawn(move |cancelled| {
            wait.recv_timeout(Duration::from_secs(5)).unwrap();
            finished.send(cancelled.load(Ordering::Relaxed)).unwrap();
            Err(Error::Cancelled)
        })
        .unwrap();
        let loader = Loader {
            state: State::Working {
                job,
                purpose: Purpose::Open,
            },
        };
        drop(loader); // Must return before the worker is released.
        release.send(()).unwrap();
        assert!(observed.recv_timeout(Duration::from_secs(5)).unwrap());
    }
}
