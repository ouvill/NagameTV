//! EPGStation recording catalogue and request ownership, independent of Qt.
use crate::services::{FetchError, Job, Network, Progress, ServerUrl, Stopping};
use serde::Deserialize;
use std::sync::Arc;
mod client;
#[cfg(test)]
mod tests;
pub use client::Login;
use client::{Access, Connection, Fetched};

pub const PAGE_SIZE: u64 = 50;
const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const MAX_KEYWORD_CHARS: usize = 256;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Address(#[from] crate::services::Error),
    #[error("Enter an HTTP or HTTPS server URL without credentials")]
    Credentials,
    #[error("The search keyword must be at most {MAX_KEYWORD_CHARS} characters")]
    KeywordTooLong,
    #[error("{0}")]
    Json(#[from] crate::json::Error),
    #[error("Invalid recording catalogue: {0}")]
    InvalidCatalogue(&'static str),
    #[error("Enter both a username and password")]
    MissingCredentials,
    #[error("The server did not issue a playback token")]
    MissingToken,
    #[error("Unexpected HTTP status: {0}")]
    UnexpectedStatus(u16),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint(ServerUrl);
impl Endpoint {
    pub fn parse(value: &str) -> Result<Self, Error> {
        let server = ServerUrl::parse(value)?;
        let url = url::Url::parse(server.as_str()).expect("ServerUrl validated the URL");
        if !url.username().is_empty() || url.password().is_some() {
            return Err(Error::Credentials);
        }
        Ok(Self(server))
    }
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// Proof tied to the address whose catalogue was successfully fetched and parsed.
pub struct VerifiedEndpoint(Endpoint);
impl VerifiedEndpoint {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Availability {
    Recorded { video_id: u64 },
    Recording,
    NoTsFile,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recording {
    pub id: u64,
    pub name: String,
    pub channel: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub description: String,
    pub availability: Availability,
}

#[derive(Deserialize)]
struct Response {
    records: Vec<WireRecording>,
    total: u64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireRecording {
    id: u64,
    name: String,
    channel_name: Option<String>,
    ts_channel_name: Option<String>,
    start_at: i64,
    end_at: i64,
    description: Option<String>,
    is_recording: bool,
    #[serde(default)]
    video_files: Vec<WireVideo>,
}
#[derive(Deserialize)]
struct WireVideo {
    id: u64,
    #[serde(rename = "type")]
    kind: VideoType,
    size: u64,
}
#[derive(Deserialize)]
enum VideoType {
    #[serde(rename = "ts")]
    Ts,
    #[serde(other)]
    Unsupported,
}

pub struct Page {
    rows: Arc<[Recording]>,
    total: u64,
}
fn parse(bytes: &[u8]) -> Result<Page, Error> {
    let response: Response = crate::json::from_slice(bytes)?;
    if response.records.len() as u64 > PAGE_SIZE {
        return Err(Error::InvalidCatalogue("too many rows"));
    }
    if response.total < response.records.len() as u64 {
        return Err(Error::InvalidCatalogue("total is smaller than the page"));
    }
    let mut ids = std::collections::HashSet::new();
    let rows = response
        .records
        .into_iter()
        .map(|record| {
            if !ids.insert(record.id) || record.end_at < record.start_at {
                return Err(Error::InvalidCatalogue("duplicate ID or invalid dates"));
            }
            let availability = if record.is_recording {
                Availability::Recording
            } else {
                record
                    .video_files
                    .into_iter()
                    .find(|file| matches!(file.kind, VideoType::Ts) && file.size > 0)
                    .map_or(Availability::NoTsFile, |file| Availability::Recorded {
                        video_id: file.id,
                    })
            };
            Ok(Recording {
                id: record.id,
                name: record.name,
                channel: record
                    .ts_channel_name
                    .filter(|name| !name.is_empty())
                    .or(record.channel_name)
                    .unwrap_or_default(),
                start_ms: record.start_at,
                end_ms: record.end_at,
                description: record.description.unwrap_or_default(),
                availability,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(Page {
        rows: rows.into(),
        total: response.total,
    })
}

#[derive(Clone)]
struct Query {
    endpoint: Endpoint,
    keyword: String,
    offset: u64,
}
impl Query {
    fn url(&self) -> String {
        let mut url = url::Url::parse(&format!("{}/api/recorded", self.endpoint.as_str()))
            .expect("validated endpoint");
        url.query_pairs_mut()
            .append_pair("isHalfWidth", "false")
            .append_pair("offset", &self.offset.to_string())
            .append_pair("limit", &PAGE_SIZE.to_string());
        if !self.keyword.is_empty() {
            url.query_pairs_mut().append_pair("keyword", &self.keyword);
        }
        url.into()
    }
}

struct Pending {
    query: Query,
    access: Access,
}
impl Pending {
    fn start(self, network: &Network) -> Operation {
        let endpoint = self.query.endpoint.clone();
        let url = self.query.url();
        let job = network.job(client::fetch(endpoint, url, self.access));
        Operation::Fetching {
            query: self.query,
            job,
        }
    }
}

#[derive(Default)]
enum Operation {
    #[default]
    Idle,
    Fetching {
        query: Query,
        job: Job<Fetched, Error>,
    },
    Stopping {
        stopping: Stopping,
        next: Option<Pending>,
    },
}

#[derive(Default)]
enum Catalogue {
    #[default]
    Empty,
    Loaded {
        query: Query,
        page: Page,
        connection: Connection,
    },
}

#[derive(Default)]
pub struct Library {
    operation: Operation,
    catalogue: Catalogue,
    session: Option<(Endpoint, Connection)>,
    error: Option<FetchError<Error>>,
    revision: u64,
}
impl Library {
    pub fn loaded(&self) -> bool {
        matches!(self.catalogue, Catalogue::Loaded { .. })
    }
    pub fn busy(&self) -> bool {
        !matches!(self.operation, Operation::Idle)
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn error(&self) -> Option<&FetchError<Error>> {
        self.error.as_ref()
    }
    pub fn rows(&self) -> Arc<[Recording]> {
        match &self.catalogue {
            Catalogue::Empty => Arc::from([]),
            Catalogue::Loaded { page, .. } => page.rows.clone(),
        }
    }
    pub fn total(&self) -> u64 {
        match &self.catalogue {
            Catalogue::Empty => 0,
            Catalogue::Loaded { page, .. } => page.total,
        }
    }
    pub fn offset(&self) -> u64 {
        match &self.catalogue {
            Catalogue::Empty => 0,
            Catalogue::Loaded { query, .. } => query.offset,
        }
    }
    pub fn has_previous(&self) -> bool {
        !self.busy() && self.offset() > 0
    }
    pub fn has_next(&self) -> bool {
        !self.busy()
            && !self.rows().is_empty()
            && self.offset().saturating_add(PAGE_SIZE) < self.total()
    }
    pub fn open(
        &mut self,
        network: &Network,
        server: &str,
        keyword: &str,
        login: Login,
    ) -> Result<(), Error> {
        let endpoint = Endpoint::parse(server)?;
        let keyword = keyword.trim();
        if keyword.chars().count() > MAX_KEYWORD_CHARS {
            return Err(Error::KeywordTooLong);
        }
        let access = match login {
            Login::Password(credentials) => Access::Login(credentials),
            Login::Current => match &self.session {
                Some((connected, connection)) if connected == &endpoint => {
                    Access::Connected(connection.clone())
                }
                _ => Access::Anonymous,
            },
        };
        if !matches!(access, Access::Connected(_)) {
            self.session = None;
        }
        self.begin(
            network,
            Pending {
                query: Query {
                    endpoint,
                    keyword: keyword.into(),
                    offset: 0,
                },
                access,
            },
        );
        Ok(())
    }
    pub fn next(&mut self, network: &Network) {
        if self.has_next() {
            self.move_page(network, self.offset() + PAGE_SIZE);
        }
    }
    pub fn previous(&mut self, network: &Network) {
        if self.has_previous() {
            self.move_page(network, self.offset().saturating_sub(PAGE_SIZE));
        }
    }
    fn move_page(&mut self, network: &Network, offset: u64) {
        if let Catalogue::Loaded {
            query, connection, ..
        } = &self.catalogue
        {
            let mut query = query.clone();
            query.offset = offset;
            let access = Access::Connected(connection.clone());
            self.begin(network, Pending { query, access });
        }
    }
    fn begin(&mut self, network: &Network, query: Pending) {
        self.error = None;
        self.catalogue = Catalogue::Empty;
        self.revision = self.revision.wrapping_add(1);
        self.operation = match std::mem::take(&mut self.operation) {
            Operation::Idle => query.start(network),
            Operation::Fetching { job, .. } => Operation::Stopping {
                stopping: job.cancel(),
                next: Some(query),
            },
            Operation::Stopping { stopping, .. } => Operation::Stopping {
                stopping,
                next: Some(query),
            },
        };
    }
    pub fn cancel(&mut self) {
        self.operation = match std::mem::take(&mut self.operation) {
            Operation::Idle => Operation::Idle,
            Operation::Fetching { job, .. } => Operation::Stopping {
                stopping: job.cancel(),
                next: None,
            },
            Operation::Stopping { stopping, .. } => Operation::Stopping {
                stopping,
                next: None,
            },
        };
    }
    pub fn poll(&mut self, network: &Network) -> Option<VerifiedEndpoint> {
        match std::mem::take(&mut self.operation) {
            Operation::Idle => {}
            Operation::Stopping { stopping, next } => match stopping.poll() {
                Progress::Pending(stopping) => {
                    self.operation = Operation::Stopping { stopping, next }
                }
                Progress::Complete(()) => {
                    if let Some(query) = next {
                        self.operation = query.start(network);
                    }
                }
            },
            Operation::Fetching { query, job } => match job.poll() {
                Progress::Pending(job) => self.operation = Operation::Fetching { query, job },
                Progress::Complete(result) => {
                    self.revision = self.revision.wrapping_add(1);
                    match result {
                        Ok(Fetched { page, connection }) => {
                            let verified = VerifiedEndpoint(query.endpoint.clone());
                            self.session = Some((query.endpoint.clone(), connection.clone()));
                            self.catalogue = Catalogue::Loaded {
                                query,
                                page,
                                connection,
                            };
                            return Some(verified);
                        }
                        Err(error) => self.error = Some(error),
                    }
                }
            },
        }
        None
    }
    /// Resolve a stable recording ID only in the currently displayed server/page.
    pub fn playback_url(&self, id: u64) -> Option<String> {
        if self.busy() {
            return None;
        }
        match &self.catalogue {
            Catalogue::Empty => None,
            Catalogue::Loaded {
                page, connection, ..
            } => page
                .rows
                .iter()
                .find(|row| row.id == id)
                .and_then(|row| match row.availability {
                    Availability::Recorded { video_id } => Some(connection.video_url(video_id)),
                    Availability::Recording | Availability::NoTsFile => None,
                }),
        }
    }
}
