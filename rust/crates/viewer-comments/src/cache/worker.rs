use super::{
    download::{Job, Outcome},
    store::Store,
    *,
};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const PENDING_BYTES: usize = 8 * 1024 * 1024;
const POLL: Duration = Duration::from_millis(100);
const LOCAL_RETRY: Duration = Duration::from_secs(5);
const SEEK_SETTLE: Duration = Duration::from_millis(500);
const MAINTENANCE: Duration = Duration::from_secs(60);
const ENDPOINT: &str = "https://jikkyo.tsukumijima.net/api/kakolog";
#[cfg(test)]
mod integration_tests;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    Starting,
    Ready,
    Loading,
    Waiting(i64),
    FetchFailed(String),
    StorageFailed(String),
}
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub source: Option<u64>,
    pub view: Option<View>,
    pub records: Arc<Vec<Record>>,
    pub revision: u64,
    pub state: State,
    pub disk_bytes: u64,
}
impl Default for Snapshot {
    fn default() -> Self {
        Self {
            source: None,
            view: None,
            records: Arc::default(),
            revision: 0,
            state: State::Starting,
            disk_bytes: 0,
        }
    }
}
struct Batch {
    source: u64,
    channel: u16,
    clock: Option<ClockSpan>,
    comments: Vec<(Comment, bool)>,
    bytes: usize,
}
#[derive(Default)]
struct Input {
    demand: Option<Demand>,
    batches: VecDeque<Batch>,
    bytes: usize,
    clear: bool,
    stop: bool,
}
struct Shared {
    input: Mutex<Input>,
    wake: Condvar,
    output: Mutex<Snapshot>,
}
pub struct Controller {
    shared: Arc<Shared>,
    task: Option<thread::JoinHandle<()>>,
}
impl Controller {
    pub fn start(directory: PathBuf) -> Result<Self, Error> {
        Self::with_endpoint(directory, ENDPOINT.into())
    }
    fn with_endpoint(directory: PathBuf, endpoint: String) -> Result<Self, Error> {
        let shared = Arc::new(Shared {
            input: Mutex::default(),
            wake: Condvar::new(),
            output: Mutex::new(Snapshot::default()),
        });
        let state = shared.clone();
        let task = thread::Builder::new()
            .name("comment-store".into())
            .spawn(move || run(directory, endpoint, state))?;
        Ok(Self {
            shared,
            task: Some(task),
        })
    }
    pub fn configure(&mut self, demand: Option<Demand>) {
        if let Ok(mut input) = self.shared.input.lock() {
            input.demand = demand;
            self.shared.wake.notify_one();
        }
    }
    /// A rejected batch remains owned by the caller. The reception controller
    /// must stop draining new comments until it can submit that same batch.
    pub fn receive(
        &mut self,
        source: u64,
        channel: u16,
        clock: Option<ClockSpan>,
        comments: Vec<(Comment, bool)>,
    ) -> Result<(), Vec<(Comment, bool)>> {
        if comments.is_empty() {
            return Ok(());
        }
        let bytes = comments
            .iter()
            .map(|(c, _)| {
                c.text.len()
                    + c.identity.as_ref().map_or(0, |i| i.user_id.len())
                    + std::mem::size_of::<Comment>()
            })
            .sum::<usize>();
        let Ok(mut input) = self.shared.input.lock() else {
            return Err(comments);
        };
        if input.bytes.saturating_add(bytes) > PENDING_BYTES {
            return Err(comments);
        }
        input.bytes += bytes;
        input.batches.push_back(Batch {
            source,
            channel,
            clock,
            comments,
            bytes,
        });
        self.shared.wake.notify_one();
        Ok(())
    }
    pub fn has_capacity(&self) -> bool {
        self.shared
            .input
            .lock()
            .is_ok_and(|input| input.bytes < PENDING_BYTES / 2)
    }
    pub fn snapshot(&self) -> Snapshot {
        self.shared
            .output
            .lock()
            .map(|output| output.clone())
            .unwrap_or_else(|_| Snapshot {
                state: State::StorageFailed("comment worker stopped".into()),
                ..Default::default()
            })
    }
    pub fn clear_unused(&mut self) {
        if let Ok(mut input) = self.shared.input.lock() {
            input.clear = true;
            self.shared.wake.notify_one();
        }
    }
    pub fn shutdown(&mut self) {
        if let Ok(mut input) = self.shared.input.lock() {
            input.stop = true;
            self.shared.wake.notify_one();
        }
        if let Some(task) = self.task.take() {
            let _ = task.join();
        }
    }
}
impl Drop for Controller {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn set_state(shared: &Shared, state: State) {
    if let Ok(mut output) = shared.output.lock() {
        output.state = state;
    }
}
fn wait(shared: &Shared, duration: Duration) -> bool {
    let Ok(input) = shared.input.lock() else {
        return true;
    };
    if input.stop {
        return true;
    }
    shared
        .wake
        .wait_timeout(input, duration)
        .map_or(true, |(input, _)| input.stop)
}
fn wall() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .unwrap_or(0)
}

