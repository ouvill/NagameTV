//! A single NX-Jikkyo client owns retry history across selection and display changes.
pub use crate::retry::Blocked;
use crate::{
    activity,
    connection::{Connection, Endpoints},
    retry::{Failure, Operation, Policy},
};
use futures_util::FutureExt;
use std::{
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{runtime::Handle, task::JoinHandle};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    policy: Arc<Mutex<Policy>>,
}
impl Client {
    pub fn new() -> Result<Self, reqwest::Error> {
        Ok(Self {
            http: reqwest::Client::builder()
                .connect_timeout(CONNECT_TIMEOUT)
                .timeout(HTTP_TIMEOUT)
                .pool_max_idle_per_host(1)
                .redirect(reqwest::redirect::Policy::none())
                .retry(reqwest::retry::never())
                .build()?,
            policy: Arc::default(),
        })
    }
    pub(crate) fn check(
        &self,
        operation: Operation,
        endpoint: &str,
        now: Instant,
    ) -> Result<(), Blocked> {
        self.policy
            .lock()
            .expect("NX retry policy")
            .check(operation, endpoint, now)
    }
    fn authorize<T>(
        &self,
        operation: Operation,
        endpoint: String,
        target: T,
        now: Instant,
    ) -> Result<Authorized<T>, Blocked> {
        self.check(operation, &endpoint, now)?;
        Ok(Authorized {
            target,
            attempt: Attempt {
                client: self.clone(),
                operation,
                endpoint,
                at: now,
                started: Instant::now(),
            },
        })
    }
    pub(crate) fn connect(
        &self,
        runtime: &Handle,
        endpoints: Endpoints,
        now: Instant,
    ) -> Result<Connection, Blocked> {
        let request = self.authorize(Operation::Live, endpoints.threads.clone(), endpoints, now)?;
        Ok(Connection::start(runtime, request))
    }
    pub(crate) fn worker_failed(&self, endpoint: &str, now: Instant) {
        self.policy.lock().expect("NX retry policy").failed(
            Operation::Live,
            endpoint,
            Failure::Temporary,
            "Comment task stopped".into(),
            now,
        );
    }
    pub(crate) fn post(
        &self,
        endpoint: String,
        now: Instant,
    ) -> Result<Authorized<String>, Blocked> {
        self.authorize(Operation::Posting, endpoint.clone(), endpoint, now)
    }
    pub fn activity(
        &self,
        runtime: &Handle,
        endpoint: String,
        now: Instant,
    ) -> Result<Request, Blocked> {
        let request = self.authorize(Operation::Activity, endpoint.clone(), endpoint, now)?;
        let (tx, rx) = mpsc::sync_channel(1);
        let task = runtime.spawn(async move {
            let (_, attempt) = request.into_parts();
            let result: Result<_, ActivityError> = async {
                let mut response = attempt.get().await?;
                let mut bytes = Vec::new();
                while let Some(chunk) = response.chunk().await? {
                    if chunk.len() > activity::MAX_RESPONSE_BYTES - bytes.len() {
                        return Err(ActivityError::Parse(activity::Error::ResponseTooLarge));
                    }
                    bytes.extend_from_slice(&chunk);
                }
                Ok(activity::Snapshot::parse(&bytes)?)
            }
            .await;
            match &result {
                Ok(_) => attempt
                    .client
                    .policy
                    .lock()
                    .expect("NX retry policy")
                    .activity_succeeded(attempt.now()),
                Err(error) => attempt.failed(error.failure(), error.to_string()),
            }
            let _ = tx.send(result);
        });
        Ok(Request {
            task: Task(task),
            rx,
        })
    }
}

/// Authorization is bound to the target and cannot be cloned or constructed by a caller.
pub(crate) struct Authorized<T> {
    target: T,
    attempt: Attempt,
}
impl<T> Authorized<T> {
    pub fn into_parts(self) -> (T, Attempt) {
        (self.target, self.attempt)
    }
}
pub(crate) struct Attempt {
    client: Client,
    operation: Operation,
    endpoint: String,
    at: Instant,
    started: Instant,
}
impl Attempt {
    pub fn now(&self) -> Instant {
        self.at + self.started.elapsed()
    }
    pub fn check(&self) -> Result<(), Blocked> {
        self.client
            .check(self.operation, &self.endpoint, self.now())
    }
    pub async fn get(&self) -> Result<reqwest::Response, HttpError> {
        self.check()?;
        let response = self.client.http.get(&self.endpoint).send().await?;
        if !response.status().is_success() {
            return Err(HttpError::Response(ResponseError::new(
                response.status(),
                response.headers(),
            )));
        }
        Ok(response)
    }
    pub fn failed(&self, failure: Option<Failure>, message: String) {
        if let Some(failure) = failure {
            self.client.policy.lock().expect("NX retry policy").failed(
                self.operation,
                &self.endpoint,
                failure,
                message,
                self.now(),
            );
        }
    }
    pub fn receiving(&self) -> Reception<'_> {
        Reception {
            attempt: self,
            since: Instant::now(),
        }
    }
    pub fn posting_succeeded(&self) {
        self.client
            .policy
            .lock()
            .expect("NX retry policy")
            .posting_succeeded();
    }
}

