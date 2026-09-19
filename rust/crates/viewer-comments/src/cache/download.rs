use super::{
    spool,
    store::{Reservation, Store},
    *,
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const READ_TIMEOUT: Duration = Duration::from_secs(30);
const PREFETCH_SECONDS: i64 = 60;
const MIN_REQUEST_SECONDS: i64 = 2;
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
    RemoteFailed(String),
}
pub(super) struct Job {
    task: thread::JoinHandle<Result<Outcome, Error>>,
    cancel: Arc<Cancellation>,
}
impl Job {
    pub fn start(
        directory: PathBuf,
        owner: String,
        demand: Demand,
        endpoint: String,
        now: i64,
    ) -> Result<Self, Error> {
        let cancel = Arc::new(Cancellation::new());
        let stopping = cancel.clone();
        let task = thread::Builder::new()
            .name("comment-archive".into())
            .spawn(move || run(directory, owner, demand, endpoint, now, &stopping))?;
        Ok(Self { task, cancel })
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

fn next(
    store: &Store,
    owner: &str,
    demand: &Demand,
    now: i64,
) -> Result<Option<(u16, Interval)>, Error> {
    if !demand.fetch {
        return Ok(None);
    }
    let Some(view) = &demand.view else {
        return Ok(None);
    };
    let (spans, whole) = match &demand.source_range {
        Source::Pending => return Ok(None),
        Source::Recording(RecordingRange::Known(spans)) => (spans, true),
        Source::Recording(RecordingRange::Discovering(spans)) => (spans, false),
        Source::Live { at_edge: true, .. } => return Ok(None),
        Source::Live { spans, .. } => (spans, false),
    };
    let mut missing = Vec::new();
    for span in spans {
        if !whole && (span.channel != demand.channel || span.key != view.clock_key) {
            continue;
        }
        let Some(mut range) = span.interval() else {
            continue;
        };
        if !whole {
            let Some(part) = range.intersection(
                Interval::new(
                    view.interval.start,
                    view.interval.start.saturating_add(FALLBACK_SECONDS),
                )
                .expect("view interval is bounded"),
            ) else {
                continue;
            };
            range = part;
        }
        let Some(range) = Interval::new(range.start, range.end.min(archive_end(now))) else {
            continue;
        };
        let mut covered = store.covered(span.channel, range, now)?;
        if matches!(demand.source_range, Source::Live { .. }) {
            covered.extend(store.live_covered(owner, span.channel, &span.key, range)?);
        }
        for gap in range.missing(covered) {
            if !whole && gap.start >= view.interval.start + LOOKBACK_SECONDS + PREFETCH_SECONDS {
                continue;
            }
            // The provider's inclusive endpoints require starttime < endtime.
            // Pad a one-second hole inside the verified scope only.
            let gap = if gap.end - gap.start >= MIN_REQUEST_SECONDS {
                Some(gap)
            } else if gap.end < range.end {
                Interval::new(gap.start, gap.end + 1)
            } else if gap.start > range.start {
                Interval::new(gap.start - 1, gap.end)
            } else {
                None
            };
            if let Some(gap) = gap {
                missing.push((span.channel, gap));
            }
        }
    }
    missing.sort_by_key(|(channel, range)| {
        (
            !(*channel == demand.channel && range.intersection(view.interval).is_some()),
            range.start,
        )
    });
    // Adjacent verified scopes of the same channel can share a request. Do not
    // bridge a saved interval or a gap in broadcast-clock verification.
    let Some((channel, mut selected)) = missing.first().copied() else {
        return Ok(None);
    };
    loop {
        let previous = selected;
        for &(next_channel, range) in &missing {
            if next_channel == channel && range.start <= selected.end && range.end >= selected.start
            {
                selected =
                    Interval::new(selected.start.min(range.start), selected.end.max(range.end))
                        .expect("union");
            }
        }
        if selected == previous {
            break;
        }
    }
    Ok(Some((channel, selected.request_prefix())))
}

fn run(
    directory: PathBuf,
    owner: String,
    demand: Demand,
    endpoint: String,
    now: i64,
    cancel: &Cancellation,
) -> Result<Outcome, Error> {
    let started = Instant::now();
    let failure_time = || now.saturating_add(started.elapsed().as_secs() as i64);
    let mut store = Store::open(&directory)?;
    let Some(lease) = store.try_provider()? else {
        return Ok(Outcome::Waiting(now + 1));
    };
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
            let Some((channel, range)) = next(&store, &owner, &demand, now)? else {
                return Ok(Outcome::Idle);
            };
            let eligible = match store.reserve(&lease, channel, range, now)? {
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
            match runtime.block_on(receive(receiving, &endpoint, cancel)) {
                Ok(file) => file,
                Err(ReceiveError::Local(error)) => return Err(error),
                Err(ReceiveError::Remote {
                    message,
                    permanent,
                    retry_after,
                }) => {
                    spool::discard(&directory)?;
                    store.failed(permanent, retry_after, failure_time())?;
                    return Ok(Outcome::RemoteFailed(message));
                }
            }
        }
    };
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
            file.discard()?;
            store.failed(true, None, failure_time())?;
            return Ok(Outcome::RemoteFailed(error.to_string()));
        }
        Err((_file, error)) => return Err(error),
    };
    validated.import(&mut store, || cancel.stopped())?;
    validated.finish()?;
    Ok(Outcome::Complete)
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

