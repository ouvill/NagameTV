//! Viewing policy and exclusive ownership of a desktop inhibition request.
//! This module has no Qt, D-Bus or presentation dependencies.
pub mod routes;
use std::time::{Duration, Instant};

pub const INTERRUPTION_GRACE: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Viewing {
    Playing,
    Interrupted,
    Inactive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Disabled,
    Idle,
    Acquiring,
    Inhibited,
    Unavailable,
    Failed,
}
impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Idle => "idle",
            Self::Acquiring => "acquiring",
            Self::Inhibited => "inhibited",
            Self::Unavailable => "unavailable",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{operation}: {cause}")]
pub struct Error {
    pub operation: &'static str,
    pub unavailable: bool,
    #[source]
    pub cause: Box<dyn std::error::Error>,
}

pub enum Progress {
    Pending,
    Held,
}

/// A request is neither Clone nor Copy. Dropping it cancels acquisition or
/// releases its grant, including after a late successful response.
pub trait Request {
    fn poll(&mut self) -> Result<Progress, Error>;
    fn release(self) -> Result<(), Error>;
}
pub trait Inhibitor {
    type Request: Request;
    fn acquire(&mut self) -> Result<Self::Request, Error>;
    fn revision(&mut self) -> u64 {
        0
    }
    fn reset(&mut self) {}
}

enum State<R> {
    Disabled,
    Idle,
    Acquiring(R),
    Held {
        request: R,
        interrupted: Option<Instant>,
    },
    Fault(Error),
}

pub struct Controller<B: Inhibitor> {
    backend: B,
    state: State<B::Request>,
    revision: u64,
}
impl<B: Inhibitor> Controller<B> {
    pub fn new(backend: B, enabled: bool) -> Self {
        Self {
            backend,
            revision: 0,
            state: if enabled {
                State::Idle
            } else {
                State::Disabled
            },
        }
    }
    pub fn status(&self) -> Status {
        match &self.state {
            State::Disabled => Status::Disabled,
            State::Idle => Status::Idle,
            State::Acquiring(_) => Status::Acquiring,
            State::Held { .. } => Status::Inhibited,
            State::Fault(error) => {
                if error.unavailable {
                    Status::Unavailable
                } else {
                    Status::Failed
                }
            }
        }
    }
    fn fault(error: Error) -> State<B::Request> {
        if error.unavailable {
            tracing::warn!(
                operation = error.operation,
                error = &error as &dyn std::error::Error,
                "Sleep inhibition unavailable until the environment changes"
            );
        } else {
            tracing::error!(
                operation = error.operation,
                error = &error as &dyn std::error::Error,
                "Sleep inhibition failed"
            );
        }
        State::Fault(error)
    }
    fn release(state: State<B::Request>, next: State<B::Request>) -> State<B::Request> {
        match state {
            State::Acquiring(request) | State::Held { request, .. } => match request.release() {
                Ok(()) => next,
                Err(error) => Self::fault(error),
            },
            State::Disabled | State::Idle | State::Fault(_) => next,
        }
    }
    pub fn reset_environment(&mut self) {
        self.backend.reset();
        if matches!(self.state, State::Fault(_)) {
            self.state = State::Idle;
        }
    }
    /// Unavailable environments survive viewing changes; transient failures do not.
    pub fn update(&mut self, enabled: bool, viewing: Viewing, now: Instant) {
        let revision = self.backend.revision();
        if revision != self.revision {
            self.revision = revision;
            if matches!(&self.state, State::Fault(error) if error.unavailable) {
                self.state = State::Idle;
            }
        }
        let state = std::mem::replace(&mut self.state, State::Idle);
        if enabled
            && viewing == Viewing::Inactive
            && matches!(&state, State::Fault(error) if error.unavailable)
        {
            self.state = state;
            return;
        }
        if !enabled || viewing == Viewing::Inactive {
            self.state = Self::release(
                state,
                if enabled {
                    State::Idle
                } else {
                    State::Disabled
                },
            );
            return;
        }
        self.state = match state {
            State::Disabled | State::Idle => {
                if viewing == Viewing::Playing {
                    match self.backend.acquire() {
                        Ok(request) => State::Acquiring(request),
                        Err(error) => Self::fault(error),
                    }
                } else {
                    State::Idle
                }
            }
            State::Acquiring(mut request) => {
                if viewing == Viewing::Interrupted {
                    Self::release(State::Acquiring(request), State::Idle)
                } else {
                    match request.poll() {
                        Ok(Progress::Pending) => State::Acquiring(request),
                        Ok(Progress::Held) => State::Held {
                            request,
                            interrupted: None,
                        },
                        Err(error) => Self::fault(error),
                    }
                }
            }
            State::Held {
                mut request,
                interrupted,
            } => {
                let interrupted = if viewing == Viewing::Interrupted {
                    Some(interrupted.unwrap_or(now))
                } else {
                    None
                };
                if interrupted
                    .is_some_and(|start| now.saturating_duration_since(start) >= INTERRUPTION_GRACE)
                {
                    Self::release(
                        State::Held {
                            request,
                            interrupted,
                        },
                        State::Idle,
                    )
                } else {
                    match request.poll() {
                        Ok(Progress::Held) => State::Held {
                            request,
                            interrupted,
                        },
                        Ok(Progress::Pending) => {
                            unreachable!("held requests cannot become pending")
                        }
                        Err(error) => Self::fault(error),
                    }
                }
            }
            State::Fault(error) => State::Fault(error),
        };
    }
    pub fn stop(&mut self, enabled: bool) {
        let state = std::mem::replace(&mut self.state, State::Idle);
        if enabled && matches!(&state, State::Fault(error) if error.unavailable) {
            self.state = state;
            return;
        }
        self.state = Self::release(
            state,
            if enabled {
                State::Idle
            } else {
                State::Disabled
            },
        );
    }
}

#[cfg(test)]
mod tests;
