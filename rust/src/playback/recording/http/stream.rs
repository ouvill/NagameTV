//! A response stays open across buffer-capacity waits. Only active I/O has a deadline.
use super::*;

pub(super) struct Stream {
    response: reqwest::Response,
    next: u64,
    pending: bytes::Bytes,
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
                                pending: bytes::Bytes::new(),
                                _client: client,
                            }
                        }
                    };
                    let bytes = stream.read_chunk(position, source.size).await?;
                    Ok((bytes, stream))
                }
            },
            retrying,
        ),
        cancelled,
    )
}

impl Stream {
    async fn read_chunk(&mut self, position: u64, size: u64) -> Result<Vec<u8>, Error> {
        loop {
            if self.pending.is_empty() {
                let chunk = tokio::time::timeout(REQUEST_TIMEOUT, self.response.chunk())
                    .await
                    .map_err(Error::WaitTimedOut)??
                    .ok_or(Error::Truncated)?;
                if chunk.len() as u64 > size - self.next {
                    return Err(Error::InvalidRange);
                }
                self.next += chunk.len() as u64;
                self.pending = chunk;
            }
            let start = self.next - self.pending.len() as u64;
            let skip = position
                .saturating_sub(start)
                .min(self.pending.len() as u64) as usize;
            self.pending = self.pending.slice(skip..);
            if !self.pending.is_empty() {
                // HTTP body chunks follow transport buffering, not our prefetch
                // reservation. Retain their suffix instead of rejecting valid data.
                let count = self.pending.len().min(RANGE_BYTES);
                return Ok(self.pending.split_to(count).to_vec());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_large_transport_chunk_preserves_bytes_across_bounded_reads_and_a_forward_skip()
    -> Result<(), Box<dyn std::error::Error>> {
        let expected: Vec<_> = (0..RANGE_BYTES * 3 + 17)
            .map(|offset| (offset % 251) as u8)
            .collect();
        let response = reqwest::Response::from(http::Response::new(expected.clone()));
        let mut stream = Stream {
            response,
            next: 0,
            pending: bytes::Bytes::new(),
            _client: client(true)?,
        };
        runtime()?.block_on(async {
            let first = stream.read_chunk(0, expected.len() as u64).await?;
            assert_eq!(first, expected[..RANGE_BYTES]);
            assert!(stream.pending.len() > RANGE_BYTES);
            let skip = 71;
            let mut position = RANGE_BYTES + skip;
            while position < expected.len() {
                let bytes = stream
                    .read_chunk(position as u64, expected.len() as u64)
                    .await?;
                assert!(bytes.len() <= RANGE_BYTES);
                assert_eq!(bytes, expected[position..position + bytes.len()]);
                position += bytes.len();
            }
            assert!(stream.pending.is_empty());
            Ok::<_, Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn a_transport_chunk_cannot_exceed_the_verified_recording_size()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut stream = Stream {
            response: reqwest::Response::from(http::Response::new(vec![0; RANGE_BYTES + 1])),
            next: 0,
            pending: bytes::Bytes::new(),
            _client: client(true)?,
        };
        assert!(matches!(
            runtime()?.block_on(stream.read_chunk(0, RANGE_BYTES as u64)),
            Err(Error::InvalidRange)
        ));
        Ok(())
    }
}
