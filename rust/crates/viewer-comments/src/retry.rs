//! NX-only retry policy. A lane's success cannot clear another lane's failures
//! or the provider's Retry-After deadline.
use std::{
    collections::HashMap,
    hash::BuildHasher,
    time::{Duration, Instant},
};

pub(crate) const LIVE_DELAY: Duration = Duration::from_secs(5);
pub(crate) const ACTIVITY_INTERVAL: Duration = Duration::from_secs(60);
pub(crate) const STABLE_CONNECTION: Duration = Duration::from_secs(60);
const RATE_LIMIT_DELAY: Duration = Duration::from_secs(60);
const MAX_DELAY: Duration = Duration::from_secs(300);
const JITTER_MILLIS: u64 = 30_000;
const MAX_BACKOFF_EXPONENT: u32 = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Operation {
    Live,
    Activity,
    Posting,
}

#[derive(Clone, Debug, thiserror::Error)]
pub enum Blocked {
    #[error("NX-Jikkyo is waiting before its next request")]
    Waiting(Instant),
    #[error("NX-Jikkyo automatic requests stopped: {0}")]
    Stopped(String),
}

pub(crate) enum Failure {
    Temporary,
    Unavailable,
    RateLimited,
    Permanent,
    Http {
        status: reqwest::StatusCode,
        retry_after: Option<Duration>,
    },
}

#[derive(Default)]
struct Lane {
    failures: u32,
    until: Option<Instant>,
}

#[derive(Default)]
pub(crate) struct Policy {
    live: Lane,
    activity: Lane,
    posting: Lane,
    provider_until: Option<Instant>,
    stopped: HashMap<(Operation, String), String>,
}

impl Policy {
    fn lane(&self, operation: Operation) -> &Lane {
        match operation {
            Operation::Live => &self.live,
            Operation::Activity => &self.activity,
            Operation::Posting => &self.posting,
        }
    }
    fn lane_mut(&mut self, operation: Operation) -> &mut Lane {
        match operation {
            Operation::Live => &mut self.live,
            Operation::Activity => &mut self.activity,
            Operation::Posting => &mut self.posting,
        }
    }
    pub fn check(&self, operation: Operation, endpoint: &str, now: Instant) -> Result<(), Blocked> {
        if let Some(message) = self.stopped.get(&(operation, endpoint.to_owned())) {
            return Err(Blocked::Stopped(message.clone()));
        }
        if let Some(until) = self
            .lane(operation)
            .until
            .into_iter()
            .chain(self.provider_until)
            .max()
            .filter(|until| *until > now)
        {
            return Err(Blocked::Waiting(until));
        }
        Ok(())
    }
    pub fn failed(
        &mut self,
        operation: Operation,
        endpoint: &str,
        failure: Failure,
        message: String,
        now: Instant,
    ) {
        let jitter = Duration::from_millis(
            std::collections::hash_map::RandomState::new().hash_one((now, operation))
                % (JITTER_MILLIS + 1),
        );
        self.failed_with_jitter(operation, endpoint, failure, message, now, jitter);
    }
    fn failed_with_jitter(
        &mut self,
        operation: Operation,
        endpoint: &str,
        failure: Failure,
        message: String,
        now: Instant,
        jitter: Duration,
    ) {
        let (rate_limited, server_delay, shared) = match failure {
            Failure::Temporary => (false, None, false),
            Failure::Unavailable => (false, None, true),
            Failure::RateLimited => (true, None, true),
            Failure::Permanent => {
                self.stopped
                    .insert((operation, endpoint.to_owned()), message);
                return;
            }
            Failure::Http {
                status,
                retry_after,
            } => {
                if !(status.is_server_error()
                    || status == reqwest::StatusCode::TOO_MANY_REQUESTS
                    || status == reqwest::StatusCode::REQUEST_TIMEOUT)
                {
                    self.stopped
                        .insert((operation, endpoint.to_owned()), message);
                    return;
                }
                (
                    status == reqwest::StatusCode::TOO_MANY_REQUESTS,
                    retry_after,
                    status == reqwest::StatusCode::TOO_MANY_REQUESTS || retry_after.is_some(),
                )
            }
        };
        let lane = self.lane_mut(operation);
        lane.failures = lane.failures.saturating_add(1);
        let initial = if rate_limited {
            RATE_LIMIT_DELAY
        } else {
            match operation {
                Operation::Live | Operation::Posting => LIVE_DELAY,
                Operation::Activity => ACTIVITY_INTERVAL,
            }
        };
        let delay = initial
            .saturating_mul(1_u32 << (lane.failures - 1).min(MAX_BACKOFF_EXPONENT))
            .min(MAX_DELAY);
        // Add jitter AFTER all minimum deadlines, including a long Retry-After.
        let delay = delay
            .max(server_delay.unwrap_or_default())
            .saturating_add(jitter);
        let Some(until) = now.checked_add(delay) else {
            self.stopped.insert(
                (operation, endpoint.to_owned()),
                "Retry-After exceeds the local clock range".into(),
            );
            return;
        };
        lane.until = Some(lane.until.map_or(until, |old| old.max(until)));
        if shared {
            self.provider_until = Some(self.provider_until.map_or(until, |old| old.max(until)));
        }
    }
    pub fn stable_live(&mut self) {
        // Do not shorten an outstanding delay (possibly set by a different request).
        self.live.failures = 0;
    }
    pub fn activity_succeeded(&mut self, now: Instant) {
        self.activity.failures = 0;
        let until = now + ACTIVITY_INTERVAL;
        self.activity.until = Some(self.activity.until.map_or(until, |old| old.max(until)));
    }
    pub fn posting_succeeded(&mut self) {
        self.posting.failures = 0;
    }
}

#[cfg(test)]
mod tests;
