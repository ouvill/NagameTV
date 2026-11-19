//! Find compatible services on the host's local private IPv4 networks.
use super::{Job, Network, NetworkError, Progress, ServerProduct, Stopping, job};
use futures_util::{StreamExt, stream};
use serde::Deserialize;
use std::{convert::Infallible, net::SocketAddrV4, time::Duration};

mod networks;
#[cfg(test)]
mod tests;

const CONCURRENCY: usize = 64;
const CONNECT_TIMEOUT: Duration = Duration::from_millis(180);
const REQUEST_TIMEOUT: Duration = Duration::from_millis(450);
const SCAN_TIMEOUT: Duration = Duration::from_secs(5);
const TUNER_LIMIT: usize = 1024 * 1024;
const RECORDINGS_LIMIT: usize = 64 * 1024;
const VERSION_LIMIT: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    Tuner,
    Recordings,
}
impl Kind {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "tuner" => Some(Self::Tuner),
            "recordings" => Some(Self::Recordings),
            _ => None,
        }
    }
    pub fn code(self) -> &'static str {
        match self {
            Self::Tuner => "tuner",
            Self::Recordings => "recordings",
        }
    }
    fn port(self) -> u16 {
        match self {
            Self::Tuner => 40772,
            Self::Recordings => 8888,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Server {
    pub url: String,
    pub name: &'static str,
    pub version: Option<String>,
}

struct Results {
    servers: Vec<Server>,
    partial: bool,
}

#[derive(Default)]
enum State {
    #[default]
    Idle,
    Searching(Kind, Job<Results, Infallible>),
    Stopping(Stopping, Option<Kind>),
    Finished(Kind, Results),
    Failed(Kind),
}

#[derive(Default)]
pub struct Search(State);

impl Search {
    pub fn request(&mut self, network: Option<&Network>, kind: Kind) {
        let Some(network) = network else {
            self.0 = State::Failed(kind);
            tracing::warn!(
                kind = kind.code(),
                "LAN search unavailable without a network runtime"
            );
            return;
        };
        self.0 = match std::mem::take(&mut self.0) {
            State::Searching(active, job) if active == kind => State::Searching(active, job),
            State::Searching(active, job) => {
                tracing::info!(kind = active.code(), "LAN search superseded");
                State::Stopping(job.cancel(), Some(kind))
            }
            State::Stopping(stopping, _) => State::Stopping(stopping, Some(kind)),
            State::Idle | State::Finished(..) | State::Failed(_) => start(network, kind),
        };
    }

    pub fn cancel(&mut self, kind: Kind) {
        self.0 = match std::mem::take(&mut self.0) {
            State::Searching(active, job) if active == kind => {
                tracing::info!(kind = kind.code(), "LAN search cancelled");
                State::Stopping(job.cancel(), None)
            }
            State::Stopping(stopping, Some(next)) if next == kind => {
                State::Stopping(stopping, None)
            }
            State::Finished(active, _) | State::Failed(active) if active == kind => State::Idle,
            state => state,
        };
    }

    pub fn poll(&mut self, network: &Network) -> bool {
        let mut changed = false;
        self.0 = match std::mem::take(&mut self.0) {
            State::Searching(kind, job) => match job.poll() {
                Progress::Pending(job) => State::Searching(kind, job),
                Progress::Complete(result) => {
                    changed = true;
                    match result {
                        Ok(results) => State::Finished(kind, results),
                        Err(error) => {
                            tracing::warn!(
                                kind = kind.code(),
                                error = &error as &dyn std::error::Error,
                                "LAN search failed"
                            );
                            State::Failed(kind)
                        }
                    }
                }
            },
            State::Stopping(stopping, next) => match stopping.poll() {
                Progress::Pending(stopping) => State::Stopping(stopping, next),
                Progress::Complete(()) => next.map_or(State::Idle, |kind| start(network, kind)),
            },
            state => state,
        };
        changed
    }

    pub fn kind(&self) -> Option<Kind> {
        match self.0 {
            State::Searching(kind, _) | State::Finished(kind, _) | State::Failed(kind) => {
                Some(kind)
            }
            State::Stopping(_, next) => next,
            State::Idle => None,
        }
    }

    pub fn status(&self) -> &'static str {
        match &self.0 {
            State::Idle | State::Stopping(_, None) => "idle",
            State::Searching(..) | State::Stopping(_, Some(_)) => "searching",
            State::Finished(_, results) if results.partial => "partial",
            State::Finished(..) => "finished",
            State::Failed(_) => "failed",
        }
    }

    pub fn results(&self) -> &[Server] {
        match &self.0 {
            State::Finished(_, results) => &results.servers,
            _ => &[],
        }
    }
}

fn start(network: &Network, kind: Kind) -> State {
    State::Searching(
        kind,
        network.job(async move {
            let client = client().map_err(NetworkError::from)?;
            let (networks, partial) = match networks::local_networks() {
                Ok(networks) => (networks, false),
                Err(error) => {
                    tracing::warn!(%error, "Could not enumerate LAN interfaces");
                    (Default::default(), true)
                }
            };
            let targets = networks::addresses(&networks)
                .into_iter()
                .map(|address| SocketAddrV4::new(address, kind.port()))
                .collect::<Vec<_>>();
            tracing::info!(
                kind = kind.code(),
                ?networks,
                candidates = targets.len(),
                "Starting LAN search"
            );
            Ok(scan(&client, kind, targets, partial).await)
        }),
    )
}

fn client() -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        // Only the enumerated IP and port may receive probes. A proxy or
        // redirect could route this local search beyond the connected networks.
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .pool_max_idle_per_host(0)
        .build()
}

