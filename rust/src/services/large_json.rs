//! Progress deadlines for finite responses too large for the small-JSON client.
use super::{FetchError, NetworkError};
use std::time::Duration;

#[derive(Clone, Copy)]
struct Limits {
    stall: Duration,
    total: Duration,
}
const LIMITS: Limits = Limits {
    stall: Duration::from_secs(15),
    total: Duration::from_secs(120),
};

#[derive(Clone)]
pub(crate) struct Client(reqwest::Client);
impl Client {
    pub fn new() -> Result<Self, reqwest::Error> {
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .pool_max_idle_per_host(1)
            .build()
            .map(Self)
    }
    pub async fn fetch<T, E>(
        &self,
        url: String,
        fallback: Option<String>,
        limit: usize,
        parse: impl FnOnce(&[u8]) -> Result<T, E>,
    ) -> Result<T, FetchError<E>> {
        self.fetch_with(url, fallback, limit, parse, LIMITS).await
    }
    async fn fetch_with<T, E>(
        &self,
        url: String,
        fallback: Option<String>,
        limit: usize,
        parse: impl FnOnce(&[u8]) -> Result<T, E>,
        limits: Limits,
    ) -> Result<T, FetchError<E>> {
        let operation = async {
            let mut response = self.headers(url, limits.stall).await?;
            if matches!(
                response.status(),
                reqwest::StatusCode::NOT_FOUND | reqwest::StatusCode::METHOD_NOT_ALLOWED
            ) && let Some(fallback) = fallback
            {
                // A missing service endpoint is capability negotiation, not a
                // failed fetch. The overall deadline includes both attempts.
                drop(response);
                response = self.headers(fallback, limits.stall).await?;
            }
            let mut response = response
                .error_for_status()
                .map_err(|error| NetworkError::Http(error.without_url()))?;
            let mut bytes = Vec::new();
            let mut deadline = tokio::time::Instant::now() + limits.stall;
            loop {
                let chunk = tokio::time::timeout_at(deadline, response.chunk())
                    .await
                    .map_err(|_| NetworkError::Stalled {
                        limit: limits.stall,
                    })?
                    .map_err(|error| NetworkError::Http(error.without_url()))?;
                let Some(chunk) = chunk else {
                    break;
                };
                if chunk.len() > limit.saturating_sub(bytes.len()) {
                    return Err(NetworkError::ResponseTooLarge { limit }.into());
                }
                if !chunk.is_empty() {
                    bytes.extend_from_slice(&chunk);
                    deadline = tokio::time::Instant::now() + limits.stall;
                }
            }
            tracing::debug!(
                path = response.url().path(),
                body_bytes = bytes.len(),
                buffer_capacity_bytes = bytes.capacity(),
                "Large JSON received"
            );
            parse(&bytes).map_err(FetchError::Parse)
        };
        tokio::time::timeout(limits.total, operation)
            .await
            .map_err(|_| NetworkError::Deadline {
                limit: limits.total,
            })?
    }
    async fn headers(
        &self,
        url: String,
        stall: Duration,
    ) -> Result<reqwest::Response, NetworkError> {
        tokio::time::timeout(stall, self.0.get(url).send())
            .await
            .map_err(|_| NetworkError::Stalled { limit: stall })?
            .map_err(|error| NetworkError::Http(error.without_url()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
        task::JoinHandle,
    };
    type TestResult = Result<(), Box<dyn std::error::Error>>;
    type Fetch = JoinHandle<Result<Vec<u8>, FetchError<std::convert::Infallible>>>;

    async fn drive<F: Future>(operation: F) -> F::Output {
        tokio::pin!(operation);
        let end = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if let std::task::Poll::Ready(result) = futures_util::poll!(&mut operation) {
                return result;
            }
            assert!(
                std::time::Instant::now() < end,
                "loopback I/O did not finish"
            );
            tokio::task::yield_now().await;
        }
    }
    async fn request(limit: usize) -> Result<(Fetch, TcpStream), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let url = format!("http://{}/api/programs", listener.local_addr()?);
        let client = Client::new()?;
        let fetch = tokio::spawn(async move {
            client
                .fetch(url, None, limit, |bytes| Ok(bytes.to_vec()))
                .await
        });
        let (mut socket, _) = listener.accept().await?;
        let mut headers = Vec::new();
        while !headers.ends_with(b"\r\n\r\n") {
            headers.push(socket.read_u8().await?);
        }
        Ok((fetch, socket))
    }
    async fn settle() {
        for _ in 0..100 {
            tokio::task::yield_now().await;
        }
    }
    #[tokio::test(start_paused = true)]
    async fn progressing_body_can_exceed_small_json_total_timeout() -> TestResult {
        drive(async {
            let (fetch, mut socket) = request(4).await?;
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\na")
                .await?;
            settle().await;
            for byte in *b"bcd" {
                tokio::time::advance(LIMITS.stall - Duration::from_secs(1)).await;
                socket.write_all(&[byte]).await?;
                settle().await;
            }
            assert_eq!(fetch.await??, b"abcd");
            Ok(())
        })
        .await
    }
    #[tokio::test(start_paused = true)]
    async fn headers_and_partial_body_abort_when_no_progress_reaches_stall_limit() -> TestResult {
        drive(async {
            for body in [false, true] {
                let (fetch, mut socket) = request(100).await?;
                if body { socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nx").await?; }
                settle().await;
                tokio::time::advance(LIMITS.stall).await;
                assert!(matches!(fetch.await?, Err(FetchError::Network(NetworkError::Stalled { limit })) if limit==LIMITS.stall));
                assert_eq!(socket.read(&mut [0]).await?,0);
            }
            Ok(())
        }).await
    }
    #[tokio::test(start_paused = true)]
    async fn overall_cap_stops_a_response_that_keeps_trickling() -> TestResult {
        drive(async {
            let (fetch, mut socket) = request(100).await?;
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nx").await?;
            settle().await;
            for _ in 0..8 {
                tokio::time::advance(Duration::from_secs(14)).await;
                socket.write_all(b"x").await?;
                settle().await;
            }
            tokio::time::advance(Duration::from_secs(8)).await;
            assert!(matches!(fetch.await?,Err(FetchError::Network(NetworkError::Deadline { limit })) if limit==LIMITS.total));
            assert_eq!(socket.read(&mut [0]).await?,0);
            Ok(())
        }).await
    }
    #[tokio::test(start_paused = true)]
    async fn oversized_response_is_rejected_before_parsing() -> TestResult {
        drive(async {
            let (fetch, mut socket) = request(4).await?;
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nabcde")
                .await?;
            assert!(matches!(
                fetch.await?,
                Err(FetchError::Network(NetworkError::ResponseTooLarge {
                    limit: 4
                }))
            ));
            Ok(())
        })
        .await
    }
}