fn run(directory: PathBuf, endpoint: String, shared: Arc<Shared>) {
    let (mut store, session) = loop {
        let result = (|| {
            let mut store = Store::open(&directory)?;
            store.reap_sessions()?;
            let session = store.session()?;
            Ok::<_, Error>((store, session))
        })();
        match result {
            Ok(value) => break value,
            Err(error) => {
                set_state(&shared, State::StorageFailed(error.to_string()));
                if wait(&shared, LOCAL_RETRY) {
                    return;
                }
            }
        }
    };
    let owner = &session.name;
    let epoch = Instant::now();
    let initial_wall = wall();
    let mut source = None;
    let mut job: Option<Job> = None;
    let mut demand: Option<Demand> = None;
    let mut pending: Option<Batch> = None;
    let mut next_fetch = Instant::now() + SEEK_SETTLE;
    let mut maintenance = Instant::now();
    let mut observed: Option<(u64, String, u16, u64, i64)> = None;
    let mut read_signature = None;
    let mut live_revision = 0_u64;
    let mut storage_retry = None;
    let mut clear_pending = false;
    loop {
        let now = initial_wall.saturating_add(epoch.elapsed().as_secs() as i64);
        let (latest, clear, stopping, backlog) = match shared.input.lock() {
            Ok(mut input) => {
                if pending.is_none() {
                    pending = input.batches.pop_front();
                }
                (
                    input.demand.clone(),
                    std::mem::take(&mut input.clear),
                    input.stop,
                    !input.batches.is_empty(),
                )
            }
            Err(_) => break,
        };
        if stopping {
            break;
        }
        clear_pending |= clear;
        if storage_retry.is_some_and(|deadline| Instant::now() < deadline) {
            if wait(&shared, POLL) {
                break;
            }
            continue;
        }
        storage_retry = None;
        let process = (|| -> Result<(), Error> {
            let next_source = latest.as_ref().map(|d| d.source);
            if source != next_source {
                store.reset_source(owner)?;
                source = next_source;
                observed = None;
                read_signature = None;
                next_fetch = Instant::now() + SEEK_SETTLE;
            }
            if latest != demand {
                if let Some(latest) = &latest {
                    match &latest.source_range {
                        Source::Pending => {}
                        Source::Recording(range) => store.pin(owner, range.spans())?,
                        Source::Live {
                            spans,
                            earliest_media_ms,
                            ..
                        } => {
                            store.pin(owner, spans)?;
                            store.retain_live(owner, *earliest_media_ms, spans)?;
                        }
                    }
                }
                // Changing fetch permission or jumping to another position must
                // replace the deferred demand, without cancelling a sent GET.
                let jump = latest.as_ref().zip(demand.as_ref()).is_some_and(|(a, b)| {
                    a.fetch != b.fetch
                        || a.view.as_ref().zip(b.view.as_ref()).is_some_and(|(a, b)| {
                            a.clock_key != b.clock_key
                                || a.interval.start.abs_diff(b.interval.start) > 2
                        })
                });
                if jump {
                    next_fetch = Instant::now() + SEEK_SETTLE;
                }
                demand = latest.clone();
            }
            if let Some(batch) = pending.as_ref() {
                if source == Some(batch.source) {
                    store.insert_live(
                        owner,
                        batch.channel,
                        batch.clock.as_ref(),
                        &batch.comments,
                    )?;
                    live_revision = live_revision.wrapping_add(1);
                }
                let bytes = batch.bytes;
                pending = None;
                if let Ok(mut input) = shared.input.lock() {
                    input.bytes = input.bytes.saturating_sub(bytes);
                }
            }
            // Only advertise reception through a tip after all batches queued
            // before that demand have reached disk. A backlog is a gap, not
            // evidence of an empty, successfully received interval.
            if let Some(d) = demand.as_ref().filter(|_| !backlog) {
                if let Source::Live {
                    spans,
                    reception: Reception::Receiving(epoch),
                    ..
                } = &d.source_range
                {
                    if let Some(tip) = spans.iter().max_by_key(|s| s.media_end_ms) {
                        if let Some(range) = tip.interval() {
                            let at = range.end;
                            if let Some((old_source, ref key, channel, old_epoch, last)) = observed
                            {
                                if old_source == d.source
                                    && *key == tip.key
                                    && channel == tip.channel
                                    && old_epoch == *epoch
                                    && at > last
                                {
                                    if let Some(range) = Interval::new(last, at) {
                                        store.observe_live(owner, channel, key, range)?;
                                    }
                                }
                            }
                            observed = Some((d.source, tip.key.clone(), tip.channel, *epoch, at));
                        }
                    }
                } else {
                    observed = None;
                }
            } else {
                observed = None;
            }
            if job.as_ref().is_some_and(Job::finished) {
                let outcome = job.take().expect("finished job").finish()?;
                match outcome {
                    Outcome::Complete => {
                        read_signature = None;
                        next_fetch = Instant::now();
                        set_state(&shared, State::Ready);
                    }
                    Outcome::Idle => {
                        next_fetch = Instant::now() + Duration::from_secs(1);
                        set_state(&shared, State::Ready);
                    }
                    Outcome::Waiting(until) => {
                        next_fetch = Instant::now()
                            + Duration::from_secs(until.saturating_sub(now).max(1) as u64);
                        set_state(&shared, State::Waiting(until));
                    }
                    Outcome::RemoteFailed(message) => {
                        next_fetch =
                            Instant::now() + Duration::from_secs(REQUEST_SPACING_SECONDS as u64);
                        set_state(&shared, State::FetchFailed(message));
                    }
                }
            }
            if clear_pending || Instant::now() >= maintenance {
                store.cleanup(now, clear_pending)?;
                maintenance = Instant::now() + MAINTENANCE;
                if let Ok(mut output) = shared.output.lock() {
                    output.disk_bytes = store.disk_bytes();
                }
                if clear_pending {
                    read_signature = None;
                }
                clear_pending = false;
            }
            if let Some(d) = &demand {
                let revision = store.revision()?;
                let signature = (d.source, d.channel, d.view.clone(), revision, live_revision);
                if read_signature.as_ref() != Some(&signature) {
                    let records = match &d.view {
                        Some(view) => store.read(owner, d.channel, view, now)?,
                        None => Vec::new(),
                    };
                    if let Ok(mut output) = shared.output.lock() {
                        output.source = Some(d.source);
                        output.view = d.view.clone();
                        output.records = Arc::new(records);
                        output.revision = output.revision.wrapping_add(1);
                        output.disk_bytes = store.disk_bytes();
                    }
                    read_signature = Some(signature);
                }
                if job.is_none() && Instant::now() >= next_fetch {
                    // Also checks for a completed local response after restart.
                    // `download::next` forbids HTTP while fetch is disabled.
                    job = Some(Job::start(
                        directory.clone(),
                        owner.clone(),
                        d.clone(),
                        endpoint.clone(),
                        now,
                    )?);
                    set_state(
                        &shared,
                        if d.fetch {
                            State::Loading
                        } else {
                            State::Ready
                        },
                    );
                }
            } else if let Ok(mut output) = shared.output.lock() {
                if output.source.take().is_some() {
                    output.records = Arc::default();
                    output.view = None;
                    output.revision = output.revision.wrapping_add(1);
                }
            }
            Ok(())
        })();
        if let Err(error) = process {
            observed = None;
            set_state(&shared, State::StorageFailed(error.to_string()));
            storage_retry = Some(Instant::now() + LOCAL_RETRY);
            // Local recovery comes before another GET. The complete spool is
            // retained by the download/import typestates on every error path.
            next_fetch = Instant::now() + LOCAL_RETRY;
        }
        if wait(&shared, POLL) {
            break;
        }
    }
    if let Some(job) = job {
        job.stop();
    }
    let _ = store.release_session(owner);
    let _ = store.cleanup(wall(), false);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_disabled_demand_does_not_contact_the_endpoint() {
        let directory = tempfile::tempdir().unwrap();
        let mut controller =
            Controller::with_endpoint(directory.path().into(), "http://127.0.0.1:1".into())
                .unwrap();
        controller.configure(Some(Demand {
            source: 1,
            channel: 1,
            view: None,
            source_range: Source::Recording(RecordingRange::Discovering(vec![])),
            fetch: false,
        }));
        let until = Instant::now() + Duration::from_secs(3);
        while controller.snapshot().source != Some(1) && Instant::now() < until {
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(controller.snapshot().source, Some(1));
        controller.shutdown();
    }
}
