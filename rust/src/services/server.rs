//! Validated addresses and proof of a successful catalog request. Constructors
//! are private so a caller cannot label arbitrary settings as connection-tested.
use super::{Error, FetchError, Progress, Request, Stopping};
use crate::channels::Channel;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerUrl(String);
impl ServerUrl {
    pub fn parse(value: &str) -> Result<Self, Error> {
        let value = value.trim().trim_end_matches('/');
        let url = reqwest::Url::parse(value)?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(Error::InvalidServerUrl);
        }
        Ok(Self(value.into()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The address and catalog from one successful HTTP/parse operation. This is
/// evidence of that operation, not a promise of continued network availability.
pub struct VerifiedServer {
    url: ServerUrl,
    channels: Vec<Channel>,
}
impl VerifiedServer {
    pub fn url(&self) -> &ServerUrl {
        &self.url
    }
    pub fn channels(&self) -> &[Channel] {
        &self.channels
    }
    pub fn into_channels(self) -> Vec<Channel> {
        self.channels
    }
}

pub struct Probe {
    url: ServerUrl,
    request: Request,
}
impl Probe {
    pub(super) fn new(url: ServerUrl, request: Request) -> Self {
        Self { url, request }
    }
    pub fn cancel(self) -> Stopping {
        self.request.cancel()
    }
    pub fn poll(
        self,
    ) -> Progress<Self, Result<VerifiedServer, FetchError<crate::channels::Error>>> {
        match self.request.poll() {
            Progress::Pending(request) => Progress::Pending(Self {
                url: self.url,
                request,
            }),
            Progress::Complete(result) => {
                Progress::Complete(result.map(|channels| VerifiedServer {
                    url: self.url,
                    channels,
                }))
            }
        }
    }
    #[cfg(test)]
    pub fn is_finished(&self) -> bool {
        self.request.is_finished()
    }
}