/// Only the established-subscription path creates this guard. Cancellation and
/// disconnect both preserve a short session's streak; quiet stable sessions
/// reset it without requiring a comment or a timely GUI poll.
pub(crate) struct Reception<'a> {
    attempt: &'a Attempt,
    since: Instant,
}
impl Drop for Reception<'_> {
    fn drop(&mut self) {
        if self.since.elapsed() >= crate::retry::STABLE_CONNECTION {
            self.attempt
                .client
                .policy
                .lock()
                .expect("NX retry policy")
                .stable_live();
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("NX-Jikkyo HTTP {status}")]
pub struct ResponseError {
    status: reqwest::StatusCode,
    retry_after: Option<Duration>,
}
impl ResponseError {
    pub(crate) fn new(status: reqwest::StatusCode, headers: &reqwest::header::HeaderMap) -> Self {
        let retry_after = headers
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|value| retry_after(value, SystemTime::now()));
        Self {
            status,
            retry_after,
        }
    }
    pub(crate) fn failure(&self) -> Failure {
        Failure::Http {
            status: self.status,
            retry_after: self.retry_after,
        }
    }
}
fn retry_after(value: &str, wall: SystemTime) -> Option<Duration> {
    let value = value.trim();
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    let time =
        time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc2822).ok()?;
    let seconds = u64::try_from(time.unix_timestamp()).ok()?;
    Some(
        (UNIX_EPOCH + Duration::from_secs(seconds))
            .duration_since(wall)
            .unwrap_or_default(),
    )
}

#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("NX-Jikkyo HTTP transport failed: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("{0}")]
    Response(#[from] ResponseError),
    #[error("{0}")]
    Blocked(#[from] Blocked),
}
impl HttpError {
    pub(crate) fn failure(&self) -> Option<Failure> {
        match self {
            Self::Transport(_) => Some(Failure::Temporary),
            Self::Response(error) => Some(error.failure()),
            Self::Blocked(_) => None,
        }
    }
}
#[derive(Debug, thiserror::Error)]
pub enum ActivityError {
    #[error("{0}")]
    Http(#[from] HttpError),
    #[error("{0}")]
    Transport(#[from] reqwest::Error),
    #[error("{0}")]
    Parse(#[from] activity::Error),
    #[error("NX-Jikkyo activity worker stopped")]
    WorkerStopped,
}
impl ActivityError {
    fn failure(&self) -> Option<Failure> {
        match self {
            Self::Http(error) => error.failure(),
            Self::Transport(_) | Self::Parse(_) | Self::WorkerStopped => Some(Failure::Temporary),
        }
    }
}

pub enum Progress<P, T> {
    Pending(P),
    Complete(T),
}
pub struct Request {
    task: Task,
    rx: mpsc::Receiver<Result<activity::Snapshot, ActivityError>>,
}
impl Request {
    pub fn is_finished(&self) -> bool {
        self.task.0.is_finished()
    }
    pub fn poll(self) -> Progress<Self, Result<activity::Snapshot, ActivityError>> {
        if !self.is_finished() {
            return Progress::Pending(self);
        }
        Progress::Complete(
            self.rx
                .try_recv()
                .unwrap_or(Err(ActivityError::WorkerStopped)),
        )
    }
    pub fn cancel(self) -> Cancelling {
        self.task.0.abort();
        Cancelling(self.task)
    }
}
pub struct Cancelling(Task);
impl Cancelling {
    pub fn is_finished(&self) -> bool {
        self.0.0.is_finished()
    }
    pub fn poll(mut self) -> Progress<Self, ()> {
        if !self.is_finished() {
            return Progress::Pending(self);
        }
        // Observe the join after completion; no result may escape cancellation.
        let _ = (&mut self.0.0).now_or_never();
        Progress::Complete(())
    }
}
struct Task(JoinHandle<()>);
impl Drop for Task {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[cfg(test)]
mod tests;
