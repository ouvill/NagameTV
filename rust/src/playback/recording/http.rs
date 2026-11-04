//! Verified, finite HTTP resources with bounded range recovery and playback read-ahead.
mod buffer;
mod stream;
pub(in crate::playback) use buffer::Control;
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
mod recovery_tests;
#[cfg(test)]
pub(super) mod tests;

pub(super) const RANGE_BYTES: usize = 512 * 1024;
// EPGStation rejects ranges extending beyond EOF and returns an empty body for
// a one-byte range. A two-byte probe discovers the size without either case.
const SIZE_PROBE_BYTES: usize = 2;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const RETRY_DEADLINE: Duration = Duration::from_secs(30);
const MAX_BACKOFF: Duration = Duration::from_secs(8);
pub(super) const CANCEL_POLL: Duration = Duration::from_millis(20);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Could not read the recording URL: {0}")]
    Network(#[source] reqwest::Error),
    #[error("The recording server returned HTTP {0}.")]
    Status(StatusCode),
    #[error("The recording URL must support HTTP byte ranges (206 Partial Content).")]
    RangeUnsupported,
    #[error("The recording server returned an invalid byte range.")]
    InvalidRange,
    #[error("The recording changed on the server. Open the URL again.")]
    Changed,
    #[error("The recording server returned an oversized body chunk.")]
    OversizedChunk,
    #[error("Timed out waiting for recording data: {0}")]
    WaitTimedOut(#[source] tokio::time::error::Elapsed),
    #[error("The recording server returned an incomplete byte range.")]
    Truncated,
    #[error("Recording URL recovery exceeded its 30 second deadline.")]
    Deadline,
    #[error("Recording URL recovery exceeded its deadline: {0}")]
    RetryExpired(#[source] Box<Error>),
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
        let client = client(false)?;
        let (bytes, size, validator) = fetch(
            &client,
            &url,
            0,
            SIZE_PROBE_BYTES,
            None,
            &|| cancelled.load(Ordering::Acquire),
            &|_| {},
        )?;
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
            ahead: None,
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
            ahead: None,
        }
    }
}

fn client(streaming: bool) -> Result<Client, Error> {
    let builder = Client::builder().connect_timeout(REQUEST_TIMEOUT);
    let builder = if streaming {
        builder
    } else {
        builder.timeout(REQUEST_TIMEOUT)
    };
    Ok(builder.build()?)
}

fn transient(error: &Error) -> bool {
    match error {
        Error::Status(status) => status.is_server_error(),
        Error::WaitTimedOut(_) => true,
        Error::Network(error) => {
            error.is_connect()
                || error.is_timeout()
                || error.is_request()
                || error.is_body()
                || error.is_decode()
                || error
                    .status()
                    .is_some_and(|status| status.is_server_error())
        }
        Error::RangeUnsupported
        | Error::InvalidRange
        | Error::Changed
        | Error::OversizedChunk
        | Error::Truncated
        | Error::Cancelled
        | Error::Runtime(_)
        | Error::RetryExpired(_)
        | Error::Deadline => false,
    }
}

fn fetch(
    original: &Client,
    url: &Url,
    start: u64,
    length: usize,
    expected: Option<&Verified>,
    cancelled: &dyn Fn() -> bool,
    retrying: &dyn Fn(&Error),
) -> Result<(Vec<u8>, u64, Validator), Error> {
    if cancelled() {
        return Err(Error::Cancelled);
    }
    run_cancellable(
        recover(
            |fresh| {
                let connection = if fresh {
                    client(false)
                } else {
                    Ok(original.clone())
                };
                async move { request(&connection?, url, start, length, expected).await }
            },
            retrying,
        ),
        cancelled,
    )
}

fn run_cancellable<T>(
    operation: impl std::future::Future<Output = Result<T, Error>>,
    cancelled: &dyn Fn() -> bool,
) -> Result<T, Error> {
    if cancelled() {
        return Err(Error::Cancelled);
    }
    runtime()?.block_on(async {
        let cancellation = async {
            loop {
                if cancelled() {
                    break;
                }
                tokio::time::sleep(CANCEL_POLL).await;
            }
        };
        tokio::select! { result = operation => result, () = cancellation => Err(Error::Cancelled) }
    })
}

