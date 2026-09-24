use super::{
    download::{Acquisition, Job, Outcome, Progress},
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
    Importing,
    Waiting(i64),
    FetchFailed {
        message: String,
        retry_at: Option<i64>,
    },
    StorageFailed(String),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Availability {
    Unrequested,
    Provisional,
    Stored,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub source: Option<u64>,
    pub view: Option<View>,
    pub records: Arc<Vec<Record>>,
    pub revision: u64,
    pub state: State,
    pub disk_bytes: u64,
    pub availability: Availability,
    target: Option<plan::Target>,
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
            availability: Availability::Unrequested,
            target: None,
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
    refresh: Option<plan::Target>,
    budget: Option<i64>,
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
    pub fn refresh_current(&mut self) {
        let target = self
            .shared
            .output
            .lock()
            .ok()
            .and_then(|output| output.target.clone());
        if let Ok(mut input) = self.shared.input.lock() {
            input.refresh = target;
            self.shared.wake.notify_one();
        }
    }
    pub fn set_budget(&mut self, bytes: i64) {
        if let Ok(mut input) = self.shared.input.lock() {
            input.budget = Some(bytes);
            self.shared.wake.notify_one();
        }
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
        if let Some(task) = self.task.take()
            && task.join().is_err()
        {
            tracing::error!("Comment cache worker panicked during shutdown");
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
                tracing::error!(
                    error = &error as &dyn std::error::Error,
                    "Comment cache open failed"
                );
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
    let mut next_fetch = Some(Instant::now() + SEEK_SETTLE);
    let mut planner = plan::Planner::default();
    let mut pinned_target = None;
    let mut fetch_key = None;
    let mut fetched_revision = None;
    let mut blocked_target = None;
    let mut maintenance = Instant::now();
    let mut observed: Option<(u64, String, u16, u64, i64)> = None;
    let mut read_signature = None;
    let mut live_revision = 0_u64;
    let mut storage_retry = None;
    let mut clear_pending = false;
    let mut refresh_pending = None;
    let mut budget_pending = None;
    loop {
        let now = initial_wall.saturating_add(epoch.elapsed().as_secs() as i64);
        let (latest, clear, refresh, budget, stopping, backlog) = match shared.input.lock() {
            Ok(mut input) => {
                if pending.is_none() {
                    pending = input.batches.pop_front();
                }
                (
                    input.demand.clone(),
                    std::mem::take(&mut input.clear),
                    input.refresh.take(),
                    input.budget.take(),
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
        if refresh.is_some() {
            refresh_pending = refresh;
        }
        if budget.is_some() {
            budget_pending = budget;
        }
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
                set_state(&shared, State::Ready);
                observed = None;
                pinned_target = None;
                read_signature = None;
                next_fetch = Some(Instant::now() + SEEK_SETTLE);
            }
            if latest != demand {
                // Moving the view does not change retention. Rewriting the
                // same pins would wait for an archive import's write lock
                // before a cached seek could use SQLite's concurrent reader.
                if let Some(latest) = &latest
                    && demand.as_ref().is_none_or(|old| {
                        old.source != latest.source || old.source_range != latest.source_range
                    })
                {
                    match &latest.source_range {
                        Source::Pending => {}
                        Source::Recording(_) => {}
                        Source::Live {
                            spans,
                            earliest_media_ms,
                            ..
                        } => {
                            store.pin(owner, spans, now)?;
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
                    next_fetch = Some(Instant::now() + SEEK_SETTLE);
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
                    if let Some(tip) = spans.iter().max_by_key(|s| s.media_end_ms)
                        && let Some(range) = tip.interval()
                    {
                        let at = range.end;
                        if let Some((old_source, ref key, channel, old_epoch, last)) = observed
                            && old_source == d.source
                            && *key == tip.key
                            && channel == tip.channel
                            && old_epoch == *epoch
                            && at > last
                            && let Some(range) = Interval::new(last, at)
                        {
                            store.observe_live(owner, channel, key, range)?;
                        }
                        observed = Some((d.source, tip.key.clone(), tip.channel, *epoch, at));
                    }
                } else {
                    observed = None;
                }
            } else {
                observed = None;
            }
            if job.as_ref().is_some_and(Job::finished) {
                let finished = job.take().expect("finished job");
                let finished_target = finished.target.clone();
                let outcome = finished.finish()?;
                match outcome {
                    Outcome::Complete => {
                        read_signature = None;
                        next_fetch = Some(Instant::now());
                        set_state(&shared, State::Ready);
                    }
                    Outcome::Idle => {
                        next_fetch = (finished_target != pinned_target).then(Instant::now);
                        set_state(&shared, State::Ready);
                    }
                    Outcome::Waiting(until) => {
                        next_fetch = Some(
                            Instant::now()
                                + Duration::from_secs(until.saturating_sub(now).max(1) as u64),
                        );
                        set_state(&shared, State::Waiting(until));
                    }
                    Outcome::RemoteFailed {
                        target: failed_target,
                        message,
                        retry_at,
                    } => {
                        tracing::warn!(error = %message, retry_at, "Comment archive fetch failed");
                        if retry_at.is_none() {
                            blocked_target = failed_target.as_ref().map(plan::Target::key);
                        }
                        if failed_target != pinned_target {
                            next_fetch = Some(Instant::now());
                            set_state(&shared, State::Ready);
                        } else {
                            next_fetch = retry_at.map(|until| {
                                Instant::now()
                                    + Duration::from_secs(until.saturating_sub(now).max(1) as u64)
                            });
                            set_state(&shared, State::FetchFailed { message, retry_at });
                        }
                    }
                }
            }
            if let Some(bytes) = budget_pending.take() {
                store.set_budget(bytes);
                maintenance = Instant::now();
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
                        Some(view) => store.read(owner, d.channel, view)?,
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
                let target = planner.update(d, now);
                if target != pinned_target {
                    if matches!(d.source_range, Source::Recording(_))
                        && let Some(target) = &target
                    {
                        store.pin_target(owner, target, now)?;
                    }
                    pinned_target = target.clone();
                }
                if let Some(refresh) = &refresh_pending {
                    store.request_refresh(refresh, now)?;
                    refresh_pending = None;
                    blocked_target = None;
                    next_fetch = Some(Instant::now());
                }
                let acquisition = target
                    .clone()
                    .filter(|_| d.fetch)
                    .map(|target| Acquisition {
                        target,
                        focus: d
                            .view
                            .as_ref()
                            .map(|view| view.interval.start + LOOKBACK_SECONDS),
                    });
                // Read coverage without holding the publication mutex: the
                // GUI's snapshot must never wait on SQLite or filesystem I/O.
                let (availability, needs_view) = if let Some(view) = &d.view {
                    let utc = view.interval.start + LOOKBACK_SECONDS;
                    let point = Interval::new(utc, utc + 1).expect("view point");
                    let covered = store.covered(d.channel, view.interval, now)?;
                    let availability = if point.missing(covered.iter().copied()).is_empty() {
                        if store.is_settled(d.channel, point)? {
                            Availability::Stored
                        } else {
                            Availability::Provisional
                        }
                    } else {
                        Availability::Unrequested
                    };
                    let needs_view = target
                        .as_ref()
                        .and_then(|t| t.range.intersection(view.interval))
                        .is_some_and(|wanted| !wanted.missing(covered).is_empty());
                    (availability, needs_view)
                } else {
                    (Availability::Unrequested, false)
                };
                let key = acquisition.as_ref().map(|a| (a.target.clone(), needs_view));
                if key != fetch_key || fetched_revision != Some(revision) {
                    if fetched_revision != Some(revision) {
                        blocked_target = None;
                    }
                    fetch_key = key;
                    fetched_revision = Some(revision);
                    next_fetch = Some(Instant::now() + SEEK_SETTLE);
                }
                if let Ok(mut output) = shared.output.lock() {
                    output.target = target.clone();
                    output.availability = availability;
                }
                // Only meaningful deadlines wake an idle acquisition: an
                // archive collection boundary or the one post-broadcast check.
                if next_fetch.is_none()
                    && let Some(a) = &acquisition
                    && blocked_target
                        .as_ref()
                        .is_none_or(|key| *key != a.target.key())
                {
                    let until = if a.target.range.end > archive_end(now) {
                        Some((now.div_euclid(COLLECTION_SECONDS) + 1) * COLLECTION_SECONDS)
                    } else {
                        let settled = a.target.range.end.saturating_add(SETTLED_SECONDS);
                        (now < settled).then_some(settled)
                    };
                    next_fetch = until.map(|until| {
                        Instant::now()
                            + Duration::from_secs(until.saturating_sub(now).max(1) as u64)
                    });
                }
                if let Some(job) = &job
                    && job.target == target
                {
                    match job.progress() {
                        Progress::Preparing => {}
                        Progress::Receiving => set_state(&shared, State::Loading),
                        Progress::Importing => set_state(&shared, State::Importing),
                    }
                }
                if job.is_none() && next_fetch.is_some_and(|until| Instant::now() >= until) {
                    // Also checks for a completed local response after restart.
                    // `download::next` forbids HTTP while fetch is disabled.
                    job = Some(Job::start(
                        directory.clone(),
                        owner.clone(),
                        acquisition,
                        endpoint.clone(),
                        now,
                    )?);
                    next_fetch = None;
                }
            } else {
                if let Ok(mut output) = shared.output.lock()
                    && output.source.take().is_some()
                {
                    output.records = Arc::default();
                    output.view = None;
                    output.target = None;
                    output.revision = output.revision.wrapping_add(1);
                }
                if job.is_none() && next_fetch.is_some_and(|until| Instant::now() >= until) {
                    job = Some(Job::start(
                        directory.clone(),
                        owner.clone(),
                        None,
                        endpoint.clone(),
                        now,
                    )?);
                    next_fetch = None;
                }
            }
            Ok(())
        })();
        if let Err(error) = process {
            tracing::error!(
                error = &error as &dyn std::error::Error,
                "Comment cache operation failed"
            );
            observed = None;
            set_state(&shared, State::StorageFailed(error.to_string()));
            storage_retry = Some(Instant::now() + LOCAL_RETRY);
            // Local recovery comes before another GET. The complete spool is
            // retained by the download/import typestates on every error path.
            next_fetch = Some(Instant::now() + LOCAL_RETRY);
        }
        if wait(&shared, POLL) {
            break;
        }
    }
    if let Some(job) = job {
        job.stop();
    }
    if let Err(error) = store.release_session(owner) {
        tracing::error!(
            error = &error as &dyn std::error::Error,
            "Comment cache session release failed"
        );
    }
    if let Err(error) = store.cleanup(wall(), false) {
        tracing::error!(
            error = &error as &dyn std::error::Error,
            "Comment cache cleanup failed"
        );
    }
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
            source_range: Source::Recording(Recording::Discovering),
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