#[cfg(test)]
mod tests {
    use super::spool::Receipt;
    use super::*;
    #[test]
    fn fallback_waits_until_prefetch_boundary_and_pads_only_one_second_holes() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let owner = store.session().unwrap();
        let span = ClockSpan {
            key: "clock".into(),
            channel: 1,
            media_start_ms: 0,
            media_end_ms: 3_600_000,
            utc_start_ms: 100_000_000,
        };
        let mut incoming = spool::Receiving::prepare(
            dir.path(),
            Receipt {
                id: "cached".into(),
                channel: 1,
                range: Interval::new(100_000, 101_800).unwrap(),
                fetched: 300_000,
                generation: store.generation().unwrap(),
            },
        )
        .unwrap();
        incoming.write(br#"{"packet":[]}"#).unwrap();
        let file = incoming
            .finish()
            .unwrap()
            .validate()
            .map_err(|(_, e)| e)
            .unwrap();
        file.import(&mut store, || false).unwrap();
        file.finish().unwrap();
        let mut demand = Demand {
            source: 1,
            channel: 1,
            fetch: true,
            view: Some(View {
                clock_key: "clock".into(),
                interval: Interval::new(100_001, 100_121).unwrap(),
            }),
            source_range: Source::Recording(RecordingRange::Discovering(vec![span.clone()])),
        };
        assert!(
            next(&store, &owner.name, &demand, 300_001)
                .unwrap()
                .is_none()
        );
        demand.view.as_mut().unwrap().interval = Interval::new(101_730, 101_850).unwrap();
        assert_eq!(
            next(&store, &owner.name, &demand, 300_001).unwrap(),
            Some((1, Interval::new(101_800, 103_530).unwrap()))
        );
        let short = ClockSpan {
            media_end_ms: 1_801_000,
            ..span
        };
        demand.source_range = Source::Recording(RecordingRange::Known(vec![short]));
        assert_eq!(
            next(&store, &owner.name, &demand, 300_001).unwrap(),
            Some((1, Interval::new(101_799, 101_801).unwrap()))
        );
    }
    #[test]
    fn fresh_recording_is_one_request_and_live_edge_is_never_requested() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let owner = store.session().unwrap();
        let span = ClockSpan {
            key: "clock".into(),
            channel: 1,
            media_start_ms: 0,
            media_end_ms: 3_600_000,
            utc_start_ms: 100_000_000,
        };
        let mut demand = Demand {
            source: 1,
            channel: 1,
            view: Some(View {
                clock_key: "clock".into(),
                interval: Interval::new(100_100, 100_220).unwrap(),
            }),
            source_range: Source::Recording(RecordingRange::Known(vec![span.clone()])),
            fetch: true,
        };
        assert_eq!(
            next(&store, &owner.name, &demand, 200_000).unwrap(),
            Some((1, Interval::new(100_000, 103_600).unwrap()))
        );
        demand.source_range = Source::Live {
            earliest_media_ms: 0,
            spans: vec![span],
            reception: Reception::Receiving(1),
            at_edge: true,
        };
        assert!(
            next(&store, &owner.name, &demand, 200_000)
                .unwrap()
                .is_none()
        );
        demand.fetch = false;
        assert!(
            next(&store, &owner.name, &demand, 200_000)
                .unwrap()
                .is_none()
        );
    }
}
