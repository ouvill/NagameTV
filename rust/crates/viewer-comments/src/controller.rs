//! At most one connection task; replace desired state, never queue channel changes.
use crate::{
    Comment,
    connection::{Connection, Endpoints, State, Stopping},
    retry::Operation,
    service::{Blocked, Client, HttpError},
};
use std::time::Instant;
use tokio::runtime::Handle;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Comment task failed to stop: {0}")]
    Task(#[source] tokio::task::JoinError),
}

pub const MAX_POLL_COMMENTS: usize = 64;

#[derive(Clone, Debug)]
pub enum Status {
    Disabled,
    Switching,
    Connection(State),
    Retrying(State),
}

#[derive(Default)]
enum Phase {
    #[default]
    Idle,
    Running(Connection),
    Stopping {
        task: Stopping,
        retry_at: Option<Instant>,
    },
    Waiting(Instant),
    Blocked,
}

pub struct Controller {
    desired: Option<Endpoints>,
    phase: Phase,
    status: Status,
    generation: u64,
    dropped: u64,
}

impl Default for Controller {
    fn default() -> Self {
        Self {
            desired: None,
            phase: Phase::Idle,
            status: Status::Disabled,
            generation: 0,
            dropped: 0,
        }
    }
}

impl Controller {
    pub fn reception_epoch(&self) -> Option<u64> {
        match &self.phase {
            Phase::Running(connection)
                if matches!(self.status, Status::Connection(State::Receiving))
                    && matches!(connection.state(), State::Receiving)
                    && connection.dropped() == self.dropped =>
            {
                Some(self.generation)
            }
            Phase::Idle
            | Phase::Waiting(_)
            | Phase::Stopping { .. }
            | Phase::Running(_)
            | Phase::Blocked => None,
        }
    }
    pub fn status(&self) -> &Status {
        &self.status
    }

    pub fn is_stopped(&self) -> bool {
        self.desired.is_none() && matches!(self.phase, Phase::Idle)
    }

    /// Caller clears its displayed history when changing the selected broadcast,
    /// even if two broadcast services map to the same commentary endpoint.
    pub fn configure(&mut self, desired: Option<Endpoints>) {
        if self.desired == desired {
            return;
        }
        self.desired = desired;
        self.phase = match std::mem::take(&mut self.phase) {
            Phase::Running(connection) => Phase::Stopping {
                task: connection.stop(),
                retry_at: None,
            },
            Phase::Stopping { task, .. } => Phase::Stopping {
                task,
                retry_at: None,
            },
            Phase::Idle | Phase::Waiting(_) | Phase::Blocked => Phase::Idle,
        };
        self.status = if self.desired.is_none() {
            Status::Disabled
        } else {
            Status::Switching
        };
    }

    /// Nonblocking GUI-side poll. Empty polls allocate no comment buffer. The
    /// caller supplies monotonic time, allowing retry tests without real sleeps.
    pub fn poll(
        &mut self,
        runtime: &Handle,
        client: &Client,
        now: Instant,
    ) -> Result<Vec<Comment>, Error> {
        self.phase = match std::mem::take(&mut self.phase) {
            Phase::Stopping { mut task, retry_at } => match task.try_finish() {
                None => Phase::Stopping { task, retry_at },
                Some(Ok(())) => retry_at.map_or(Phase::Idle, Phase::Waiting),
                Some(Err(error)) => {
                    // The failed generation is fully joined. Report once and
                    // preserve bounded retry instead of restarting in this poll.
                    self.phase = if self.desired.is_some() {
                        Phase::Waiting(now + crate::retry::LIVE_DELAY)
                    } else {
                        Phase::Idle
                    };
                    return Err(Error::Task(error));
                }
            },
            phase => phase,
        };
        if let Phase::Waiting(deadline) = self.phase
            && now >= deadline
        {
            self.phase = Phase::Idle;
        }
        if matches!(self.phase, Phase::Idle)
            && let Some(endpoints) = &self.desired
        {
            match client.connect(runtime, endpoints.clone(), now) {
                Ok(connection) => {
                    self.generation = self.generation.wrapping_add(1);
                    self.dropped = 0;
                    self.phase = Phase::Running(connection);
                }
                Err(blocked) => {
                    tracing::warn!(
                        error = &blocked as &dyn std::error::Error,
                        "Comment connection could not start"
                    );
                    let state = State::Failed(std::sync::Arc::new(crate::connection::Error::Http(
                        HttpError::Blocked(blocked.clone()),
                    )));
                    match blocked {
                        Blocked::Waiting(until) => {
                            self.phase = Phase::Waiting(until);
                            self.status = Status::Retrying(state);
                        }
                        Blocked::Stopped(_) => {
                            self.phase = Phase::Blocked;
                            self.status = Status::Connection(state);
                        }
                    }
                }
            }
        }
        let Phase::Running(connection) = &self.phase else {
            return Ok(Vec::new());
        };
        // Observe terminal state first: if it arrives during draining, defer
        // teardown to the next poll so a concurrently queued last chat survives.
        let state = connection.state();
        let dropped = connection.dropped();
        if dropped != self.dropped {
            // Queue loss is a reception gap even when the socket stayed open.
            self.generation = self.generation.wrapping_add(1);
            self.dropped = dropped;
        }
        let comments: Vec<_> = std::iter::from_fn(|| connection.try_next())
            .take(MAX_POLL_COMMENTS)
            .collect();
        self.status = Status::Connection(state.clone());
        // On ordinary disconnect, deliver the final bounded queue before retry.
        // On configure, stop() has already discarded the old queue immediately.
        if matches!(state, State::Ended(_) | State::Failed(_)) && comments.len() < MAX_POLL_COMMENTS
        {
            match &state {
                State::Failed(error) => {
                    tracing::error!(
                        error = error.as_ref() as &dyn std::error::Error,
                        "Comment reception failed"
                    );
                }
                State::Ended(reason) => {
                    tracing::warn!(%reason, "Comment connection ended");
                }
                State::Connecting | State::Receiving => {}
            }
            let endpoints = self.desired.as_ref().expect("running connection target");
            if matches!(&state, State::Failed(error) if matches!(&**error, crate::connection::Error::WorkerStopped))
            {
                client.worker_failed(&endpoints.threads, now);
            }
            let blocked = client.check(Operation::Live, &endpoints.threads, now);
            let retry_at = match &blocked {
                Err(Blocked::Waiting(until)) => Some(*until),
                Err(Blocked::Stopped(_)) | Ok(()) => None,
            };
            if let Phase::Running(connection) = std::mem::take(&mut self.phase) {
                self.phase = Phase::Stopping {
                    task: connection.stop(),
                    retry_at,
                };
            }
            self.status = if matches!(blocked, Err(Blocked::Stopped(_))) {
                Status::Connection(state)
            } else {
                Status::Retrying(state)
            };
        }
        Ok(comments)
    }
}

#[cfg(test)]
mod tests;
