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
    client: reqwest::Client,
    epg: EpgStore,
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
        Ok(Self {
            runtime: Some(runtime),
            client,
            epg: EpgStore::default(),
        })
    }

    pub fn handle(&self) -> tokio::runtime::Handle {
        self.runtime
            .as_ref()
            .expect("network runtime is available until drop")
            .handle()
            .clone()
    }

    pub fn client(&self) -> reqwest::Client {
        self.client.clone()
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
