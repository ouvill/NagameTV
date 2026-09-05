use std::time::Duration;
use thiserror::Error;

use crate::epg::EpgStore;

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("Could not start the network runtime: {0}")]
    Runtime(#[source] std::io::Error),
    #[error("Could not create the HTTP client: {0}")]
    HttpClient(#[source] reqwest::Error),
}

pub struct NetworkRuntime {
    runtime: Option<tokio::runtime::Runtime>,
    handle: tokio::runtime::Handle,
    client: reqwest::Client,
    epg: EpgStore,
    stream_client: reqwest::Client,
}

impl NetworkRuntime {
    pub fn new() -> Result<Self, NetworkError> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("mirakurun-network")
            .enable_all()
            .build()
            .map_err(NetworkError::Runtime)?;
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(10))
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(4)
            .build()
            .map_err(NetworkError::HttpClient)?;
        // Event bodies remain open indefinitely; only connection establishment is bounded.
        let stream_client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .pool_max_idle_per_host(1)
            .build()
            .map_err(NetworkError::HttpClient)?;
        Ok(Self {
            stream_client,
            handle: runtime.handle().clone(),
            runtime: Some(runtime),
            client,
            epg: EpgStore::default(),
        })
    }

    pub fn handle(&self) -> tokio::runtime::Handle {
        self.handle.clone()
    }

    pub fn client(&self) -> reqwest::Client {
        self.client.clone()
    }

    pub fn stream_client(&self) -> reqwest::Client {
        self.stream_client.clone()
    }

    pub fn epg(&self) -> EpgStore {
        self.epg.clone()
    }
}

impl Drop for NetworkRuntime {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_timeout(Duration::from_secs(1));
        }
    }
}

/// Dropping/replacing the owner cancels socket IO, including handshakes and backoff.
pub struct NetworkTask(pub tokio::task::JoinHandle<()>);

impl Drop for NetworkTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[cfg(test)]
mod tests {
    // In tests, unwrap/expect assert successful setup or an expected result.
    // Failures intentionally fail the test; they are not assumed impossible IO.
    use super::*;
    use std::io::{Read, Write};

    #[test]
    fn event_body_survives_the_normal_request_deadline() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = [0; 4096];
            socket.read(&mut request).unwrap();
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n[")
                .unwrap();
            std::thread::sleep(Duration::from_secs(11));
            socket.write_all(b"]").unwrap();
        });
        let network = NetworkRuntime::new().unwrap();
        let client = network.stream_client();
        network.handle().block_on(async {
            let response = client.get(url).send().await.unwrap();
            let body = tokio::time::timeout(Duration::from_secs(15), response.bytes())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(&body[..], b"[]");
        });
        server.join().unwrap();
    }

    #[test]
    fn dropping_task_owner_cancels_pending_io() {
        let network = NetworkRuntime::new().unwrap();
        let task = network.handle().spawn(std::future::pending::<()>());
        let abort = task.abort_handle();
        drop(NetworkTask(task));
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while !abort.is_finished() && std::time::Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert!(abort.is_finished());
    }
}
