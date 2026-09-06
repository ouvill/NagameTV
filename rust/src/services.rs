use serde::Deserialize;
use std::{collections::HashSet, sync::mpsc, time::Duration};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("チャンネルJSONの解析失敗: {0}")]
    Json(#[from] serde_json::Error),
    #[error("TVチャンネルがありません")]
    NoTvChannels,
    #[error("サーバーURLの解析失敗: {0}")]
    Url(#[from] url::ParseError),
    #[error("http:// または https:// のサーバーURLを入力してください")]
    InvalidServerUrl,
}

#[derive(Debug, thiserror::Error)]
pub enum NetworkError {
    #[error("ネットワーク実行環境の初期化失敗: {0}")]
    Runtime(#[from] std::io::Error),
    #[error("HTTPエラー: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Response exceeds {limit} bytes")]
    ResponseTooLarge { limit: usize },
    #[error("Channel worker stopped")]
    WorkerStopped,
}

/// Transport does not know the feature's parse/validation error type.
#[derive(Debug, thiserror::Error)]
pub enum FetchError<E> {
    #[error("{0}")]
    Network(#[from] NetworkError),
    #[error("{0}")]
    Parse(#[source] E),
}

#[derive(Deserialize, Debug)]
pub struct Service {
    pub id: u64,
    pub name: String,
    #[serde(rename = "type")]
    kind: u32,
}

pub fn parse(bytes: &[u8]) -> Result<Vec<Service>, Error> {
    let entries: Vec<Service> = serde_json::from_slice(bytes)?;
    let mut seen = HashSet::new();
    let entries: Vec<_> = entries
        .into_iter()
        .filter(|s| s.kind == 1 && s.id != 0 && !s.name.is_empty() && seen.insert(s.id))
        .collect();
    if entries.is_empty() {
        return Err(Error::NoTvChannels);
    }
    Ok(entries)
}

pub fn server_url(value: &str) -> Result<String, Error> {
    let value = value.trim().trim_end_matches('/');
    let url = reqwest::Url::parse(value)?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::InvalidServerUrl);
    }
    Ok(value.into())
}

pub struct Network {
    runtime: tokio::runtime::Runtime,
    client: reqwest::Client,
}

impl Network {
    pub fn new() -> Result<Self, NetworkError> {
        Ok(Self {
            runtime: tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()?,
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(10))
                .pool_max_idle_per_host(1)
                .build()?,
        })
    }
    pub fn fetch(&self, server: &str) -> Request {
        self.fetch_json(format!("{server}/api/services"), 1024 * 1024, parse)
    }
    pub fn fetch_json<T: Send + 'static, E: Send + 'static>(
        &self,
        url: String,
        limit: usize,
        parse: fn(&[u8]) -> Result<T, E>,
    ) -> Job<T, E> {
        let (tx, rx) = mpsc::sync_channel(1);
        let client = self.client.clone();
        let task = self.runtime.spawn(async move {
            let result = async {
                let mut response = client
                    .get(url)
                    .send()
                    .await
                    .map_err(NetworkError::from)?
                    .error_for_status()
                    .map_err(NetworkError::from)?;
                let mut bytes = Vec::new();
                while let Some(chunk) = response.chunk().await.map_err(NetworkError::from)? {
                    if bytes.len() + chunk.len() > limit {
                        return Err(NetworkError::ResponseTooLarge { limit }.into());
                    }
                    bytes.extend_from_slice(&chunk);
                }
                // Capacity includes spare buffer space; neither value is process RSS.
                // Log only the endpoint path, never server credentials or query data.
                eprintln!(
                    "HTTP_JSON path={} body_bytes={} buffer_capacity_bytes={}",
                    response.url().path(),
                    bytes.len(),
                    bytes.capacity()
                );
                parse(&bytes).map_err(FetchError::Parse)
            }
            .await;
            let _ = tx.try_send(result);
        });
        Job { task, rx }
    }
}

pub type Request = Job<Vec<Service>, Error>;

pub struct Job<T, E> {
    task: tokio::task::JoinHandle<()>,
    rx: mpsc::Receiver<Result<T, FetchError<E>>>,
}
impl<T, E> Job<T, E> {
    pub fn cancel(&self) {
        self.task.abort();
    }
    pub fn is_finished(&self) -> bool {
        self.task.is_finished()
    }
    pub fn poll(&self) -> Option<Result<T, FetchError<E>>> {
        match self.rx.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err(NetworkError::WorkerStopped.into())),
        }
    }
}
impl<T, E> Drop for Job<T, E> {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selects_tv_preserving_large_ids_and_deduplicating() {
        let entries = parse(
            br#"[{"id":3203246080,"name":"TV","type":1},
            {"id":3203246080,"name":"duplicate","type":1},
            {"id":2,"name":"Radio","type":2}]"#,
        )
        .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, 3203246080);
    }
    #[test]
    fn rejects_bad_or_empty_services_and_urls() {
        for bytes in [b"broken".as_slice(), b"{}", b"[]"] {
            assert!(parse(bytes).is_err());
        }
        for url in ["file:///tmp/a", "http://", "http://localhost/?q=1"] {
            assert!(server_url(url).is_err());
        }
        assert_eq!(
            server_url(" http://localhost:40772/ ").unwrap(),
            "http://localhost:40772"
        );
    }
    #[test]
    fn fetch_preserves_http_parse_and_capacity_failures() {
        use std::error::Error as _;
        use std::{
            io::{Read, Write},
            net::TcpListener,
            thread,
            time::Instant,
        };

        let network = Network::new().unwrap();
        for (status, body, limit) in [
            (503, "unavailable", 1024),
            (200, "not json", 1024),
            (200, "[]", 1),
            (200, "[]", 2),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let url = format!("http://{}/api/services", listener.local_addr().unwrap());
            let server = thread::spawn(move || {
                let deadline = Instant::now() + Duration::from_secs(3);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(e)
                            if e.kind() == std::io::ErrorKind::WouldBlock
                                && Instant::now() < deadline =>
                        {
                            thread::sleep(Duration::from_millis(1))
                        }
                        Err(e) => panic!("test server accept failed: {e}"),
                    }
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(1)))
                    .unwrap();
                let mut header = Vec::new();
                while !header.ends_with(b"\r\n\r\n") {
                    assert!(header.len() < 4096);
                    let mut byte = [0];
                    stream.read_exact(&mut byte).unwrap();
                    header.push(byte[0]);
                }
                write!(stream, "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            });
            let job = network.fetch_json(url, limit, parse);
            let deadline = Instant::now() + Duration::from_secs(3);
            let error = loop {
                if let Some(result) = job.poll() {
                    break result.unwrap_err();
                }
                assert!(Instant::now() < deadline, "request did not finish");
                thread::sleep(Duration::from_millis(1));
            };
            server.join().unwrap();
            match (status, limit, &error) {
                (503, _, FetchError::Network(NetworkError::Http(source))) => {
                    assert_eq!(source.status().unwrap().as_u16(), 503);
                    assert!(error.source().unwrap().source().is_some());
                }
                (200, 1024, FetchError::Parse(Error::Json(source))) => {
                    assert!(source.is_syntax());
                    assert!(error.source().unwrap().source().is_some());
                }
                (200, 1, FetchError::Network(NetworkError::ResponseTooLarge { limit: 1 })) => {}
                (200, 2, FetchError::Parse(Error::NoTvChannels)) => {}
                _ => panic!("unexpected classification: {error:?}"),
            }
        }
    }

    #[test]
    fn replaced_request_cannot_publish_an_old_result() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let (tx, rx) = mpsc::sync_channel(1);
        let task = runtime.spawn(std::future::pending());
        let handle = task.abort_handle();
        let request = Request { task, rx };
        drop(request);
        assert!(
            tx.try_send(Err(NetworkError::WorkerStopped.into()))
                .is_err()
        );
        runtime.block_on(tokio::task::yield_now());
        assert!(handle.is_finished());
    }
}
