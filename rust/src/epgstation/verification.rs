//! Explicit, anonymous probes establish a session without publishing a recording page.
use super::{Connection, Endpoint, Error, client};
use crate::services::{FetchError, Job, Network, Progress, Stopping};

#[derive(Default)]
pub(super) enum Verification {
    #[default]
    Idle,
    Fetching {
        endpoint: Endpoint,
        job: Job<Connection, Error>,
    },
    Retiring {
        stopping: Stopping,
        next: Option<Endpoint>,
    },
    Closed(Option<Stopping>),
}
impl Verification {
    pub(super) fn request(&mut self, network: &Network, endpoint: Endpoint) -> bool {
        match self {
            Self::Idle => {
                *self = Self::start(network, endpoint);
                true
            }
            Self::Retiring { next, .. } => {
                *next = Some(endpoint);
                true
            }
            Self::Fetching { .. } | Self::Closed(_) => false,
        }
    }
    fn start(network: &Network, endpoint: Endpoint) -> Self {
        Self::Fetching {
            job: network.job(client::verify(endpoint.clone())),
            endpoint,
        }
    }
    pub(super) fn cancel(&mut self) {
        *self = match std::mem::take(self) {
            Self::Fetching { job, .. } => Self::Retiring {
                stopping: job.cancel(),
                next: None,
            },
            Self::Retiring { stopping, .. } => Self::Retiring {
                stopping,
                next: None,
            },
            state @ (Self::Idle | Self::Closed(_)) => state,
        };
    }
    pub(super) fn close(&mut self) {
        self.cancel();
        *self = match std::mem::take(self) {
            Self::Retiring { stopping, .. } => Self::Closed(Some(stopping)),
            Self::Idle => Self::Closed(None),
            state @ Self::Closed(_) => state,
            Self::Fetching { .. } => unreachable!("cancelled above"),
        };
    }
    pub(super) fn poll(&mut self, network: &Network) -> Option<(Endpoint, Connection)> {
        let mut verified = None;
        *self = match std::mem::take(self) {
            Self::Idle => Self::Idle,
            Self::Fetching { endpoint, job } => match job.poll() {
                Progress::Pending(job) => Self::Fetching { endpoint, job },
                Progress::Complete(result) => {
                    match result {
                        Ok(connection) => verified = Some((endpoint, connection)),
                        Err(error) => log_failure(error),
                    }
                    Self::Idle
                }
            },
            Self::Retiring { stopping, next } => match stopping.poll() {
                Progress::Pending(stopping) => Self::Retiring { stopping, next },
                Progress::Complete(()) => match next {
                    Some(endpoint) => Self::start(network, endpoint),
                    None => Self::Idle,
                },
            },
            Self::Closed(Some(stopping)) => Self::Closed(match stopping.poll() {
                Progress::Pending(stopping) => Some(stopping),
                Progress::Complete(()) => None,
            }),
            Self::Closed(None) => Self::Closed(None),
        };
        verified
    }
}
fn log_failure(error: FetchError<Error>) {
    // serde errors can quote response values; preserve only safe field/cause diagnostics.
    match error {
        FetchError::Parse(Error::Json(error)) => log(&super::reservations::JsonError(error)),
        error => log(&error),
    }
}
fn log(error: &(dyn std::error::Error + 'static)) {
    tracing::warn!(
        operation = "verify EPGStation reservation session",
        error,
        "EPGStation reservation session remains unverified"
    );
}