async fn scan(
    client: &reqwest::Client,
    kind: Kind,
    targets: Vec<SocketAddrV4>,
    mut partial: bool,
) -> Results {
    let mut jobs = stream::iter(targets)
        .map(|target| async move { (target, probe(client, kind, target).await) })
        .buffer_unordered(CONCURRENCY);
    let deadline = tokio::time::sleep(SCAN_TIMEOUT);
    tokio::pin!(deadline);
    let mut found = Vec::new();
    let mut rejected = 0;
    loop {
        tokio::select! {
            _ = &mut deadline => {
                tracing::warn!(kind = kind.code(), ?SCAN_TIMEOUT, "LAN search deadline reached");
                partial = true;
                break;
            }
            result = jobs.next() => match result {
                Some((target, Ok(server))) => {
                    tracing::debug!(%target, product = server.name, "Verified LAN service");
                    found.push((target, server));
                }
                Some((target, Err(error))) => {
                    rejected += 1;
                    // Refusal and timeout are expected on unoccupied LAN addresses.
                    tracing::debug!(%target, %error, "LAN candidate did not identify as a service");
                }
                None => break,
            },
        }
    }
    found.sort_unstable_by_key(|(target, _)| *target);
    tracing::info!(
        kind = kind.code(),
        verified = found.len(),
        rejected,
        partial,
        "LAN search completed"
    );
    Results {
        servers: found.into_iter().map(|(_, server)| server).collect(),
        partial,
    }
}

#[derive(Debug, thiserror::Error)]
enum ProbeError {
    #[error(transparent)]
    Network(#[from] NetworkError),
    #[error("Unexpected HTTP status {0}")]
    Status(reqwest::StatusCode),
    // External parse errors may contain values from the server response.
    #[error("Invalid response from {0}")]
    Schema(&'static str),
}

async fn get(
    client: &reqwest::Client,
    url: &str,
    path: &str,
    limit: usize,
) -> Result<(reqwest::header::HeaderMap, Vec<u8>), ProbeError> {
    let response = client
        .get(format!("{url}{path}"))
        .send()
        .await
        .map_err(NetworkError::from)?;
    if !response.status().is_success() {
        return Err(ProbeError::Status(response.status()));
    }
    Ok(job::read_body(response, limit).await?)
}

#[derive(Deserialize)]
struct TunerVersion {
    current: String,
}
#[derive(Deserialize)]
struct RecordingsVersion {
    version: String,
}

fn version_text(value: &str) -> Option<String> {
    let value = value.trim();
    (value.len() <= 80
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_digit())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b".+-_".contains(&byte)))
    .then(|| value.to_owned())
}

async fn probe(
    client: &reqwest::Client,
    kind: Kind,
    target: SocketAddrV4,
) -> Result<Server, ProbeError> {
    let url = format!("http://{target}");
    let (name, version) = match kind {
        Kind::Tuner => {
            let (headers, bytes) = get(client, &url, "/api/services", TUNER_LIMIT).await?;
            crate::channels::parse(&bytes).map_err(|_| ProbeError::Schema("/api/services"))?;
            let product = ServerProduct::from_headers(&headers);
            let version = match headers.get(reqwest::header::SERVER) {
                Some(header) if product != ServerProduct::Unknown => header
                    .to_str()
                    .ok()
                    .and_then(|value| value.split_once('/'))
                    .and_then(|(_, value)| version_text(value)),
                _ => None,
            };
            let name = match product {
                ServerProduct::Mirakurun => "Mirakurun",
                ServerProduct::Mirakc => "mirakc",
                ServerProduct::Unknown => "Mirakurun / mirakc",
            };
            let version = match version {
                Some(version) => Some(version),
                None => match fetch_version(client, &url, kind).await {
                    Ok(version) => Some(version),
                    // An empty array alone is not evidence behind a proxy.
                    Err(error) if product == ServerProduct::Unknown => return Err(error),
                    Err(error) => {
                        tracing::debug!(%target, %error, "LAN tuner version unavailable");
                        None
                    }
                },
            };
            (name, version)
        }
        Kind::Recordings => {
            let (_, bytes) = get(
                client,
                &url,
                "/api/recorded?limit=1&offset=0&isHalfWidth=false",
                RECORDINGS_LIMIT,
            )
            .await?;
            crate::epgstation::parse(&bytes).map_err(|_| ProbeError::Schema("/api/recorded"))?;
            let version = match fetch_version(client, &url, kind).await {
                Ok(version) => Some(version),
                Err(error) => {
                    tracing::debug!(%target, %error, "LAN EPGStation version unavailable");
                    None
                }
            };
            ("EPGStation", version)
        }
    };
    Ok(Server { url, name, version })
}

async fn fetch_version(
    client: &reqwest::Client,
    url: &str,
    kind: Kind,
) -> Result<String, ProbeError> {
    const PATH: &str = "/api/version";
    let (_, bytes) = get(client, url, PATH, VERSION_LIMIT).await?;
    let version = match kind {
        Kind::Tuner => crate::json::from_slice::<TunerVersion>(&bytes).map(|v| v.current),
        Kind::Recordings => crate::json::from_slice::<RecordingsVersion>(&bytes).map(|v| v.version),
    }
    .map_err(|_| ProbeError::Schema(PATH))?;
    version_text(&version).ok_or(ProbeError::Schema(PATH))
}

#[cfg(feature = "native_tests")]
pub(crate) use networks::limit_to_test_loopback;
