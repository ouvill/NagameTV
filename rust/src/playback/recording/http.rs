//! Verified, finite HTTP resources. Each cursor retains only one bounded range.
use reqwest::{Client, StatusCode, header};
use std::{
    io::{self, Read, Seek, SeekFrom},
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use url::Url;
#[cfg(test)]
pub(super) mod tests;

const RANGE_BYTES: usize = 512 * 1024;
// EPGStation rejects ranges extending beyond EOF and returns an empty body for
// a one-byte range. A two-byte probe discovers the size without either case.
const SIZE_PROBE_BYTES: usize = 2;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const CANCEL_POLL: Duration = Duration::from_millis(20);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Could not read the recording URL: {0}")]
    Network(#[source] reqwest::Error),
    #[error("The recording URL must support HTTP byte ranges (206 Partial Content).")]
    RangeUnsupported,
    #[error("The recording server returned an invalid byte range.")]
    InvalidRange,
    #[error("The recording changed on the server. Open the URL again.")]
    Changed,
    #[error("The recording server returned an incomplete byte range.")]
    Truncated,
    #[error("Recording URL loading was cancelled.")]
    Cancelled,
    #[error("Could not initialize recording URL loading: {0}")]
    Runtime(#[source] io::Error),
}
impl From<reqwest::Error> for Error {
    fn from(error: reqwest::Error) -> Self {
        // URLs can contain passwords and signed query strings.
        Self::Network(error.without_url())
    }
}

fn runtime() -> Result<&'static tokio::runtime::Runtime, Error> {
    static RUNTIME: OnceLock<io::Result<tokio::runtime::Runtime>> = OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
        })
        .as_ref()
        .map_err(|error| Error::Runtime(io::Error::other(error.to_string())))
}

#[derive(Clone)]
enum Validator {
    Etag(header::HeaderValue),
    Modified(header::HeaderValue),
    Unavailable,
}
impl Validator {
    fn from_headers(headers: &header::HeaderMap) -> Self {
        if let Some(etag) = headers.get(header::ETAG)
            && !etag.as_bytes().starts_with(b"W/")
        {
            return Self::Etag(etag.clone());
        }
        headers
            .get(header::LAST_MODIFIED)
            .cloned()
            .map(Self::Modified)
            .unwrap_or(Self::Unavailable)
    }
    fn header(&self) -> Option<&header::HeaderValue> {
        match self {
            Self::Etag(value) | Self::Modified(value) => Some(value),
            Self::Unavailable => None,
        }
    }
    fn agrees(&self, headers: &header::HeaderMap) -> bool {
        match self {
            Self::Etag(value) => headers.get(header::ETAG) == Some(value),
            Self::Modified(value) => headers.get(header::LAST_MODIFIED) == Some(value),
            Self::Unavailable => true,
        }
    }
}

