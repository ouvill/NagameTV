use serde::Deserialize;
use std::{collections::HashSet, sync::mpsc, time::Duration};

#[derive(Deserialize, Debug)]
pub struct Service {
    pub id: u64,
    pub name: String,
    #[serde(rename = "type")]
    kind: u32,
}

pub fn parse(bytes: &[u8]) -> Result<Vec<Service>, String> {
    let entries: Vec<Service> = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let mut seen = HashSet::new();
    let entries: Vec<_> = entries
        .into_iter()
        .filter(|s| s.kind == 1 && s.id != 0 && !s.name.is_empty() && seen.insert(s.id))
        .collect();
    if entries.is_empty() {
        return Err("TVチャンネルがありません".into());
    }
    Ok(entries)
}

pub fn server_url(value: &str) -> Result<String, String> {
    let value = value.trim().trim_end_matches('/');
    let url = reqwest::Url::parse(value).map_err(|e| e.to_string())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("http:// または https:// のサーバーURLを入力してください".into());
    }
    Ok(value.into())
}

pub struct Network {
    runtime: tokio::runtime::Runtime,
    client: reqwest::Client,
}

impl Network {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            runtime: tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .map_err(|e| e.to_string())?,
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(10))
                .pool_max_idle_per_host(1)
                .build()
                .map_err(|e| e.to_string())?,
        })
    }
    pub fn fetch(&self, server: &str) -> Request {
        self.fetch_json(format!("{server}/api/services"), 1024 * 1024, parse)
    }
    pub fn fetch_json<T: Send + 'static>(
        &self,
        url: String,
        limit: usize,
        parse: fn(&[u8]) -> Result<T, String>,
    ) -> Job<T> {
        let (tx, rx) = mpsc::sync_channel(1);
        let client = self.client.clone();
        let task = self.runtime.spawn(async move {
            let result = async {
                let mut response = client
                    .get(url)
                    .send()
                    .await
                    .map_err(|e| e.to_string())?
                    .error_for_status()
                    .map_err(|e| e.to_string())?;
                let mut bytes = Vec::new();
                while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
                    if bytes.len() + chunk.len() > limit {
                        return Err(format!("Response exceeds {limit} bytes"));
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
                parse(&bytes)
            }
            .await;
            let _ = tx.try_send(result);
        });
        Job { task, rx }
    }
}

pub type Request = Job<Vec<Service>>;

pub struct Job<T> {
    task: tokio::task::JoinHandle<()>,
    rx: mpsc::Receiver<Result<T, String>>,
}
impl<T> Job<T> {
    pub fn cancel(&self) {
        self.task.abort();
    }
    pub fn is_finished(&self) -> bool {
        self.task.is_finished()
    }
    pub fn poll(&self) -> Option<Result<T, String>> {
        match self.rx.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err("Channel worker stopped".into())),
        }
    }
}
impl<T> Drop for Job<T> {
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
    fn replaced_request_cannot_publish_an_old_result() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let (tx, rx) = mpsc::sync_channel(1);
        let task = runtime.spawn(std::future::pending());
        let handle = task.abort_handle();
        let request = Request { task, rx };
        drop(request);
        assert!(tx.try_send(Err("old server".into())).is_err());
        runtime.block_on(tokio::task::yield_now());
        assert!(handle.is_finished());
    }
}
