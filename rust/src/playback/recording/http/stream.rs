//! A response stays open across buffer-capacity waits. Only active I/O has a deadline.
use super::*;

pub(super) struct Stream {
    response: reqwest::Response,
    next: u64,
    // Keep the request client alive with its in-flight response.
    _client: Client,
}

pub(super) fn next(
    source: &Verified,
    position: u64,
    mut stream: Option<Stream>,
    cancelled: &dyn Fn() -> bool,
    retrying: &dyn Fn(&Error),
) -> Result<(Vec<u8>, Stream), Error> {
    run_cancellable(
        recover(
            |_| {
                let previous = stream.take();
                async move {
                    let mut stream = match previous {
                        Some(stream) => stream,
                        None => {
                            // Every reopened response gets a new client/connection.
                            // No idle-pool policy is needed for a response still in use.
                            let client = client(true)?;
                            let start =
                                position.min(source.size.saturating_sub(SIZE_PROBE_BYTES as u64));
                            let response = tokio::time::timeout(
                                REQUEST_TIMEOUT,
                                send(&client, &source.url, start, None, Some(source)),
                            )
                            .await
                            .map_err(Error::WaitTimedOut)??;
                            validate(&response, start, source.size - 1, Some(source))?;
                            Stream {
                                response,
                                next: start,
                                _client: client,
                            }
                        }
                    };
                    loop {
                        let chunk = tokio::time::timeout(REQUEST_TIMEOUT, stream.response.chunk())
                            .await
                            .map_err(Error::WaitTimedOut)??
                            .ok_or(Error::Truncated)?;
                        if chunk.len() > RANGE_BYTES {
                            return Err(Error::OversizedChunk);
                        }
                        if chunk.len() as u64 > source.size - stream.next {
                            return Err(Error::InvalidRange);
                        }
                        let skip =
                            position.saturating_sub(stream.next).min(chunk.len() as u64) as usize;
                        stream.next += chunk.len() as u64;
                        if skip != chunk.len() {
                            return Ok((chunk[skip..].to_vec(), stream));
                        }
                    }
                }
            },
            retrying,
        ),
        cancelled,
    )
}
