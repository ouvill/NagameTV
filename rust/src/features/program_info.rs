//! EPG acquisition lives independently of playback and of the guide's visibility.
use crate::services::{FetchError, Job, Network, NetworkError};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

const MAX_RESPONSE: usize = 32 * 1024 * 1024;
const MAX_PROGRAMS: usize = 50_000;
const REFRESH: Duration = Duration::from_secs(300);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("番組JSONの解析失敗: {0}")]
    Json(#[from] serde_json::Error),
    #[error("番組数が上限を超えています（{actual} > {limit}）")]
    TooManyPrograms { actual: usize, limit: usize },
}

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
fn parse(bytes: &[u8]) -> Result<Vec<Program>, Error> {
    let mut entries: Vec<Program> = serde_json::from_slice(bytes)?;
    if entries.len() > MAX_PROGRAMS {
        return Err(Error::TooManyPrograms {
            actual: entries.len(),
            limit: MAX_PROGRAMS,
        });
    }
    entries.sort_unstable_by_key(|p| (p.start_at, p.id));
    entries.dedup_by_key(|p| (p.start_at, p.id));
    Ok(entries)
}

fn record_storage(entries: &[Program], capacity: usize) {
    // Excludes allocator metadata and transient HTTP/parser allocations.
    let record_bytes = capacity * std::mem::size_of::<Program>();
    let string_bytes: usize = entries
        .iter()
        .map(|p| p.name.capacity() + p.description.capacity())
        .sum();
    eprintln!(
        "EPG_MEMORY programs={} record_capacity_bytes={record_bytes} string_capacity_bytes={string_bytes} snapshot_capacity_bytes={}",
        entries.len(),
        record_bytes + string_bytes
    );
}

type Request = Job<Vec<Program>, Error>;
type RequestError = FetchError<Error>;

#[derive(Default)]
enum Outcome {
    #[default]
    Waiting,
    Ready,
    Failed(RequestError),
}

// Each active state owns exactly one job. Cancelling retains ownership until
// the worker finishes; its result can never be accepted as a fresh snapshot.
enum Acquisition {
    Idle {
        next: Option<Instant>,
        outcome: Outcome,
    },
    Fetching(Request),
    Cancelling(Request),
}
impl Default for Acquisition {
    fn default() -> Self {
        Self::Idle {
            next: None,
            outcome: Outcome::Waiting,
        }
    }
}

pub enum Status<'a> {
    Disabled,
    Waiting,
    Fetching,
    Cancelling,
    Ready(usize),
    Failed(&'a RequestError),
}