async fn recover<T, F, Fut>(mut operation: F, retrying: &dyn Fn(&Error)) -> Result<T, Error>
where
    F: FnMut(bool) -> Fut,
    Fut: std::future::Future<Output = Result<T, Error>>,
{
    let deadline = tokio::time::Instant::now() + RETRY_DEADLINE;
    let mut attempt = 0_u32;
    loop {
        let result = tokio::time::timeout_at(deadline, operation(attempt != 0)).await;
        let error = match result {
            Ok(Ok(value)) => return Ok(value),
            Ok(Err(error)) => error,
            Err(_) => return Err(Error::Deadline),
        };
        if !transient(&error) {
            return Err(error);
        }
        if attempt == 0 {
            tracing::warn!(
                error = &error as &dyn std::error::Error,
                "Retrying recording HTTP range after a transient failure"
            );
            retrying(&error);
        }
        let delay = if attempt == 0 {
            Duration::ZERO
        } else {
            Duration::from_secs(1_u64 << (attempt - 1).min(3)).min(MAX_BACKOFF)
        };
        if tokio::time::Instant::now() + delay >= deadline {
            tokio::time::sleep_until(deadline).await;
            return Err(Error::RetryExpired(Box::new(error)));
        }
        tokio::time::sleep(delay).await;
        attempt += 1;
    }
}

async fn request(
    client: &Client,
    url: &Url,
    start: u64,
    length: usize,
    expected: Option<&Verified>,
) -> Result<(Vec<u8>, u64, Validator), Error> {
    let end = expected.map_or(start.saturating_add(length as u64 - 1), |source| {
        start.saturating_add(length as u64 - 1).min(source.size - 1)
    });
    let mut response = send(client, url, start, Some(end), expected).await?;
    let (expected_bytes, size, validator) = validate(&response, start, end, expected)?;
    let mut bytes = Vec::with_capacity(expected_bytes);
    while let Some(chunk) = response.chunk().await.map_err(Error::from)? {
        if chunk.len() > expected_bytes - bytes.len() {
            return Err(Error::InvalidRange);
        }
        bytes.extend_from_slice(&chunk);
    }
    if bytes.len() != expected_bytes {
        return Err(Error::Truncated);
    }
    Ok((bytes, size, validator))
}

async fn send(
    client: &Client,
    url: &Url,
    start: u64,
    end: Option<u64>,
    expected: Option<&Verified>,
) -> Result<reqwest::Response, Error> {
    let range = end.map_or_else(
        || format!("bytes={start}-"),
        |end| format!("bytes={start}-{end}"),
    );
    let mut request = client
        .get(url.clone())
        .header(header::ACCEPT_ENCODING, "identity")
        .header(header::RANGE, range);
    if let Some(value) = expected.and_then(|source| source.validator.header()) {
        request = request.header(header::IF_RANGE, value);
    }
    Ok(request.send().await?)
}

fn validate(
    response: &reqwest::Response,
    start: u64,
    end: u64,
    expected: Option<&Verified>,
) -> Result<(usize, u64, Validator), Error> {
    if response.status().is_client_error() || response.status().is_server_error() {
        return Err(Error::Status(response.status()));
    }
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
    Ok((expected_bytes, size, validator))
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
    ahead: Option<buffer::Buffer>,
}
impl Cursor {
    pub fn prefetch(&mut self) -> Control {
        let buffer = self.ahead.get_or_insert_with(|| {
            buffer::Buffer::new(self.source.clone(), self.cancelled.clone())
        });
        buffer.control()
    }
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
        if let Some(ahead) = &mut self.ahead {
            let count = ahead.read(self.position, output)?;
            self.position += count as u64;
            return Ok(count);
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
                &|| self.cancelled.load(Ordering::Acquire),
                &|_| {},
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
        if (position != self.position || !matches!(from, SeekFrom::Current(0)))
            && let Some(ahead) = &self.ahead
        {
            ahead.control().seek(position);
        }
        self.position = position;
        Ok(position)
    }
}
