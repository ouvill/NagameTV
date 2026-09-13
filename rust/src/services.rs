use crate::channels;
use std::time::Duration;

mod job;
pub use job::{Job, Progress, Stopping};
mod server;
pub use server::{Probe, ServerUrl, VerifiedServer};

#[derive(Debug, thiserror::Error)]
pub enum Error {
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

pub struct Network {
    runtime: tokio::runtime::Runtime,
    client: reqwest::Client,
}

impl Network {
    pub fn poll_epg_events(
        &self,
        controller: &mut viewer_epg_events::controller::Controller,
    ) -> Result<viewer_epg_events::controller::Update, viewer_epg_events::controller::Error> {
        controller.poll(self.runtime.handle())
    }

    pub fn poll_comments(
        &self,
        controller: &mut viewer_comments::controller::Controller,
    ) -> Result<Vec<viewer_comments::Comment>, viewer_comments::controller::Error> {
        controller.poll(
            self.runtime.handle(),
            &self.client,
            std::time::Instant::now(),
        )
    }

    pub fn post_comment(
        &self,
        controller: &mut viewer_comments::posting::Controller,
        text: &str,
    ) -> bool {
        controller.submit(self.runtime.handle(), text, std::time::Instant::now())
    }

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
    pub fn fetch(&self, server: &ServerUrl) -> Probe {
        Probe::new(
            server.clone(),
            self.fetch_json(
                format!("{}/api/services", server.as_str()),
                1024 * 1024,
                channels::parse,
            ),
        )
    }
    pub fn fetch_json<T: Send + 'static, E: Send + 'static>(
        &self,
        url: String,
        limit: usize,
        parse: fn(&[u8]) -> Result<T, E>,
    ) -> Job<T, E> {
        Job::start(self.runtime.handle(), &self.client, url, limit, parse)
    }
}

pub type Request = Job<Vec<channels::Channel>, channels::Error>;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_bad_urls() -> Result<(), Error> {
        for url in ["file:///tmp/a", "http://", "http://localhost/?q=1"] {
            assert!(ServerUrl::parse(url).is_err());
        }
        assert_eq!(
            ServerUrl::parse(" http://localhost:40772/ ")?.as_str(),
            "http://localhost:40772"
        );
        Ok(())
    }
    #[test]
    fn fetch_preserves_http_parse_and_capacity_failures() -> Result<(), Box<dyn std::error::Error>>
    {
        use std::error::Error as _;
        use std::{
            io::{Read, Write},
            net::TcpListener,
            thread,
            time::Instant,
        };

        let network = Network::new()?;
        for (status, body, limit) in [
            (503, "unavailable", 1024),
            (200, "not json", 1024),
            (200, "[]", 1),
            (200, "[]", 2),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0")?;
            listener.set_nonblocking(true)?;
            let url = format!("http://{}/api/services", listener.local_addr()?);
            let server = thread::spawn(move || -> std::io::Result<()> {
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
                        Err(e) => return Err(e),
                    }
                };
                stream.set_read_timeout(Some(Duration::from_secs(1)))?;
                let mut header = Vec::new();
                while !header.ends_with(b"\r\n\r\n") {
                    assert!(header.len() < 4096);
                    let mut byte = [0];
                    stream.read_exact(&mut byte)?;
                    header.push(byte[0]);
                }
                write!(
                    stream,
                    "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )?;
                Ok(())
            });
            let mut job = network.fetch_json(url, limit, channels::parse);
            let deadline = Instant::now() + Duration::from_secs(3);
            let outcome = loop {
                match job.poll() {
                    Progress::Pending(pending) => job = pending,
                    Progress::Complete(result) => break result,
                }
                assert!(Instant::now() < deadline, "request did not finish");
                thread::sleep(Duration::from_millis(1));
            };
            server.join().map_err(|_| "test server panicked")??;
            if status == 200 && limit == 2 {
                assert!(outcome?.is_empty());
                continue;
            }
            let error = outcome.err().ok_or("expected request failure")?;
            match (status, limit, &error) {
                (503, _, FetchError::Network(NetworkError::Http(source))) => {
                    assert_eq!(source.status().map(|status| status.as_u16()), Some(503));
                    assert!(error.source().and_then(|source| source.source()).is_some());
                }
                (200, 1024, FetchError::Parse(channels::Error::Json(source))) => {
                    assert!(source.is_syntax());
                    assert!(error.source().and_then(|source| source.source()).is_some());
                }
                (200, 1, FetchError::Network(NetworkError::ResponseTooLarge { limit: 1 })) => {}
                _ => panic!("unexpected classification: {error:?}"),
            }
        }
        Ok(())
    }
}