#[derive(Default)]
pub struct ProgramInfo {
    desired: Option<String>,
    acquisition: Acquisition,
    snapshot: Vec<Program>,
    pub revision: u64,
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
        self.acquisition = match std::mem::take(&mut self.acquisition) {
            Acquisition::Fetching(job) | Acquisition::Cancelling(job) => {
                job.cancel();
                Acquisition::Cancelling(job)
            }
            Acquisition::Idle { .. } => Acquisition::default(),
        };
    }
    pub fn refresh(&mut self) {
        if let Acquisition::Idle { next, .. } = &mut self.acquisition {
            *next = None;
        }
    }
    pub fn poll(&mut self, network: &Network) {
        self.poll_at(network, Instant::now());
    }
    fn poll_at(&mut self, network: &Network, now: Instant) {
        self.acquisition = match std::mem::take(&mut self.acquisition) {
            Acquisition::Cancelling(job) if job.is_finished() => Acquisition::default(),
            Acquisition::Fetching(job) if job.is_finished() => {
                let outcome = match job
                    .poll()
                    .unwrap_or_else(|| Err(NetworkError::WorkerStopped.into()))
                {
                    Ok(programs) => {
                        record_storage(&programs, programs.capacity());
                        self.snapshot = programs;
                        self.revision += 1;
                        Outcome::Ready
                    }
                    Err(error) => Outcome::Failed(error),
                };
                Acquisition::Idle {
                    next: Some(now + REFRESH),
                    outcome,
                }
            }
            state => state,
        };
        if let Some(server) = &self.desired
            && matches!(&self.acquisition, Acquisition::Idle { next, .. } if next.is_none_or(|deadline| now >= deadline))
        {
            self.acquisition = Acquisition::Fetching(network.fetch_json(
                format!("{server}/api/programs"),
                MAX_RESPONSE,
                parse,
            ));
        }
    }
    pub fn status(&self) -> Status<'_> {
        match &self.acquisition {
            Acquisition::Fetching(_) => Status::Fetching,
            Acquisition::Cancelling(_) => Status::Cancelling,
            Acquisition::Idle { .. } if self.desired.is_none() => Status::Disabled,
            Acquisition::Idle {
                outcome: Outcome::Waiting,
                ..
            } => Status::Waiting,
            Acquisition::Idle {
                outcome: Outcome::Ready,
                ..
            } => Status::Ready(self.snapshot.len()),
            Acquisition::Idle {
                outcome: Outcome::Failed(error),
                ..
            } => Status::Failed(error),
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
            usize::from(!matches!(self.acquisition, Acquisition::Idle { .. })),
            self.snapshot.len(),
            matches!(self.acquisition, Acquisition::Cancelling(_)),
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
    fn updates_replace_failure_retains_refresh_waits_and_disable_clears() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let count = Arc::new(AtomicUsize::new(0));
        let requests = count.clone();
        let server = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            while requests.load(Ordering::SeqCst) < 3 && Instant::now() < deadline {
                if let Ok((mut stream, _)) = listener.accept() {
                    stream
                        .set_read_timeout(Some(Duration::from_secs(1)))
                        .unwrap();
                    let mut input = [0; 4096];
                    let n = stream.read(&mut input).unwrap();
                    assert!(String::from_utf8_lossy(&input[..n]).starts_with("GET /api/programs "));
                    let request = requests.fetch_add(1, Ordering::SeqCst);
                    let valid = r#"[{"id":1,"serviceId":1024,"networkId":32096,"startAt":100,"duration":500,"name":"番組"}]"#;
                    let body = if request == 2 { "invalid JSON" } else { valid };
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
        let deadline = Instant::now() + Duration::from_secs(3);
        while !matches!(feature.status(), Status::Failed(_)) {
            assert!(Instant::now() < deadline);
            feature.poll(&network);
            thread::sleep(Duration::from_millis(1));
        }
        assert!(matches!(
            feature.status(),
            Status::Failed(FetchError::Parse(Error::Json(_)))
        ));
        assert_eq!(feature.revision, 3);
        assert!(feature.view(Some(3209601024), 100).contains("番組"));
        feature.poll_at(&network, Instant::now() + REFRESH - Duration::from_secs(1));
        assert_eq!(count.load(Ordering::SeqCst), 3);
        assert_eq!(feature.counters().0, 0);
        feature.poll_at(&network, Instant::now() + REFRESH + Duration::from_secs(1));
        assert!(matches!(feature.status(), Status::Fetching));
        feature.configure(None);
        let deadline = Instant::now() + Duration::from_secs(3);
        while !matches!(feature.status(), Status::Disabled) {
            assert!(Instant::now() < deadline);
            feature.poll(&network);
            thread::sleep(Duration::from_millis(1));
        }
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
        assert!(matches!(feature.status(), Status::Cancelling));
        let deadline = Instant::now() + Duration::from_secs(2);
        while matches!(&feature.acquisition, Acquisition::Cancelling(job) if !job.is_finished())
            && Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(1));
        }
        feature.configure(None);
        feature.poll(&network);
        assert_eq!(feature.counters(), (0, 0, false));
        assert!(matches!(feature.status(), Status::Disabled));
    }
}
