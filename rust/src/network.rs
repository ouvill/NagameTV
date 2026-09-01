use std::time::Duration;

pub struct NetworkRuntime {
    runtime: Option<tokio::runtime::Runtime>,
    client: reqwest::Client,
}

impl NetworkRuntime {
    pub fn new() -> Result<Self, String> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("mirakurun-network")
            .enable_all()
            .build()
            .map_err(|error| format!("Could not start the network runtime: {error}"))?;
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(10))
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(4)
            .build()
            .map_err(|error| format!("Could not create the HTTP client: {error}"))?;
        Ok(Self {
            runtime: Some(runtime),
            client,
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
}

impl Drop for NetworkRuntime {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_timeout(Duration::from_secs(1));
        }
    }
}
