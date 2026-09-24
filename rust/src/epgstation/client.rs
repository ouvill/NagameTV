//! Per-server cookie sessions. Credentials are consumed by login, never persisted.
use super::{Endpoint, Error, MAX_RESPONSE_BYTES, Page};
use crate::services::{FetchError, NetworkError};
use serde::Deserialize;
use std::time::Duration;

pub enum Login {
    Current,
    Password(Credentials),
}
pub struct Credentials {
    name: String,
    password: String,
}
impl Login {
    pub fn password(name: String, password: String) -> std::result::Result<Self, Error> {
        if name.is_empty() || password.is_empty() {
            return Err(Error::MissingCredentials);
        }
        Ok(Self::Password(Credentials { name, password }))
    }
}

pub(super) enum Access {
    Anonymous,
    Login(Credentials),
    Connected(Connection),
}
#[derive(Clone)]
enum MediaAccess {
    Anonymous,
    Token(String),
}
#[derive(Clone)]
pub(super) struct Connection {
    endpoint: Endpoint,
    client: reqwest::Client,
    media: MediaAccess,
}
impl Connection {
    pub(super) fn metadata(&self, video: u64) -> PlaybackMetadata {
        PlaybackMetadata {
            connection: self.clone(),
            video,
        }
    }
    pub fn video_url(&self, id: u64) -> String {
        let mut url = url::Url::parse(&format!("{}/api/videos/{id}", self.endpoint.as_str()))
            .expect("validated endpoint");
        match &self.media {
            MediaAccess::Anonymous => {}
            MediaAccess::Token(token) => {
                url.query_pairs_mut().append_pair("token", token);
            }
        }
        url.into()
    }
}

/// The cookie session and video ID stay together during asynchronous inspection.
/// This optional endpoint is absent on older EPGStation versions.
pub(crate) struct PlaybackMetadata {
    connection: Connection,
    video: u64,
}
impl PlaybackMetadata {
    pub(crate) fn start_ms(self, cancelled: &std::sync::atomic::AtomicBool) -> Option<i64> {
        use std::sync::atomic::Ordering;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .ok()?;
        runtime.block_on(async {
            let fetch = async {
                let response = self.connection.client.get(format!("{}/api/videos/{}/metadata", self.connection.endpoint.as_str(), self.video))
                    .send().await.map_err(network)?;
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Metadata { video_file_id: u64, start_at: Option<i64> }
                let metadata: Metadata = crate::json::from_slice(&body(response).await?).map_err(Error::from).map_err(FetchError::Parse)?;
                Ok::<_, FetchError<Error>>((metadata.video_file_id == self.video).then_some(metadata.start_at).flatten().filter(|ms| *ms > 0))
            };
            tokio::pin!(fetch);
            loop {
                if cancelled.load(Ordering::Acquire) { return None; }
                tokio::select! {
                    result = &mut fetch => return match result {
                        Ok(start) => start,
                        Err(error) => { tracing::debug!(%error, "EPGStation video timing unavailable; using catalogue timing"); None },
                    },
                    _ = tokio::time::sleep(Duration::from_millis(25)) => {},
                }
            }
        })
    }
}
pub(super) struct Fetched {
    pub page: Page,
    pub connection: Connection,
}
type Result<T> = std::result::Result<T, FetchError<Error>>;
fn network(error: reqwest::Error) -> FetchError<Error> {
    // Login errors must never expose a URL carrying credentials or a media token.
    FetchError::Network(NetworkError::Http(error.without_url()))
}
async fn body(response: reqwest::Response) -> Result<Vec<u8>> {
    let mut response = response.error_for_status().map_err(network)?;
    if !response.status().is_success() {
        return Err(FetchError::Parse(Error::UnexpectedStatus(
            response.status().as_u16(),
        )));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(network)? {
        if bytes.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(NetworkError::ResponseTooLarge {
                limit: MAX_RESPONSE_BYTES,
            }
            .into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
pub(super) async fn fetch(endpoint: Endpoint, url: String, access: Access) -> Result<Fetched> {
    let connection = match access {
        Access::Connected(connection) => connection,
        Access::Anonymous | Access::Login(_) => {
            let client = reqwest::Client::builder()
                .cookie_store(true)
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(10))
                .pool_max_idle_per_host(1)
                .build()
                .map_err(network)?;
            let media = match access {
                Access::Login(Credentials { name, password }) => {
                    let credentials =
                        serde_json::json!({ "name": name, "password": password }).to_string();
                    let response = client
                        .post(format!("{}/api/auth/login", endpoint.as_str()))
                        .header(reqwest::header::CONTENT_TYPE, "application/json")
                        .body(credentials)
                        .send()
                        .await
                        .map_err(network)?;
                    body(response).await?;
                    let response = client
                        .get(format!("{}/api/auth/media-token", endpoint.as_str()))
                        .send()
                        .await
                        .map_err(network)?;
                    #[derive(Deserialize)]
                    struct Token {
                        token: String,
                    }
                    let token: Token = crate::json::from_slice(&body(response).await?)
                        .map_err(Error::from)
                        .map_err(FetchError::Parse)?;
                    if token.token.is_empty() {
                        return Err(FetchError::Parse(Error::MissingToken));
                    }
                    MediaAccess::Token(token.token)
                }
                Access::Anonymous => MediaAccess::Anonymous,
                Access::Connected(_) => unreachable!("handled before client construction"),
            };
            Connection {
                endpoint,
                client,
                media,
            }
        }
    };
    let response = connection.client.get(url).send().await.map_err(network)?;
    let mut page = super::parse(&body(response).await?).map_err(FetchError::Parse)?;
    if page.rows.iter().any(|row| row.channel_id.is_some()) {
        // Some older servers do not expose channels to this account. Media still
        // opens, but we never guess broadcast IDs from opaque API resource IDs.
        match connection.channels().await {
            Ok(channels) => {
                for row in std::sync::Arc::make_mut(&mut page.rows) {
                    row.service = channels
                        .iter()
                        .find(|c| Some(c.id) == row.channel_id)
                        .map(|c| crate::channels::BroadcastService {
                            network_id: c.network_id,
                            service_id: c.service_id,
                        });
                }
            }
            Err(error) => {
                tracing::warn!(%error, "EPGStation broadcast channel metadata unavailable")
            }
        }
    }
    Ok(Fetched { page, connection })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Channel {
    id: u64,
    network_id: u16,
    service_id: u16,
}
impl Connection {
    async fn channels(&self) -> Result<Vec<Channel>> {
        let response = self
            .client
            .get(format!("{}/api/channels", self.endpoint.as_str()))
            .send()
            .await
            .map_err(network)?;
        crate::json::from_slice(&body(response).await?)
            .map_err(Error::from)
            .map_err(FetchError::Parse)
    }
}
