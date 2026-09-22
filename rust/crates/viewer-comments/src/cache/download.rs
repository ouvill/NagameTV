use super::{
    spool,
    store::{Planned, Reservation, Store},
    *,
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const READ_TIMEOUT: Duration = Duration::from_secs(30);
#[cfg(test)]
mod integration_tests;

pub(super) struct Cancellation {
    stopped: AtomicBool,
    wake: tokio::sync::Notify,
}
impl Cancellation {
    fn new() -> Self {
        Self {
            stopped: AtomicBool::new(false),
            wake: tokio::sync::Notify::new(),
        }
    }
    pub fn stopped(&self) -> bool {
        self.stopped.load(Ordering::Acquire)
    }
    fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        self.wake.notify_one();
    }
}
pub(super) enum Outcome {
    Complete,
    Idle,
    Waiting(i64),
    RemoteFailed {
        target: Option<plan::Target>,
        message: String,
        retry_at: Option<i64>,
    },
}
pub(super) struct Job {
    task: thread::JoinHandle<Result<Outcome, Error>>,
    cancel: Arc<Cancellation>,
    progress: Arc<AtomicU8>,
    pub target: Option<plan::Target>,
}
impl Job {
    pub fn start(
        directory: PathBuf,
        owner: String,
        demand: Option<Acquisition>,
        endpoint: String,
        now: i64,
    ) -> Result<Self, Error> {
        let cancel = Arc::new(Cancellation::new());
        let stopping = cancel.clone();
        let progress = Arc::new(AtomicU8::new(Progress::Preparing as u8));
        let reporting = progress.clone();
        let target = demand.as_ref().map(|d| d.target.clone());
        let task = thread::Builder::new()
            .name("comment-archive".into())
            .spawn(move || {
                run_progress(
                    directory, owner, demand, endpoint, now, &stopping, &reporting,
                )
            })?;
        Ok(Self {
            task,
            cancel,
            progress,
            target,
        })
    }
    pub fn progress(&self) -> Progress {
        match self.progress.load(Ordering::Acquire) {
            value if value == Progress::Preparing as u8 => Progress::Preparing,
            value if value == Progress::Receiving as u8 => Progress::Receiving,
            value if value == Progress::Importing as u8 => Progress::Importing,
            _ => unreachable!("private progress state"),
        }
    }
    pub fn finished(&self) -> bool {
        self.task.is_finished()
    }
    pub fn finish(self) -> Result<Outcome, Error> {
        self.task
            .join()
            .map_err(|_| Error::Format("archive worker stopped".into()))?
    }
    pub fn stop(self) {
        self.cancel.stop();
        let _ = self.task.join();
    }
}

