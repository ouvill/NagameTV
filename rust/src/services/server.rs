//! Validated addresses and proof of a successful catalog request. Constructors
//! are private so a caller cannot label arbitrary settings as connection-tested.
use super::{Error, FetchError, Job, Progress, Stopping};
use crate::channels::Channel;

/// Presentation only; protocol selection must use observed endpoint capabilities.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ServerProduct {
    Mirakurun,
    Mirakc,
    #[default]
    Unknown,
}
impl ServerProduct {
    pub(super) fn from_headers(headers: &reqwest::header::HeaderMap) -> Self {
        let Some(header) = headers.get(reqwest::header::SERVER) else {
            return Self::Unknown;
        };
        // An absent or opaque product header is normal for reverse proxies.
        let Ok(header) = header.to_str() else {
            return Self::Unknown;
        };
        match header.split('/').next() {
            Some("Mirakurun") => Self::Mirakurun,
            Some("mirakc") => Self::Mirakc,
            _ => Self::Unknown,
        }
    }
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Mirakurun => "Mirakurun",
            Self::Mirakc => "mirakc",
            Self::Unknown => "",
        }
    }
}

pub(super) struct Catalog {
    pub channels: Vec<Channel>,
    pub product: ServerProduct,
}

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
    product: ServerProduct,
}
impl VerifiedServer {
    pub fn url(&self) -> &ServerUrl {
        &self.url
    }
    pub fn channels(&self) -> &[Channel] {
        &self.channels
    }
    pub fn product(&self) -> ServerProduct {
        self.product
    }
    pub fn into_channels(self) -> Vec<Channel> {
        self.channels
    }
}

pub struct Probe {
    url: ServerUrl,
    request: Job<Catalog, crate::channels::Error>,
}
impl Probe {
    pub(super) fn new(url: ServerUrl, request: Job<Catalog, crate::channels::Error>) -> Self {
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
                Progress::Complete(result.map(|catalog| VerifiedServer {
                    url: self.url,
                    channels: catalog.channels,
                    product: catalog.product,
                }))
            }
        }
    }
    #[cfg(test)]
    pub fn is_finished(&self) -> bool {
        self.request.is_finished()
    }
}
