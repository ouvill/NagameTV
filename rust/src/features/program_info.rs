//! EPG acquisition lives independently of playback and of the guide's visibility.
use crate::channels::BroadcastService;
use crate::services::{FetchError, Job, Network, NetworkError};
pub mod browser;
pub mod guide;
mod model;
pub mod presentation;
use model::{Snapshot, parse};
use std::time::{Duration, Instant};

const MAX_RESPONSE: usize = 32 * 1024 * 1024;
const MAX_PROGRAMS: usize = 50_000;
const REFRESH: Duration = Duration::from_secs(300);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("番組JSONの解析失敗: {0}")]
    Json(#[from] serde_json::Error),
    #[error("番組数が上限を超えています（{actual} > {limit}）")]
    TooManyPrograms { actual: usize, limit: usize },
}

type Request = Job<Snapshot, Error>;
type RequestError = FetchError<Error>;

#[derive(Default)]
enum Outcome {
    #[default]
    Waiting,
    Ready,
    Failed(RequestError),
}

// Each active state owns exactly one job. Cancelling retains ownership until
// the worker finishes; its result can never be accepted as a fresh snapshot.
enum Acquisition {
    Idle {
        next: Option<Instant>,
        outcome: Outcome,
    },
    Fetching(Request),
    Cancelling(Request),
}
impl Default for Acquisition {
    fn default() -> Self {
        Self::Idle {
            next: None,
            outcome: Outcome::Waiting,
        }
    }
}

pub enum Status<'a> {
    Disabled,
    Waiting,
    Fetching,
    Cancelling,
    Ready(usize),
    Failed(&'a RequestError),
}

#[derive(Default)]
pub struct ProgramInfo {
    desired: Option<String>,
    acquisition: Acquisition,
    snapshot: Snapshot,
    pub revision: u64,
}
impl ProgramInfo {
    /// Invalidate before cancellation. Never reuse results, even for A -> B -> A.
    pub fn configure(&mut self, server: Option<String>) {
        if self.desired == server {
            return;
        }
        self.desired = server;
        self.snapshot = Snapshot::default();
        self.revision += 1;
        self.acquisition = match std::mem::take(&mut self.acquisition) {
            Acquisition::Fetching(job) | Acquisition::Cancelling(job) => {
                job.cancel();
                Acquisition::Cancelling(job)
            }
            Acquisition::Idle { .. } => Acquisition::default(),
        };
    }
    pub fn refresh(&mut self) {
        if let Acquisition::Idle { next, .. } = &mut self.acquisition {
            *next = None;
        }
    }
    pub fn poll(&mut self, network: &Network) {
        self.poll_at(network, Instant::now());
    }
    fn poll_at(&mut self, network: &Network, now: Instant) {
        self.acquisition = match std::mem::take(&mut self.acquisition) {
            Acquisition::Cancelling(job) if job.is_finished() => Acquisition::default(),
            Acquisition::Fetching(job) if job.is_finished() => {
                let outcome = match job
                    .poll()
                    .unwrap_or_else(|| Err(NetworkError::WorkerStopped.into()))
                {
                    Ok(programs) => {
                        programs.record_storage();
                        self.snapshot = programs;
                        self.revision += 1;
                        Outcome::Ready
                    }
                    Err(error) => Outcome::Failed(error),
                };
                Acquisition::Idle {
                    next: Some(now + REFRESH),
                    outcome,
                }
            }
            state => state,
        };
        if let Some(server) = &self.desired
            && matches!(&self.acquisition, Acquisition::Idle { next, .. } if next.is_none_or(|deadline| now >= deadline))
        {
            self.acquisition = Acquisition::Fetching(network.fetch_json(
                format!("{server}/api/programs"),
                MAX_RESPONSE,
                parse,
            ));
        }
    }
    pub fn status(&self) -> Status<'_> {
        match &self.acquisition {
            Acquisition::Fetching(_) => Status::Fetching,
            Acquisition::Cancelling(_) => Status::Cancelling,
            Acquisition::Idle { .. } if self.desired.is_none() => Status::Disabled,
            Acquisition::Idle {
                outcome: Outcome::Waiting,
                ..
            } => Status::Waiting,
            Acquisition::Idle {
                outcome: Outcome::Ready,
                ..
            } => Status::Ready(self.snapshot.len()),
            Acquisition::Idle {
                outcome: Outcome::Failed(error),
                ..
            } => Status::Failed(error),
        }
    }
    /// Borrow only the requested service's bounded schedule while the guide is open.
    pub fn view(
        &self,
        service: Option<BroadcastService>,
        window: guide::DayWindow,
    ) -> Result<String, serde_json::Error> {
        self.snapshot.view(service, window)
    }
    pub fn browser_presentation(
        &self,
        projection: &mut browser::Projection,
        channels: &[crate::channels::Channel],
        now: Option<u64>,
    ) -> Result<Option<String>, serde_json::Error> {
        projection.update(&self.snapshot, self.revision, channels, now)
    }
    pub fn current_presentation(
        &self,
        projection: &mut presentation::Projection,
        service: Option<BroadcastService>,
        now_ms: u64,
    ) -> Result<presentation::Update, serde_json::Error> {
        projection.update(&self.snapshot, self.revision, service, now_ms)
    }
    pub fn counters(&self) -> (usize, usize, bool) {
        (
            usize::from(!matches!(self.acquisition, Acquisition::Idle { .. })),
            self.snapshot.len(),
            matches!(self.acquisition, Acquisition::Cancelling(_)),
        )
    }
}

#[cfg(test)]
mod tests;