#[derive(Clone)]
pub(in crate::playback) struct Verified {
    url: Url,
    client: Client,
    size: u64,
    validator: Validator,
}
impl std::fmt::Debug for Verified {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VerifiedHttpRecording")
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}
impl Verified {
    pub fn inspect(url: Url, cancelled: Arc<AtomicBool>) -> Result<Cursor, Error> {
        let client = Client::builder()
            .connect_timeout(REQUEST_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .build()?;
        let (bytes, size, validator) = fetch(&client, &url, 0, SIZE_PROBE_BYTES, None, &cancelled)?;
        Ok(Cursor {
            source: Self {
                url,
                client,
                size,
                validator,
            },
            position: 0,
            start: 0,
            bytes,
            cancelled,
        })
    }
    pub fn size(&self) -> u64 {
        self.size
    }
    pub fn url(&self) -> &Url {
        &self.url
    }
    pub fn cursor(&self, cancelled: Arc<AtomicBool>) -> Cursor {
        Cursor {
            source: self.clone(),
            position: 0,
            start: 0,
            bytes: Vec::new(),
            cancelled,
        }
    }
}

fn fetch(
    client: &Client,
    url: &Url,
    start: u64,
    length: usize,
    expected: Option<&Verified>,
    cancelled: &AtomicBool,
) -> Result<(Vec<u8>, u64, Validator), Error> {
    if cancelled.load(Ordering::Acquire) {
        return Err(Error::Cancelled);
    }
    runtime()?.block_on(async {
        let operation = async {
            let end = start.saturating_add(length as u64 - 1);
            let end = expected.map_or(end, |source| end.min(source.size - 1));
            let mut request = client
                .get(url.clone())
                .header(header::ACCEPT_ENCODING, "identity")
                .header(header::RANGE, format!("bytes={start}-{end}"));
            if let Some(value) = expected.and_then(|source| source.validator.header()) {
                request = request.header(header::IF_RANGE, value);
            }
            let mut response = request.send().await?.error_for_status()?;
            if response.status() != StatusCode::PARTIAL_CONTENT {
                return Err(if expected.is_some() {
                    Error::Changed
                } else {
                    Error::RangeUnsupported
                });
            }
            let headers = response.headers();
            if headers
                .get(header::CONTENT_ENCODING)
                .is_some_and(|value| value != "identity")
            {
                return Err(Error::InvalidRange);
            }
            let (actual_start, actual_end, size) = headers
                .get(header::CONTENT_RANGE)
                .and_then(|value| value.to_str().ok())
                .and_then(parse_range)
                .ok_or(Error::InvalidRange)?;
            if actual_start != start || actual_end != end.min(size - 1) {
                return Err(Error::InvalidRange);
            }
            if let Some(source) = expected
                && (size != source.size || !source.validator.agrees(headers))
            {
                return Err(Error::Changed);
            }
            let validator = Validator::from_headers(headers);
            let expected_bytes = (actual_end - actual_start + 1) as usize;
            if response
                .content_length()
                .is_some_and(|length| length != expected_bytes as u64)
            {
                return Err(Error::InvalidRange);
            }
            let mut bytes = Vec::with_capacity(expected_bytes);
            while let Some(chunk) = response.chunk().await? {
                if chunk.len() > expected_bytes - bytes.len() {
                    return Err(Error::InvalidRange);
                }
                bytes.extend_from_slice(&chunk);
            }
            if bytes.len() != expected_bytes {
                return Err(Error::Truncated);
            }
            Ok((bytes, size, validator))
        };
        let cancellation = async {
            loop {
                if cancelled.load(Ordering::Acquire) {
                    break;
                }
                tokio::time::sleep(CANCEL_POLL).await;
            }
        };
        tokio::select! {
            result = operation => result,
            () = cancellation => Err(Error::Cancelled),
        }
    })
}

fn parse_range(value: &str) -> Option<(u64, u64, u64)> {
    let (range, size) = value.strip_prefix("bytes ")?.split_once('/')?;
    let (start, end) = range.split_once('-')?;
    let (start, end, size) = (start.parse().ok()?, end.parse().ok()?, size.parse().ok()?);
    (start <= end && end < size).then_some((start, end, size))
}

pub(in crate::playback) struct Cursor {
    source: Verified,
    position: u64,
    start: u64,
    bytes: Vec<u8>,
    cancelled: Arc<AtomicBool>,
}
impl Cursor {
    pub fn source(&self) -> &Verified {
        &self.source
    }
}
impl Read for Cursor {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err(io::Error::other(Error::Cancelled));
        }
        if output.is_empty() || self.position >= self.source.size {
            return Ok(0);
        }
        if self.position < self.start || self.position - self.start >= self.bytes.len() as u64 {
            let start = self
                .position
                .min(self.source.size.saturating_sub(SIZE_PROBE_BYTES as u64));
            self.bytes = fetch(
                &self.source.client,
                &self.source.url,
                start,
                RANGE_BYTES,
                Some(&self.source),
                &self.cancelled,
            )
            .map_err(io::Error::other)?
            .0;
            self.start = start;
        }
        let offset = (self.position - self.start) as usize;
        let count = output.len().min(self.bytes.len() - offset);
        output[..count].copy_from_slice(&self.bytes[offset..offset + count]);
        self.position += count as u64;
        Ok(count)
    }
}
impl Seek for Cursor {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let position = match from {
            SeekFrom::Start(position) => Some(position),
            SeekFrom::Current(offset) => self.position.checked_add_signed(offset),
            SeekFrom::End(offset) => self.source.size.checked_add_signed(offset),
        }
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid recording offset"))?;
        self.position = position;
        Ok(position)
    }
}