#[repr(u8)]
pub(super) enum Progress {
    Preparing,
    Receiving,
    Importing,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Acquisition {
    pub target: plan::Target,
    pub focus: Option<i64>,
}

fn next(store: &mut Store, owner: &str, demand: &Acquisition, now: i64) -> Result<Planned, Error> {
    let plan = store.planned(owner, &demand.target, now)?;
    if let Planned::Ready(request) = &plan
        && demand.target.range.end > archive_end(now)
        && demand
            .focus
            .is_some_and(|utc| request.range.start > utc + plan::PROGRAM_PADDING_SECONDS)
    {
        return Ok(Planned::Complete);
    }
    Ok(plan)
}

#[cfg(test)]
fn run(
    directory: PathBuf,
    owner: String,
    demand: Demand,
    endpoint: String,
    now: i64,
    cancel: &Cancellation,
) -> Result<Outcome, Error> {
    let target = plan::Planner::default().update(&demand, now);
    let acquisition = target.filter(|_| demand.fetch).map(|target| Acquisition {
        target,
        focus: demand
            .view
            .as_ref()
            .map(|v| v.interval.start + LOOKBACK_SECONDS),
    });
    run_progress(
        directory,
        owner,
        acquisition,
        endpoint,
        now,
        cancel,
        &AtomicU8::new(Progress::Preparing as u8),
    )
}

fn run_progress(
    directory: PathBuf,
    owner: String,
    demand: Option<Acquisition>,
    endpoint: String,
    now: i64,
    cancel: &Cancellation,
    progress: &AtomicU8,
) -> Result<Outcome, Error> {
    let started = Instant::now();
    let failure_time = || now.saturating_add(started.elapsed().as_secs() as i64);
    let mut store = Store::open(&directory)?;
    let Some(lease) = store.try_provider()? else {
        return Ok(Outcome::Waiting(now + REQUEST_SPACING_SECONDS));
    };
    // A download owns its pin independently of the player's current source.
    // A seek or source reset cannot make the in-flight programme evictable.
    let session = store.session()?;
    let result = (|| {
        if let Some(demand) = &demand {
            store.pin_target(&session.name, &demand.target, now)?;
        }
        let downloaded = match spool::recover(&directory)? {
            spool::Recovery::Downloaded(file) => {
                store.discard_staged(&lease, Some(&file.receipt.id))?;
                file
            }
            spool::Recovery::Partial => {
                store.failed(false, None, now)?;
                return Ok(Outcome::Waiting(now + 5 * 60));
            }
            spool::Recovery::None => {
                store.discard_staged(&lease, None)?;
                let Some(demand) = &demand else {
                    return Ok(Outcome::Idle);
                };
                let request = match next(&mut store, &owner, demand, now)? {
                    Planned::Complete => return Ok(Outcome::Idle),
                    Planned::Waiting(until) => return Ok(Outcome::Waiting(until)),
                    Planned::Failed(message) => {
                        return Ok(Outcome::RemoteFailed {
                            target: Some(demand.target.clone()),
                            message,
                            retry_at: None,
                        });
                    }
                    Planned::Ready(request) => request,
                };
                tracing::debug!(target: "comment_archive", start = request.range.start, end = request.range.end, refresh = request.refresh, "planned archive range");
                let eligible = match store.reserve_plan(&lease, request, now)? {
                    Reservation::Ready => return Ok(Outcome::Complete),
                    Reservation::Waiting(until) => return Ok(Outcome::Waiting(until)),
                    Reservation::Granted(eligible) => eligible,
                };
                static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
                let receiving = eligible.begin(format!(
                    "{}-{now}-{}",
                    owner,
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ))?;
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()?;
                progress.store(Progress::Receiving as u8, Ordering::Release);
                tracing::debug!(target: "comment_archive", channel = demand.target.channel, target_key = %demand.target.key(), start = demand.target.range.start, end = demand.target.range.end, "sending archive request");
                match runtime.block_on(receive(receiving, &endpoint, cancel)) {
                    Ok(file) => file,
                    Err(ReceiveError::Local(error)) => return Err(error),
                    Err(ReceiveError::Remote {
                        message,
                        permanent,
                        retry_after,
                    }) => {
                        spool::discard(&directory)?;
                        let retry_at = store.fail_target(
                            &demand.target,
                            &message,
                            permanent,
                            retry_after,
                            failure_time(),
                        )?;
                        return Ok(Outcome::RemoteFailed {
                            target: Some(demand.target.clone()),
                            message,
                            retry_at,
                        });
                    }
                }
            }
        };
        if let Some(plan) = &downloaded.receipt.plan {
            store.pin_target(&session.name, &plan.target, now)?;
        } else {
            store.pin_archive(
                &session.name,
                downloaded.receipt.channel,
                downloaded.receipt.range,
                now,
            )?;
        }
        if cancel.stopped() {
            return Err(Error::Cancelled);
        }
        if store.imported(&downloaded.receipt.id)? || !store.can_publish(&downloaded.receipt)? {
            store.discard_staged(&lease, None)?;
            downloaded.discard()?;
            return Ok(Outcome::Complete);
        }
        let validated = match downloaded.validate_with(|| cancel.stopped()) {
            Ok(file) => file,
            Err((file, Error::Archive(error))) => {
                let target = file.receipt.plan.as_ref().map(|plan| plan.target.clone());
                file.discard()?;
                let message = error.to_string();
                if let Some(target) = &target {
                    store.fail_target(target, &message, true, None, failure_time())?;
                } else {
                    store.failed(true, None, failure_time())?;
                }
                return Ok(Outcome::RemoteFailed {
                    target,
                    message,
                    retry_at: None,
                });
            }
            Err((_file, error)) => return Err(error),
        };
        progress.store(Progress::Importing as u8, Ordering::Release);
        validated.import(&mut store, || cancel.stopped())?;
        validated.finish()?;
        Ok(Outcome::Complete)
    })();
    store.release_session(&session.name)?;
    result
}

enum ReceiveError {
    Local(Error),
    Remote {
        message: String,
        permanent: bool,
        retry_after: Option<i64>,
    },
}
impl From<Error> for ReceiveError {
    fn from(error: Error) -> Self {
        Self::Local(error)
    }
}
fn transport(error: reqwest::Error) -> ReceiveError {
    ReceiveError::Remote {
        message: error.without_url().to_string(),
        permanent: false,
        retry_after: None,
    }
}
fn retry_after(value: &str) -> Option<i64> {
    if let Ok(seconds) = value.parse::<u64>() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_secs();
        return i64::try_from(now.checked_add(seconds)?).ok();
    }
    time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc2822)
        .ok()
        .map(|time| time.unix_timestamp())
}
async fn receive(
    mut receiving: spool::Receiving,
    endpoint: &str,
    cancel: &Cancellation,
) -> Result<spool::Downloaded, ReceiveError> {
    let (channel, range) = receiving.target();
    let client = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(READ_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .build()
        .map_err(transport)?;
    let url = format!(
        "{}/jk{channel}?starttime={}&endtime={}&format=json",
        endpoint.trim_end_matches('/'),
        range.start,
        range.end - 1
    );
    let work = async {
        let mut response = client.get(url).send().await.map_err(transport)?;
        if !response.status().is_success() {
            let status = response.status();
            let retry = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(retry_after);
            return Err(ReceiveError::Remote {
                message: format!("過去ログ HTTP {status}"),
                permanent: !(status.is_server_error() || status.as_u16() == 429),
                retry_after: retry,
            });
        }
        let started = Instant::now();
        let mut bytes = 0_u64;
        while let Some(chunk) = response.chunk().await.map_err(transport)? {
            receiving.write(&chunk)?;
            bytes = bytes.saturating_add(chunk.len() as u64);
        }
        // No full-response buffer, total byte limit, count limit, HEAD or size
        // retry. Disk I/O is backpressure on this dedicated download thread.
        tracing::debug!(target: "comment_archive", channel, start = range.start, end = range.end,
            bytes, elapsed_ms = started.elapsed().as_millis() as u64, "response saved");
        receiving.finish().map_err(ReceiveError::from)
    };
    if cancel.stopped() {
        return Err(ReceiveError::Local(Error::Cancelled));
    }
    tokio::select! {
        result=work=>result,
        _=cancel.wake.notified()=>Err(ReceiveError::Local(Error::Cancelled)),
    }
}
