//! One optional activity request, independent of commentary reception and video playback.
use crate::{
    channels::Channel,
    services::{Job, Network, NetworkError, Stopping},
};
use std::time::{Duration, Instant};
use viewer_comments::activity::{Error, MAX_RESPONSE_BYTES, Snapshot};

const ENDPOINT: &str = "https://nx-jikkyo.tsukumijima.net/api/v1/channels";
const REFRESH: Duration = Duration::from_secs(300);
type Request = Job<Snapshot, Error>;
#[derive(Default)]
enum Acquisition {
    #[default]
    Idle,
    Fetching(Request),
    Cancelling(Stopping),
}
#[derive(Default)]
pub struct Activity {
    enabled: bool,
    acquisition: Acquisition,
    next: Option<Instant>,
    snapshot: Snapshot,
    pub dirty: bool,
}
impl Activity {
    pub fn configure(&mut self, enabled: bool) {
        if self.enabled == enabled {
            return;
        }
        self.enabled = enabled;
        self.snapshot = Snapshot::default();
        self.dirty = true;
        self.next = None;
        self.acquisition = match std::mem::take(&mut self.acquisition) {
            Acquisition::Fetching(job) => Acquisition::Cancelling(job.cancel()),
            Acquisition::Cancelling(job) => Acquisition::Cancelling(job),
            Acquisition::Idle => Acquisition::Idle,
        };
    }
    pub fn poll(&mut self, network: &Network) {
        self.poll_at(network, Instant::now(), ENDPOINT);
    }
    fn poll_at(&mut self, network: &Network, now: Instant, endpoint: &str) {
        self.acquisition = match std::mem::take(&mut self.acquisition) {
            Acquisition::Cancelling(job) if job.is_finished() => Acquisition::Idle,
            Acquisition::Fetching(job) if job.is_finished() => {
                let snapshot = match job
                    .poll()
                    .unwrap_or_else(|| Err(NetworkError::WorkerStopped.into()))
                {
                    Ok(snapshot) => snapshot,
                    Err(error) => {
                        eprintln!("Comment activity fetch failed: {error}");
                        Snapshot::default()
                    }
                };
                self.dirty |= self.snapshot != snapshot;
                self.snapshot = snapshot;
                self.next = Some(now + REFRESH);
                Acquisition::Idle
            }
            state => state,
        };
        if self.enabled
            && matches!(self.acquisition, Acquisition::Idle)
            && self.next.is_none_or(|next| now >= next)
        {
            self.acquisition = Acquisition::Fetching(network.fetch_json(
                endpoint.to_owned(),
                MAX_RESPONSE_BYTES,
                Snapshot::parse,
            ));
        }
    }
    pub fn json(&self, channels: &[Channel]) -> Result<String, serde_json::Error> {
        // Strings preserve all u64 values through QML's IEEE-754 JavaScript numbers.
        let values: Vec<Option<String>> = channels
            .iter()
            .map(|channel| {
                super::jikkyo(channel)
                    .and_then(|id| self.snapshot.get(id))
                    .map(|value| value.to_string())
            })
            .collect();
        serde_json::to_string(&values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };
    type TestResult = Result<(), Box<dyn std::error::Error>>;
    fn response(
        body: &'static str,
    ) -> Result<(String, thread::JoinHandle<std::io::Result<()>>), std::io::Error> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let url = format!("http://{}/activity", listener.local_addr()?);
        let handle = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(3);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(error) => return Err(error),
                }
            };
            stream.set_read_timeout(Some(Duration::from_secs(3)))?;
            stream.set_write_timeout(Some(Duration::from_secs(3)))?;
            let mut bytes = [0; 2048];
            let mut received = 0;
            while !bytes[..received]
                .windows(4)
                .any(|window| window == b"\r\n\r\n")
            {
                let count = stream.read(&mut bytes[received..])?;
                if count == 0 {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "incomplete or oversized request headers",
                    ));
                }
                received += count;
            }
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
        });
        Ok((url, handle))
    }
    fn wait(activity: &Activity) -> TestResult {
        let deadline = Instant::now() + Duration::from_secs(3);
        while match &activity.acquisition {
            Acquisition::Fetching(job) => !job.is_finished(),
            Acquisition::Cancelling(job) => !job.is_finished(),
            Acquisition::Idle => false,
        } {
            if Instant::now() >= deadline {
                return Err("activity worker deadline exceeded".into());
            }
            thread::sleep(Duration::from_millis(2));
        }
        Ok(())
    }
    #[test]
    fn success_is_projected_once_refresh_is_delayed_and_failure_clears_stale_values() -> TestResult
    {
        let network = Network::new()?;
        let mut activity = Activity::default();
        let now = Instant::now();
        activity.poll_at(&network, now, "unused while disabled");
        assert!(matches!(activity.acquisition, Acquisition::Idle));
        let (url, server) = response(
            r#"[{"id":"jk101","threads":[{"status":"ACTIVE","jikkyo_force":18446744073709551615}]}]"#,
        )?;
        activity.configure(true);
        activity.poll_at(&network, now, &url);
        wait(&activity)?;
        server.join().map_err(|_| "server panicked")??;
        activity.poll_at(&network, now, &url);
        assert_eq!(activity.snapshot.get(101), Some(u64::MAX));
        let channels = crate::channels::parse(br#"[{"id":999,"type":1,"name":"BS","channel":{"type":"BS"},"networkId":4,"serviceId":101}]"#)?;
        assert_eq!(activity.json(&channels)?, r#"["18446744073709551615"]"#);
        activity.dirty = false;
        activity.configure(true);
        activity.poll_at(&network, now + REFRESH - Duration::from_secs(1), &url);
        assert!(!activity.dirty);
        assert!(matches!(activity.acquisition, Acquisition::Idle));
        let (url, server) = response("invalid JSON")?;
        activity.poll_at(&network, now + REFRESH, &url);
        wait(&activity)?;
        server.join().map_err(|_| "server panicked")??;
        activity.poll_at(&network, now + REFRESH, &url);
        assert!(activity.dirty);
        assert_eq!(activity.json(&channels)?, "[null]");
        Ok(())
    }
    #[test]
    fn disable_reenable_discards_even_an_already_completed_old_result() -> TestResult {
        let network = Network::new()?;
        let mut activity = Activity::default();
        let now = Instant::now();
        let (url, server) =
            response(r#"[{"id":"jk1","threads":[{"status":"ACTIVE","jikkyo_force":99}]}]"#)?;
        activity.configure(true);
        activity.poll_at(&network, now, &url);
        wait(&activity)?;
        server.join().map_err(|_| "server panicked")??;
        activity.configure(false);
        activity.configure(true);
        assert!(matches!(activity.acquisition, Acquisition::Cancelling(_)));
        let (url, server) =
            response(r#"[{"id":"jk1","threads":[{"status":"ACTIVE","jikkyo_force":0}]}]"#)?;
        activity.poll_at(&network, now, &url);
        assert_eq!(activity.snapshot.get(1), None);
        assert!(matches!(activity.acquisition, Acquisition::Fetching(_)));
        wait(&activity)?;
        server.join().map_err(|_| "server panicked")??;
        activity.poll_at(&network, now, &url);
        assert_eq!(activity.snapshot.get(1), Some(0));
        activity.configure(false);
        activity.poll_at(&network, now + REFRESH, "unused while disabled");
        assert_eq!(activity.snapshot.get(1), None);
        assert!(matches!(activity.acquisition, Acquisition::Idle));
        Ok(())
    }
}
