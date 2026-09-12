//! An explicit, bounded posting attempt. No background connection or automatic retry.
mod echo;
mod protocol;
mod transport;

use futures_util::FutureExt;
use std::{
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};
use tokio::{runtime::Handle, task::JoinHandle};

pub use protocol::{Error, validate_text};

const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(10);
const POST_INTERVAL: Duration = Duration::from_secs(1);
const SUCCESS_NOTICE_DURATION: Duration = Duration::from_secs(3);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    /// Changing broadcasts invalidates an attempt even when they share a JK channel.
    pub service_id: u64,
    pub watch_url: String,
}

#[derive(Debug, Default)]
pub enum Status {
    #[default]
    Idle,
    Sending,
    Sent,
    Failed(Error),
    /// Bytes may have reached the service. Never automatically resend this text.
    Unknown(Error),
}

#[derive(Debug)]
enum Outcome {
    Sent,
    Failed(Error),
    Unknown(Error),
}

struct Attempt(JoinHandle<Outcome>);
impl Attempt {
    fn poll(&mut self) -> Option<Result<Outcome, tokio::task::JoinError>> {
        match self.0.poll_unpin(&mut Context::from_waker(Waker::noop())) {
            Poll::Pending => None,
            Poll::Ready(result) => Some(result),
        }
    }
}
impl Drop for Attempt {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[derive(Default)]
enum Phase {
    #[default]
    Idle,
    Running(Attempt),
    Stopping(Attempt),
}

#[derive(Default)]
pub struct Controller {
    target: Option<Target>,
    phase: Phase,
    status: Status,
    next_post: Option<Instant>,
    success_until: Option<Instant>,
    echoes: echo::Echoes,
}

impl Controller {
    pub fn configure(&mut self, target: Option<Target>) {
        if self.target == target {
            return;
        }
        self.target = target;
        self.phase = match std::mem::take(&mut self.phase) {
            Phase::Running(attempt) | Phase::Stopping(attempt) => {
                attempt.0.abort();
                Phase::Stopping(attempt)
            }
            Phase::Idle => Phase::Idle,
        };
        self.status = Status::Idle;
        self.success_until = None;
        self.echoes = echo::Echoes::default();
    }

    pub fn status(&self) -> &Status {
        &self.status
    }
    pub fn is_own_comment(&mut self, comment: &crate::Comment, now: Instant) -> bool {
        self.echoes.take_match(comment, now)
    }
    pub fn busy(&self) -> bool {
        !matches!(self.phase, Phase::Idle)
    }
    pub fn available(&self, now: Instant) -> bool {
        self.target.is_some() && !self.busy() && self.next_post.is_none_or(|at| now >= at)
    }

    /// Refuse overlapping requests; there is no queue to replay after reconnect/selection.
    pub fn submit(&mut self, runtime: &Handle, text: &str, now: Instant) -> bool {
        if !self.available(now) {
            return false;
        }
        self.success_until = None;
        if let Err(error) = validate_text(text) {
            self.status = Status::Failed(error);
            return false;
        }
        let url = self
            .target
            .as_ref()
            .expect("available target")
            .watch_url
            .clone();
        let text = text.to_owned();
        self.phase = Phase::Running(Attempt(runtime.spawn(transport::post(
            url,
            text,
            ATTEMPT_TIMEOUT,
            self.echoes.start(now),
        ))));
        self.status = Status::Sending;
        true
    }

    /// Return true exactly once for an acknowledged post in the current target.
    pub fn poll(&mut self, now: Instant) -> bool {
        self.echoes.collect(now);
        if self.success_until.is_some_and(|until| now >= until) {
            self.success_until = None;
            self.status = Status::Idle;
        }
        let (result, cancelled) = match &mut self.phase {
            Phase::Idle => return false,
            Phase::Running(attempt) => (attempt.poll(), false),
            Phase::Stopping(attempt) => (attempt.poll(), true),
        };
        let Some(result) = result else {
            return false;
        };
        self.phase = Phase::Idle; // The completed JoinHandle is never polled again.
        // Keep this limit across channel changes as well as reconnects.
        self.next_post = now.checked_add(POST_INTERVAL);
        if cancelled {
            return false;
        }
        self.status = match result {
            Ok(Outcome::Sent) => Status::Sent,
            Ok(Outcome::Failed(error)) => Status::Failed(error),
            Ok(Outcome::Unknown(error)) => Status::Unknown(error),
            Err(error) => Status::Unknown(Error::Worker(error)),
        };
        let sent = matches!(self.status, Status::Sent);
        self.success_until = sent.then(|| now + SUCCESS_NOTICE_DURATION);
        sent
    }
}

#[cfg(test)]
mod tests;
