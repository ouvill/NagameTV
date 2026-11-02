//! EPG acquisition lives independently of playback and of the guide's visibility.
use crate::channels::BroadcastService;
use crate::services::{FetchError, Job, Network, Progress, Stopping};
pub mod browser;
mod details;
mod genre;
pub mod guide;
pub mod model;
pub mod presentation;
pub mod schedule;
pub mod view;
mod visibility;
pub mod watch;
mod wire;
use model::{Snapshot, parse};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
use viewer_epg_events::{Changes, MAX_PENDING_SERVICES};

const MAX_RESPONSE: usize = 32 * 1024 * 1024;
const MAX_PROGRAMS: usize = 50_000;
const MAX_SNAPSHOT_BYTES: usize = 128 * 1024 * 1024;
const RETRY_INITIAL: Duration = Duration::from_secs(5);
const RETRY_MAX: Duration = Duration::from_secs(60);
const REFRESH: Duration = Duration::from_secs(300);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("番組情報の変換失敗: {0}")]
    Projection(#[source] serde_json::Error),
    #[error("番組JSONの解析失敗: {0}")]
    Json(#[from] crate::json::Error),
    #[error("番組データのメモリー上限を超えています（{actual} > {limit} bytes）")]
    TooMuchMemory { actual: usize, limit: usize },
    #[error("番組イベントの識別子が本文と一致しません")]
    WrongProgramId,
    #[error("サービス別番組応答に別のサービスが含まれています")]
    WrongService,
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
    Failed {
        error: RequestError,
        retry_at: Instant,
    },
}

#[derive(Clone, Copy)]
enum Scope {
    Full,
    Service {
        endpoint: u64,
        broadcast: BroadcastService,
    },
}
struct Fetching {
    job: Request,
    scope: Scope,
    next: Option<Instant>,
}

// Each active state owns exactly one job. Cancelling retains ownership until
// the worker finishes; its result can never be accepted as a fresh snapshot.
enum Acquisition {
    Idle {
        next: Option<Instant>,
        outcome: Outcome,
    },
    Fetching(Fetching),
    Cancelling(Stopping),
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

/// Transitions during one poll, including completion followed by immediate reacquisition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Completion {
    Succeeded,
    Failed,
}
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Update {
    pub completed: Option<Completion>,
    pub started: bool,
}

#[derive(Default)]
pub struct ProgramInfo {
    desired: Option<String>,
    generation: u64,
    acquisition: Acquisition,
    // Preserve one follow-up request even when notifications arrive during a fetch.
    refresh_pending: bool,
    deferred: Changes,
    services: BTreeMap<u64, BroadcastService>,
    failures: u32,
    snapshot: Snapshot,
    pub revision: u64,
    pub text_capacity_bytes: usize,
}
impl ProgramInfo {
    /// Share validated candidates with the guide's interval projection.
    pub fn guide_view(
        &self,
        channels: std::sync::Arc<[crate::channels::Channel]>,
        window: guide::DayWindow,
    ) -> Result<view::View, serde_json::Error> {
        view::View::new(self.snapshot.clone(), channels, window, self.generation)
    }
    /// Borrow unambiguous current metadata without parsing QML JSON.
    pub fn current(&self, service: Option<BroadcastService>, now: u64) -> Option<&model::Program> {
        self.snapshot.current(service, now)
    }

    /// Invalidate before cancellation. Never reuse results, even for A -> B -> A.
    pub fn configure(&mut self, server: Option<String>) {
        if self.desired == server {
            return;
        }
        self.desired = server;
        self.generation = self.generation.wrapping_add(1);
        self.refresh_pending = false;
        self.deferred = Changes::default();
        self.services.clear();
        self.failures = 0;
        self.snapshot = Snapshot::default();
        self.text_capacity_bytes = 0;
        self.revision += 1;
        self.acquisition = match std::mem::take(&mut self.acquisition) {
            Acquisition::Fetching(fetch) => Acquisition::Cancelling(fetch.job.cancel()),
            Acquisition::Cancelling(job) => Acquisition::Cancelling(job),
            Acquisition::Idle { .. } => Acquisition::default(),
        };
    }
    pub fn refresh(&mut self) {
        self.refresh_pending = true;
    }
    /// Returns whether an unknown service also requires catalog reconciliation.
    pub fn events(&mut self, changes: Changes, channels: &[crate::channels::Channel]) -> bool {
        let delta = match changes {
            Changes::Full => {
                if self.desired.is_none() {
                    return false;
                }
                self.refresh();
                self.deferred = Changes::default();
                self.services.clear();
                return false;
            }
            Changes::Delta(delta) => delta,
        };
        let (programs, services) = delta.into_parts();
        // An empty initial catalog disables acquisition, but its active event
        // subscription can still discover a newly announced service.
        if self.desired.is_none() {
            return services.iter().any(|id| {
                !channels
                    .iter()
                    .any(|channel| channel.id == *id && channel.broadcast.is_some())
            });
        }
        let mut catalog = false;
        for id in services {
            match channels
                .iter()
                .find(|channel| channel.id == id)
                .and_then(|channel| channel.broadcast)
            {
                Some(service) => {
                    self.services.insert(id, service);
                }
                None => {
                    catalog = true;
                    self.refresh();
                }
            }
        }
        for program in programs {
            self.deferred.program(program);
        }
        if self.services.len() > MAX_PENDING_SERVICES || matches!(self.deferred, Changes::Full) {
            self.refresh();
            self.deferred = Changes::default();
            self.services.clear();
        }
        if matches!(self.acquisition, Acquisition::Idle { .. }) {
            self.apply_deferred();
        }
        catalog
    }
    fn publish(&mut self, snapshot: Snapshot, force: bool) {
        if force || !self.snapshot.same_programs(&snapshot) {
            self.revision += 1;
        }
        let storage = snapshot.storage();
        tracing::debug!(
            programs = snapshot.len(),
            record_capacity_bytes = storage.records,
            string_capacity_bytes = storage.strings,
            audio_heap_bytes = storage.audio,
            detail_record_bytes = storage.details,
            index_bytes = storage.index,
            source_bytes = storage.source,
            snapshot_capacity_bytes = storage.total(),
            "EPG_MEMORY"
        );
        tracing::debug!(
            conflict_services = snapshot.conflict_services(),
            unknown_ends = snapshot.unknown_ends(),
            "EPG schedule validation completed"
        );
        self.text_capacity_bytes = storage.strings + snapshot.source_string_bytes();
        self.snapshot = snapshot;
    }
    fn apply_deferred(&mut self) {
        match std::mem::take(&mut self.deferred) {
            Changes::Full => self.refresh(),
            Changes::Delta(delta) => {
                let (programs, _) = delta.into_parts();
                match self.snapshot.apply(programs) {
                    Ok(Some(snapshot)) => self.publish(snapshot, false),
                    Ok(None) => {}
                    Err(error) => {
                        tracing::warn!(
                            error = &error as &dyn std::error::Error,
                            "EPG event application failed; requesting full reconciliation"
                        );
                        self.refresh();
                    }
                }
            }
        }
    }
    pub fn poll(&mut self, network: &Network) -> Update {
        self.poll_at(network, Instant::now())
    }
    fn poll_at(&mut self, network: &Network, now: Instant) -> Update {
        let mut update = Update::default();
        self.acquisition = match std::mem::take(&mut self.acquisition) {
            Acquisition::Cancelling(job) => match job.poll() {
                Progress::Pending(job) => Acquisition::Cancelling(job),
                Progress::Complete(()) => Acquisition::default(),
            },
            Acquisition::Fetching(Fetching { job, scope, next }) => match job.poll() {
                Progress::Pending(job) => Acquisition::Fetching(Fetching { job, scope, next }),
                Progress::Complete(result) => {
                    let result = result.and_then(|snapshot| match scope {
                        Scope::Full => Ok(snapshot),
                        Scope::Service { broadcast, .. } => self
                            .snapshot
                            .replace_service(broadcast, snapshot)
                            .map_err(FetchError::Parse),
                    });
                    match result {
                        Ok(snapshot) => {
                            update.completed = Some(Completion::Succeeded);
                            self.failures = 0;
                            self.publish(snapshot, matches!(scope, Scope::Full));
                            Acquisition::Idle {
                                next: if matches!(scope, Scope::Full) {
                                    Some(now + REFRESH)
                                } else {
                                    next
                                },
                                outcome: Outcome::Ready,
                            }
                        }
                        Err(error) => {
                            tracing::error!(
                                error = &error as &dyn std::error::Error,
                                "Program guide fetch failed"
                            );
                            update.completed = Some(Completion::Failed);
                            self.failures = self.failures.saturating_add(1);
                            let delay = (RETRY_INITIAL * 2_u32.pow((self.failures - 1).min(4)))
                                .min(RETRY_MAX);
                            match scope {
                                Scope::Full => self.refresh(),
                                Scope::Service {
                                    endpoint,
                                    broadcast,
                                } => {
                                    self.services.insert(endpoint, broadcast);
                                    if self.services.len() > MAX_PENDING_SERVICES {
                                        self.services.clear();
                                        self.refresh();
                                    }
                                }
                            }
                            Acquisition::Idle {
                                next,
                                outcome: Outcome::Failed {
                                    error,
                                    retry_at: now + delay,
                                },
                            }
                        }
                    }
                }
            },
            state @ Acquisition::Idle { .. } => state,
        };
        if matches!(self.acquisition, Acquisition::Idle { .. }) {
            self.apply_deferred();
        }
        if let Some(server) = &self.desired
            && let Acquisition::Idle { next, outcome } = &self.acquisition
            && !matches!(outcome, Outcome::Failed { retry_at, .. } if now < *retry_at)
        {
            let scope = if self.refresh_pending || next.is_none_or(|deadline| now >= deadline) {
                self.refresh_pending = false;
                self.services.clear();
                Some(Scope::Full)
            } else {
                self.services
                    .pop_first()
                    .map(|(endpoint, broadcast)| Scope::Service {
                        endpoint,
                        broadcast,
                    })
            };
            if let Some(scope) = scope {
                let next = *next;
                let client = network.large_json();
                let (url, fallback) = match scope {
                    Scope::Full => (format!("{server}/api/programs"), None),
                    Scope::Service {
                        endpoint,
                        broadcast,
                    } => (
                        format!("{server}/api/services/{endpoint}/programs"),
                        Some(format!(
                            "{server}/api/programs?networkId={}&serviceId={}",
                            broadcast.network_id, broadcast.service_id
                        )),
                    ),
                };
                let job = network.job(async move {
                    client
                        .fetch(url, fallback, MAX_RESPONSE, move |bytes| match scope {
                            Scope::Full => parse(bytes),
                            Scope::Service { broadcast, .. } => {
                                model::parse_service(bytes, broadcast)
                            }
                        })
                        .await
                });
                self.acquisition = Acquisition::Fetching(Fetching { job, scope, next });
                update.started = true;
            }
        }
        update
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
                outcome: Outcome::Failed { error, .. },
                ..
            } => Status::Failed(error),
        }
    }
    /// Borrow only the requested service's bounded schedule while the guide is open.
    #[cfg(test)]
    pub fn view(
        &self,
        service: Option<BroadcastService>,
        window: guide::DayWindow,
    ) -> Result<String, serde_json::Error> {
        self.snapshot.view(service, window)
    }

    /// Share the same simulcast policy with navigation and both channel views.
    pub fn visible_channels(
        &self,
        channels: &[crate::channels::Channel],
        now: Option<u64>,
    ) -> Vec<usize> {
        visibility::with_uncertain(&self.snapshot, channels, now)
    }
    /// Compute navigation on demand; do not retain browser widgets or a second EPG.
    pub fn adjacent_channel(
        &self,
        channels: &[crate::channels::Channel],
        selected: Option<usize>,
        step: crate::channels::Step,
        now: Option<u64>,
    ) -> Option<usize> {
        let visible = self.visible_channels(channels, now);
        visibility::adjacent(&visible, selected, step)
    }

    pub fn browser_presentation(
        &self,
        projection: &mut browser::Projection,
        channels: &[crate::channels::Channel],
        now: Option<u64>,
    ) -> Result<Option<String>, serde_json::Error> {
        projection.update(&self.snapshot, self.revision, channels, now)
    }
    /// Only fill missing fields of an exactly matched TS event.
    pub(crate) fn supplement_data(&self, data: &mut serde_json::Value) {
        let identity = || {
            Some((
                BroadcastService {
                    network_id: u16::try_from(data["networkId"].as_u64()?).ok()?,
                    service_id: u16::try_from(data["serviceId"].as_u64()?).ok()?,
                },
                u16::try_from(data["eventId"].as_u64()?).ok()?,
                data["startAt"].as_u64()?,
            ))
        };
        let Some((service, event, start)) = identity() else {
            return;
        };
        let Some(epg) = self.snapshot.exact(service, event, start) else {
            return;
        };
        for (key, value) in [("name", &epg.name), ("description", &epg.description)] {
            if data[key].as_str().is_none_or(str::is_empty)
                && let Some(value) = value
            {
                data[key] = value.clone().into();
                data["fieldSources"][key] = "mirakurun_epg".into();
            }
        }
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

#[cfg(test)]
mod incremental_tests;
