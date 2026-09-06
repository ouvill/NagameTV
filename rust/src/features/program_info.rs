//! EPG acquisition lives independently of playback and of the guide's visibility.
use crate::services::{Job, Network};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

const MAX_RESPONSE: usize = 32 * 1024 * 1024;
const MAX_PROGRAMS: usize = 50_000;
const REFRESH: Duration = Duration::from_secs(300);

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Program {
    id: u64,
    service_id: u64,
    network_id: u64,
    start_at: u64,
    duration: u64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
}
impl Program {
    fn service(&self) -> Option<u64> {
        self.network_id
            .checked_mul(100_000)?
            .checked_add(self.service_id)
    }
}
fn parse(bytes: &[u8]) -> Result<Vec<Program>, String> {
    let mut entries: Vec<Program> = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if entries.len() > MAX_PROGRAMS {
        return Err("番組数が上限を超えています".into());
    }
    entries.sort_unstable_by_key(|p| (p.start_at, p.id));
    entries.dedup_by_key(|p| (p.start_at, p.id));
    Ok(entries)
}

#[derive(Default)]
pub struct ProgramInfo {
    desired: Option<String>,
    job: Option<Job<Vec<Program>>>,
    stopping: bool,
    snapshot: Vec<Program>,
    next: Option<Instant>,
    pub revision: u64,
    pub status: String,
}
impl ProgramInfo {
    /// Invalidate before cancellation. Never reuse results, even for A -> B -> A.
    pub fn configure(&mut self, server: Option<String>) {
        if self.desired == server {
            return;
        }
        self.desired = server;
        self.snapshot = Vec::new();
        self.revision += 1;
        self.next = None;
        if let Some(job) = &self.job {
            job.cancel();
            self.stopping = true;
            self.status = "停止処理中".into();
        } else {
            self.status = if self.desired.is_some() {
                "取得待ち"
            } else {
                "無効"
            }
            .into();
        }
    }
    pub fn refresh(&mut self) {
        self.next = None;
    }
    pub fn poll(&mut self, network: &Network) {
        if self.stopping {
            if self.job.as_ref().is_some_and(|job| !job.is_finished()) {
                return;
            }
            self.job = None;
            self.stopping = false;
            self.status = if self.desired.is_some() {
                "取得待ち"
            } else {
                "無効"
            }
            .into();
        }
        if self.job.as_ref().is_some_and(Job::is_finished) {
            let job = self.job.take().unwrap();
            match job
                .poll()
                .unwrap_or_else(|| Err("番組情報の取得結果がありません".into()))
            {
                Ok(programs) => {
                    self.snapshot = programs;
                    self.revision += 1;
                    self.status = format!("{} 番組", self.snapshot.len());
                }
                Err(error) => self.status = format!("取得失敗: {error}"),
            }
            self.next = Some(Instant::now() + REFRESH);
        }
        if let Some(server) = &self.desired
            && self.job.is_none()
            && self.next.is_none_or(|next| Instant::now() >= next)
        {
            self.job =
                Some(network.fetch_json(format!("{server}/api/programs"), MAX_RESPONSE, parse));
            self.status = "取得中".into();
        }
    }
    /// Only project one channel and a bounded horizon while the guide is open.
    pub fn view(&self, service: Option<u64>, now_ms: u64) -> String {
        let programs: Vec<_> = self
            .snapshot
            .iter()
            .filter(|p| {
                p.service() == service
                    && p.start_at.saturating_add(p.duration) > now_ms
                    && p.start_at < now_ms.saturating_add(24 * 60 * 60 * 1000)
            })
            .take(200)
            .collect();
        serde_json::to_string(&programs).unwrap_or_else(|_| "[]".into())
    }
    pub fn counters(&self) -> (usize, usize, bool) {
        (
            usize::from(self.job.is_some()),
            self.snapshot.len(),
            self.stopping,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        thread,
    };
    #[test]
    fn disabled_makes_no_request_and_updates_replace_then_disable_clears() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let count = Arc::new(AtomicUsize::new(0));
        let requests = count.clone();
        let server = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            while requests.load(Ordering::SeqCst) < 2 && Instant::now() < deadline {
                if let Ok((mut stream, _)) = listener.accept() {
                    stream
                        .set_read_timeout(Some(Duration::from_secs(1)))
                        .unwrap();
                    let mut input = [0; 4096];
                    let n = stream.read(&mut input).unwrap();
                    assert!(String::from_utf8_lossy(&input[..n]).starts_with("GET /api/programs "));
                    requests.fetch_add(1, Ordering::SeqCst);
                    let body = r#"[{"id":1,"serviceId":1024,"networkId":32096,"startAt":100,"duration":500,"name":"番組"}]"#;
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                    .unwrap();
                } else {
                    thread::sleep(Duration::from_millis(2));
                }
            }
        });
        let network = Network::new().unwrap();
        let mut feature = ProgramInfo::default();
        for _ in 0..20 {
            feature.poll(&network);
        }
        assert_eq!(count.load(Ordering::SeqCst), 0);
        feature.configure(Some(address));
        for revision in [2, 3] {
            let deadline = Instant::now() + Duration::from_secs(3);
            while feature.revision < revision && Instant::now() < deadline {
                feature.poll(&network);
                thread::sleep(Duration::from_millis(2));
            }
            assert_eq!(feature.revision, revision);
            assert_eq!(feature.counters().1, 1);
            assert!(feature.view(Some(3209601024), 100).contains("番組"));
            feature.refresh();
        }
        feature.configure(None);
        assert_eq!(feature.counters(), (0, 0, false));
        assert_eq!(feature.view(Some(3209601024), 100), "[]");
        server.join().unwrap();
    }
    #[test]
    fn cancelled_generation_cannot_return_on_same_server() {
        let network = Network::new().unwrap();
        let mut feature = ProgramInfo::default();
        feature.configure(Some("http://127.0.0.1:1".into()));
        feature.poll(&network);
        feature.configure(None);
        feature.configure(Some("http://127.0.0.1:1".into()));
        assert!(feature.stopping);
        let deadline = Instant::now() + Duration::from_secs(2);
        while feature.job.as_ref().is_some_and(|j| !j.is_finished()) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(1));
        }
        feature.configure(None);
        feature.poll(&network);
        assert_eq!(feature.counters(), (0, 0, false));
        assert_eq!(feature.status, "無効");
    }
}
