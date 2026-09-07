//! At most one connection task; replace desired state, never queue channel changes.
use crate::{
    Comment,
    connection::{Connection, Endpoints, State, Stopping},
};
use std::time::{Duration, Instant};
use tokio::runtime::Handle;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Comment task failed to stop: {0}")]
    Task(#[source] tokio::task::JoinError),
}

pub const MAX_POLL_COMMENTS: usize = 64;
const RETRY_DELAY: Duration = Duration::from_secs(5);

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
}

pub struct Controller {
    desired: Option<Endpoints>,
    phase: Phase,
    status: Status,
}

impl Default for Controller {
    fn default() -> Self {
        Self {
            desired: None,
            phase: Phase::Idle,
            status: Status::Disabled,
        }
    }
}

impl Controller {
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
            _ => Phase::Idle,
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
        client: &reqwest::Client,
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
                        now.checked_add(RETRY_DELAY)
                            .map_or(Phase::Idle, Phase::Waiting)
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
            self.phase = Phase::Running(Connection::start(
                runtime,
                client.clone(),
                endpoints.clone(),
            ));
        }
        let Phase::Running(connection) = &self.phase else {
            return Ok(Vec::new());
        };
        // Observe terminal state first: if it arrives during draining, defer
        // teardown to the next poll so a concurrently queued last chat survives.
        let state = connection.state();
        let comments: Vec<_> = std::iter::from_fn(|| connection.try_next())
            .take(MAX_POLL_COMMENTS)
            .collect();
        self.status = Status::Connection(state.clone());
        // On ordinary disconnect, deliver the final bounded queue before retry.
        // On configure, stop() has already discarded the old queue immediately.
        if matches!(state, State::Ended | State::Failed(_)) && comments.len() < MAX_POLL_COMMENTS {
            if let Phase::Running(connection) = std::mem::take(&mut self.phase) {
                self.phase = Phase::Stopping {
                    task: connection.stop(),
                    retry_at: now.checked_add(RETRY_DELAY),
                };
            }
            self.status = Status::Retrying(state);
        }
        Ok(comments)
    }
}

#[cfg(test)]
mod tests;
