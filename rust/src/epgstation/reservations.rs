//! Serial, cancellable guide reservations owned independently of Qt.
mod data;
mod http;
#[cfg(test)]
mod tests;
use super::client::Connection;
use crate::services::{FetchError, Job, Network, Progress, Stopping};
pub use data::{Program, Snapshot, Status};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
const REFRESH_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Recording reservations are unavailable")]
    Unavailable,
    #[error("A reservation operation is already running")]
    Busy,
    #[error("Program information changed or the program ended")]
    ProgramChanged,
    #[error("The program already has a reservation")]
    AlreadyReserved,
    #[error("Only one future manual reservation can be cancelled")]
    NotCancellable,
    #[error("The rule reservation cannot be excluded")]
    NotExcludable,
    #[error("The rule reservation exclusion cannot be removed")]
    NotUnexcludable,
    #[error("Reservation response exceeds its limit")]
    Limit,
    #[error("Reservation response is inconsistent")]
    Invalid,
    #[error("{0}")]
    Json(#[source] JsonError),
    #[error("Unexpected reservation HTTP status {0}")]
    Status(u16),
    #[error("Reservation HTTP request failed: {0}")]
    Http(#[source] reqwest::Error),
    #[error("Reservation write completed, but the list could not be refreshed: {0}")]
    RefreshAfterWrite(#[source] Box<Error>),
}

// Retain the original cause without exposing its Display through source():
// serde may quote a password or token from an invalid response field.
#[derive(thiserror::Error)]
pub struct JsonError(crate::json::Error);
impl std::fmt::Display for JsonError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (path, source) = match &self.0 {
            crate::json::Error::Decode { path, source } => (path.to_string(), source),
            crate::json::Error::Trailing(source) => ("root".into(), source),
        };
        write!(
            formatter,
            "Invalid reservation JSON at {path}: {:?}, line {}, column {}",
            source.classify(),
            source.line(),
            source.column()
        )
    }
}
impl std::fmt::Debug for JsonError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, formatter)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Cancel,
    Exclude,
    Unexclude,
}
impl Change {
    pub fn name(self) -> &'static str {
        match self {
            Self::Cancel => "cancel",
            Self::Exclude => "exclude",
            Self::Unexclude => "unexclude",
        }
    }
    fn rejection(self) -> Error {
        match self {
            Self::Cancel => Error::NotCancellable,
            Self::Exclude => Error::NotExcludable,
            Self::Unexclude => Error::NotUnexcludable,
        }
    }
}
#[derive(Clone, Copy)]
enum Operation {
    Refresh,
    Add(Program),
    Change {
        program: Program,
        target: data::Target,
        change: Change,
    },
}
#[derive(Default)]
enum Mode {
    #[default]
    Disabled,
    Closed,
    Connected {
        generation: u64,
        connection: Connection,
    },
}
#[derive(Default)]
enum Work {
    #[default]
    Idle,
    Running {
        job: Job<Snapshot, Error>,
        operation: Operation,
    },
    Retiring(Stopping),
}
#[derive(Default)]
enum Catalogue {
    #[default]
    Unknown,
    Ready(Arc<Snapshot>),
}
pub struct Manager {
    mode: Mode,
    work: Work,
    catalogue: Catalogue,
    error: Option<FetchError<Error>>,
    next_refresh: Instant,
    revision: u64,
}
impl Default for Manager {
    fn default() -> Self {
        Self {
            mode: Mode::Disabled,
            work: Work::Idle,
            catalogue: Catalogue::Unknown,
            error: None,
            next_refresh: Instant::now(),
            revision: 0,
        }
    }
}
impl Manager {
    pub fn enabled(&self) -> bool {
        matches!(self.mode, Mode::Connected { .. })
    }
    pub fn busy(&self) -> bool {
        !matches!(self.work, Work::Idle)
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn error(&self) -> Option<&FetchError<Error>> {
        self.error.as_ref()
    }
    pub fn snapshot(&self) -> Arc<Snapshot> {
        match &self.catalogue {
            Catalogue::Unknown => Arc::default(),
            Catalogue::Ready(snapshot) => snapshot.clone(),
        }
    }
    pub(super) fn configure(&mut self, session: Option<(u64, &Connection)>, now: Instant) {
        if matches!(self.mode, Mode::Closed)
            || matches!((&self.mode, session), (Mode::Disabled, None))
            || matches!((&self.mode, session), (Mode::Connected { generation, .. }, Some((next, _))) if *generation == next)
        {
            return;
        }
        self.mode = match session {
            Some((generation, connection)) => Mode::Connected {
                generation,
                connection: connection.clone(),
            },
            None => Mode::Disabled,
        };
        self.work = match std::mem::take(&mut self.work) {
            Work::Running { job, .. } => Work::Retiring(job.cancel()),
            work => work,
        };
        self.catalogue = Catalogue::Unknown;
        self.error = None;
        self.next_refresh = now;
        self.revision = self.revision.wrapping_add(1);
    }
    pub fn shutdown(&mut self, now: Instant) {
        self.configure(None, now);
        // Qt may poll or finish closing signals before destroying Player.
        self.mode = Mode::Closed;
    }
    pub fn can_reserve(&self, program: Program, now: u64) -> bool {
        self.enabled()
            && !self.busy()
            && program.ongoing(now)
            && matches!(&self.catalogue, Catalogue::Ready(snapshot) if snapshot.status(program).is_none())
    }
    pub fn can_cancel(&self, program: Program, now: u64) -> bool {
        self.enabled()
            && !self.busy()
            && matches!(&self.catalogue, Catalogue::Ready(snapshot) if snapshot.cancellation(program, now).is_some())
    }
    pub fn can_change(&self, program: Program, now: u64, change: Change) -> bool {
        self.enabled()
            && !self.busy()
            && matches!(&self.catalogue, Catalogue::Ready(snapshot) if snapshot.target(program, now, change).is_some())
    }
    pub fn reject(&mut self, operation: &'static str, error: Error) -> bool {
        tracing::warn!(
            operation,
            error = &error as &dyn std::error::Error,
            "Reservation operation rejected"
        );
        self.error = Some(FetchError::Parse(error));
        self.revision = self.revision.wrapping_add(1);
        false
    }
    pub fn reserve(&mut self, network: &Network, program: Program, now_ms: u64) -> bool {
        if !self.can_reserve(program, now_ms) {
            let error = if self.busy() {
                Error::Busy
            } else if !self.enabled() {
                Error::Unavailable
            } else if self.snapshot().status(program).is_some() {
                Error::AlreadyReserved
            } else {
                Error::ProgramChanged
            };
            return self.reject("reserve", error);
        }
        self.begin(network, Operation::Add(program), now_ms);
        true
    }
    pub fn cancel(&mut self, network: &Network, program: Program, now_ms: u64) -> bool {
        self.change(network, program, now_ms, Change::Cancel)
    }
    pub fn change(
        &mut self,
        network: &Network,
        program: Program,
        now_ms: u64,
        change: Change,
    ) -> bool {
        if !self.can_change(program, now_ms, change) {
            let error = if self.busy() {
                Error::Busy
            } else if !self.enabled() {
                Error::Unavailable
            } else {
                change.rejection()
            };
            return self.reject(change.name(), error);
        }
        let target = self
            .snapshot()
            .target(program, now_ms, change)
            .expect("validated target");
        self.begin(
            network,
            Operation::Change {
                program,
                target,
                change,
            },
            now_ms,
        );
        true
    }
    pub fn refresh(&mut self, network: &Network) -> bool {
        if !self.enabled() || self.busy() {
            return false;
        }
        self.begin(network, Operation::Refresh, 0);
        true
    }
    fn begin(&mut self, network: &Network, operation: Operation, now_ms: u64) {
        let Mode::Connected { connection, .. } = &self.mode else {
            return;
        };
        let connection = connection.clone();
        self.work = Work::Running {
            job: network.job(async move {
                http::execute(connection, operation, now_ms)
                    .await
                    .map_err(FetchError::Parse)
            }),
            operation,
        };
        self.error = None;
        self.revision = self.revision.wrapping_add(1);
    }
    pub fn poll(&mut self, network: &Network, now: Instant) {
        self.work = match std::mem::take(&mut self.work) {
            Work::Idle => Work::Idle,
            Work::Retiring(job) => match job.poll() {
                Progress::Pending(job) => Work::Retiring(job),
                Progress::Complete(()) => {
                    self.revision = self.revision.wrapping_add(1);
                    Work::Idle
                }
            },
            Work::Running { job, operation } => match job.poll() {
                Progress::Pending(job) => Work::Running { job, operation },
                Progress::Complete(result) => {
                    self.revision = self.revision.wrapping_add(1);
                    self.next_refresh = now + REFRESH_INTERVAL;
                    match result {
                        Ok(snapshot) => {
                            self.catalogue = Catalogue::Ready(Arc::new(snapshot));
                            self.error = None;
                        }
                        Err(error) => {
                            let operation = match operation {
                                Operation::Refresh => "refresh",
                                Operation::Add(_) => "reserve",
                                Operation::Change { change, .. } => change.name(),
                            };
                            tracing::warn!(
                                operation,
                                error = &error as &dyn std::error::Error,
                                "EPGStation reservation operation failed"
                            );
                            self.catalogue = Catalogue::Unknown;
                            self.error = Some(error);
                        }
                    }
                    Work::Idle
                }
            },
        };
        if self.enabled() && !self.busy() && now >= self.next_refresh {
            self.begin(network, Operation::Refresh, 0);
        }
    }
}
