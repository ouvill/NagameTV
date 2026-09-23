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
    let page = super::parse(&body(response).await?).map_err(FetchError::Parse)?;
    Ok(Fetched { page, connection })
}
