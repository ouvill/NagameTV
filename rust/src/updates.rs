//! Stable-release checks and daily scheduling, independent of Qt and playback.
mod history;
use crate::services::{FetchError, Job, Network, NetworkError, Progress, Stopping};
use semver::Version;
use serde::Deserialize;
use std::time::{Duration, SystemTime};

const ENDPOINT: &str = "https://api.github.com/repos/ouvill/NagameTV/releases/latest";
const RELEASES: &str = "https://github.com/ouvill/NagameTV/releases/tag/";
const MAX_RESPONSE_BYTES: usize = 256 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Could not read release information: {0}")]
    Json(#[from] crate::json::Error),
    #[error("Invalid release version: {0}")]
    Version(#[from] semver::Error),
    #[error("Network runtime is unavailable")]
    Unavailable,
    #[error("Unexpected release response status: {0}")]
    Status(reqwest::StatusCode),
}

#[derive(Debug, thiserror::Error)]
#[error("The desktop could not open the release page")]
pub struct OpenError;

#[derive(Debug, PartialEq, Eq)]
pub struct Release {
    version: Version,
    url: String,
}
impl Release {
    pub fn version(&self) -> &Version {
        &self.version
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Available(Release),
    UpToDate,
    NoRelease,
}

#[derive(Default)]
enum State {
    #[default]
    Idle,
    Checking(Job<Outcome, Error>),
    Checked(Outcome),
    Failed(FetchError<Error>),
    Stopping(Stopping),
    Stopped,
}

pub enum Status<'a> {
    Idle,
    Checking,
    Checked(&'a Outcome),
    Failed(&'a FetchError<Error>),
}

#[derive(Default)]
pub struct Checker {
    state: State,
    open_error: Option<OpenError>,
    history: history::History,
    automatic: Automatic,
}

#[derive(Default)]
enum Automatic {
    #[default]
    Unavailable,
    Disabled,
    Enabled,
}

#[derive(Clone, Copy)]
enum Trigger {
    Manual,
    Automatic,
}

impl Checker {
    pub fn load(allowed: bool, enabled: bool) -> Self {
        Self {
            history: history::History::open(history::path()),
            automatic: if !allowed {
                Automatic::Unavailable
            } else if enabled {
                Automatic::Enabled
            } else {
                Automatic::Disabled
            },
            ..Self::default()
        }
    }

    pub fn automatic_allowed(&self) -> bool {
        !matches!(self.automatic, Automatic::Unavailable)
    }
    pub fn automatic_enabled(&self) -> bool {
        matches!(self.automatic, Automatic::Enabled)
    }
    pub fn configure_automatic(&mut self, enabled: bool) -> bool {
        if !self.automatic_allowed() {
            return false;
        }
        self.automatic = if enabled {
            Automatic::Enabled
        } else {
            Automatic::Disabled
        };
        true
    }
    pub fn last_success(&self) -> Option<u64> {
        self.history.last_success()
    }
    pub fn history_error(&self) -> Option<&history::Error> {
        self.history.error()
    }

    pub fn tick(&mut self, network: Option<&Network>, now: SystemTime) -> bool {
        self.tick_endpoint(network, now, ENDPOINT)
    }

    fn tick_endpoint(
        &mut self,
        network: Option<&Network>,
        now: SystemTime,
        endpoint: &str,
    ) -> bool {
        let changed = self.poll_at(now);
        if self.automatic_enabled() && self.history.due(now) {
            return self.start_at(
                network,
                endpoint.into(),
                crate::build_info::INFO.version,
                now,
                Trigger::Automatic,
            ) || changed;
        }
        changed
    }
    pub fn status(&self) -> Status<'_> {
        match &self.state {
            State::Idle | State::Stopping(_) | State::Stopped => Status::Idle,
            State::Checking(_) => Status::Checking,
            State::Checked(outcome) => Status::Checked(outcome),
            State::Failed(error) => Status::Failed(error),
        }
    }

    pub fn check(&mut self, network: Option<&Network>) -> bool {
        self.start(network, ENDPOINT.into(), crate::build_info::INFO.version)
    }

    #[cfg(feature = "native_tests")]
    pub(crate) fn check_fixture(&mut self, network: Option<&Network>, endpoint: String) -> bool {
        self.start(network, endpoint, crate::build_info::INFO.version)
    }

    fn start(&mut self, network: Option<&Network>, endpoint: String, current: &str) -> bool {
        self.start_at(
            network,
            endpoint,
            current,
            SystemTime::now(),
            Trigger::Manual,
        )
    }

    fn start_at(
        &mut self,
        network: Option<&Network>,
        endpoint: String,
        current: &str,
        now: SystemTime,
        trigger: Trigger,
    ) -> bool {
        match &self.state {
            State::Checking(_) | State::Stopping(_) | State::Stopped => return false,
            State::Idle | State::Checked(_) | State::Failed(_) => {}
        }
        self.open_error = None;
        self.history.attempt(now);
        if matches!(trigger, Trigger::Automatic) && self.history.error().is_some() {
            return true;
        }
        self.state = match network {
            Some(network) => {
                let current = current.to_owned();
                State::Checking(network.job(async move { fetch(&endpoint, &current).await }))
            }
            None => Self::failed(FetchError::Parse(Error::Unavailable)),
        };
        true
    }

    fn failed(error: FetchError<Error>) -> State {
        tracing::error!(
            error = &error as &dyn std::error::Error,
            "Update check failed"
        );
        State::Failed(error)
    }

    #[cfg(test)]
    pub fn poll(&mut self) -> bool {
        self.poll_at(SystemTime::now())
    }

    fn poll_at(&mut self, now: SystemTime) -> bool {
        let mut changed = false;
        self.state = match std::mem::take(&mut self.state) {
            State::Checking(job) => match job.poll() {
                Progress::Pending(job) => State::Checking(job),
                Progress::Complete(result) => {
                    changed = true;
                    match result {
                        Ok(outcome) => {
                            self.history.succeeded(now);
                            State::Checked(outcome)
                        }
                        Err(error) => Self::failed(error),
                    }
                }
            },
            State::Stopping(job) => match job.poll() {
                Progress::Pending(job) => State::Stopping(job),
                Progress::Complete(()) => State::Stopped,
            },
            state @ (State::Idle | State::Checked(_) | State::Failed(_) | State::Stopped) => state,
        };
        changed
    }

    pub fn stop(&mut self) {
        self.open_error = None;
        self.state = match std::mem::replace(&mut self.state, State::Stopped) {
            State::Checking(job) => State::Stopping(job.cancel()),
            State::Stopping(job) => State::Stopping(job),
            State::Idle | State::Checked(_) | State::Failed(_) | State::Stopped => State::Stopped,
        };
    }

    pub fn open_release(&mut self, open: impl FnOnce(&str) -> bool) -> bool {
        let State::Checked(Outcome::Available(release)) = &self.state else {
            return false;
        };
        let opened = open(&release.url);
        self.open_error = if opened {
            None
        } else {
            let error = OpenError;
            tracing::error!(
                error = &error as &dyn std::error::Error,
                "Opening release page failed"
            );
            Some(error)
        };
        opened
    }

    pub fn open_error(&self) -> Option<&OpenError> {
        self.open_error.as_ref()
    }
}

#[derive(Deserialize)]
struct Response {
    tag_name: String,
    prerelease: bool,
    draft: bool,
}

fn parse(bytes: &[u8], current: &str) -> Result<Outcome, Error> {
    let release: Response = crate::json::from_slice(bytes)?;
    if release.prerelease || release.draft {
        return Ok(Outcome::NoRelease);
    }
    let version = Version::parse(
        release
            .tag_name
            .strip_prefix('v')
            .unwrap_or(&release.tag_name),
    )?;
    if !version.pre.is_empty() {
        return Ok(Outcome::NoRelease);
    }
    let current = Version::parse(current)?;
    if version.cmp_precedence(&current).is_gt() {
        Ok(Outcome::Available(Release {
            version,
            // Only a parsed version tag can become a URL; API-provided links are not opened.
            url: format!("{RELEASES}{}", release.tag_name),
        }))
    } else {
        Ok(Outcome::UpToDate)
    }
}

async fn fetch(endpoint: &str, current: &str) -> Result<Outcome, FetchError<Error>> {
    let client = reqwest::Client::builder()
        .user_agent(concat!("NagameTV/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(NetworkError::from)?;
    let mut response = client
        .get(endpoint)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2026-03-10")
        .send()
        .await
        .map_err(NetworkError::from)?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(Outcome::NoRelease);
    }
    response = response.error_for_status().map_err(NetworkError::from)?;
    if response.status() != reqwest::StatusCode::OK {
        return Err(FetchError::Parse(Error::Status(response.status())));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(NetworkError::from)? {
        if bytes.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(NetworkError::ResponseTooLarge {
                limit: MAX_RESPONSE_BYTES,
            }
            .into());
        }
        bytes.extend_from_slice(&chunk);
    }
    parse(&bytes, current).map_err(FetchError::Parse)
}

#[cfg(test)]
mod tests;
