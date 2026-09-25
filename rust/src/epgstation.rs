//! EPGStation recording catalogue and request ownership, independent of Qt.
use crate::services::{FetchError, Job, Network, Progress, ServerUrl, Stopping};
use serde::Deserialize;
use std::sync::Arc;
mod client;
#[cfg(test)]
mod provider_tests;
#[cfg(test)]
mod tests;
pub use client::Login;
pub(crate) use client::PlaybackMetadata;
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
    Recorded { files: Arc<[Video]> },
    Recording,
    NoVideoFile,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Video {
    pub id: u64,
    pub name: String,
    pub filename: String,
    pub kind: VideoType,
    pub start_ms: Option<i64>,
}
impl Availability {
    pub fn files(&self) -> Arc<[Video]> {
        match self {
            Self::Recorded { files } => files.clone(),
            Self::Recording | Self::NoVideoFile => Arc::default(),
        }
    }
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
    channel_id: Option<u64>,
    pub service: Option<crate::channels::BroadcastService>,
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
    channel_id: Option<u64>,
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
    #[serde(rename = "startAt")]
    start_at: Option<i64>,
    #[serde(default)]
    name: String,
    #[serde(default)]
    filename: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum VideoType {
    #[serde(rename = "ts")]
    Ts,
    #[serde(rename = "encoded")]
    Encoded,
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
                let mut ids = std::collections::HashSet::new();
                let mut files = record
                    .video_files
                    .into_iter()
                    .filter(|file| file.kind != VideoType::Unsupported && file.size > 0)
                    .map(|file| {
                        if !ids.insert(file.id) {
                            return Err(Error::InvalidCatalogue("duplicate video ID"));
                        }
                        Ok(Video {
                            id: file.id,
                            name: file.name,
                            filename: file.filename,
                            kind: file.kind,
                            start_ms: file.start_at.filter(|ms| *ms > 0),
                        })
                    })
                    .collect::<Result<Vec<_>, Error>>()?;
                // Preserve existing one-click TS playback; encoded-only recordings
                // use their first completed file. The chooser exposes every candidate.
                files.sort_by_key(|file| file.kind != VideoType::Ts);
                if files.is_empty() {
                    Availability::NoVideoFile
                } else {
                    Availability::Recorded {
                        files: files.into(),
                    }
                }
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
                channel_id: record.channel_id,
                service: None,
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
                        Err(error) => {
                            tracing::error!(
                                error = &error as &dyn std::error::Error,
                                "Recording catalogue fetch failed"
                            );
                            self.error = Some(error);
                        }
                    }
                }
            },
        }
        None
    }
    pub fn files(&self, id: u64) -> Arc<[Video]> {
        if self.busy() {
            return Arc::default();
        }
        self.rows()
            .iter()
            .find(|row| row.id == id)
            .map(|row| row.availability.files())
            .unwrap_or_default()
    }
    /// Resolve both IDs in the current server/page. A stale chooser cannot play
    /// a file from another recording or reuse an old server's authentication.
    pub fn video_url(&self, id: u64, video: u64) -> Option<String> {
        let file = self.files(id).iter().any(|file| file.id == video);
        match (&self.catalogue, file) {
            (Catalogue::Loaded { connection, .. }, true) => Some(connection.video_url(video)),
            _ => None,
        }
    }
    pub fn playback_request(
        &self,
        id: u64,
        video: Option<u64>,
    ) -> Option<crate::playback::recording::Request> {
        let rows = self.rows();
        let recording = rows.iter().find(|row| row.id == id)?;
        let files = self.files(id);
        let file = match video {
            Some(id) => files.iter().find(|file| file.id == id)?,
            None => files.first()?,
        };
        let start = file
            .start_ms
            .or_else(|| {
                files
                    .iter()
                    .find(|v| v.kind == VideoType::Ts)
                    .and_then(|v| v.start_ms)
            })
            .unwrap_or(recording.start_ms);
        let broadcast = recording
            .service
            .and_then(|service| crate::playback::recording::Broadcast::new(service, start));
        let metadata =
            if file.kind == VideoType::Encoded && file.start_ms.is_none() && broadcast.is_some() {
                match &self.catalogue {
                    Catalogue::Loaded { connection, .. } => Some(connection.metadata(file.id)),
                    Catalogue::Empty => None,
                }
            } else {
                None
            };
        crate::playback::recording::Request::epgstation(
            &self.video_url(id, file.id)?,
            broadcast,
            metadata,
        )
        .ok()
    }
    #[cfg(test)]
    pub fn playback_url(&self, id: u64) -> Option<String> {
        self.video_url(id, self.files(id).first()?.id)
    }
}
