//! Latest desired endpoint wins; a replacement waits until old task destruction completes.
use crate::connection::{Client, State, Stopping, Subscription};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::runtime::Handle;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Could not initialize the EPG event client: {0}")]
    Client(#[source] reqwest::Error),
    #[error("EPG event task failed to stop: {0}")]
    Task(#[source] tokio::task::JoinError),
}
#[derive(Default)]
enum Phase {
    #[default]
    Idle,
    Running(Subscription),
    Stopping(Stopping),
}
#[derive(Default)]
pub struct Update {
    pub refresh: bool,
    pub failure: Option<Arc<crate::connection::Error>>,
}
#[derive(Default)]
pub struct Controller {
    desired: Option<String>,
    phase: Phase,
    client: Option<Client>,
    retry_at: Option<Instant>,
    last_failure: Option<Arc<crate::connection::Error>>,
}
impl Controller {
    pub fn configure(&mut self, endpoint: Option<String>) {
        if self.desired == endpoint {
            return;
        }
        self.desired = endpoint;
        self.last_failure = None;
        self.retry_at = None;
        if self.desired.is_none() {
            self.client = None;
        }
        self.phase = match std::mem::take(&mut self.phase) {
            Phase::Running(subscription) => Phase::Stopping(subscription.stop()),
            other => other,
        };
    }
    pub fn is_stopped(&self) -> bool {
        self.desired.is_none() && matches!(self.phase, Phase::Idle)
    }
    pub fn poll(&mut self, runtime: &Handle) -> Result<Update, Error> {
        let now = Instant::now();
        self.phase = match std::mem::take(&mut self.phase) {
            Phase::Stopping(mut stopping) => match stopping.try_finish() {
                None => Phase::Stopping(stopping),
                Some(Ok(())) => Phase::Idle,
                Some(Err(error)) => {
                    self.retry_at = Some(now + Duration::from_secs(5));
                    return Err(Error::Task(error));
                }
            },
            phase => phase,
        };
        if let Some(endpoint) = &self.desired
            && matches!(self.phase, Phase::Idle)
            && self.retry_at.is_none_or(|at| now >= at)
        {
            if self.client.is_none() {
                self.client = Some(Client::new().map_err(|error| {
                    self.retry_at = Some(now + Duration::from_secs(5));
                    Error::Client(error)
                })?);
            }
            if let Some(client) = &self.client {
                self.phase = Phase::Running(Subscription::start(runtime, client, endpoint.clone()));
            }
        }
        let mut update = Update::default();
        if let Phase::Running(subscription) = &self.phase {
            update.refresh = subscription.take_refresh();
            match subscription.state() {
                State::Retrying(error) => {
                    if self
                        .last_failure
                        .as_ref()
                        .is_none_or(|last| !Arc::ptr_eq(last, &error))
                    {
                        update.failure = Some(error.clone());
                        self.last_failure = Some(error);
                    }
                }
                State::WorkerStopped => {
                    if let Phase::Running(subscription) = std::mem::take(&mut self.phase) {
                        self.phase = Phase::Stopping(subscription.stop());
                    }
                    // The reconnect may have missed updates even if the worker panicked.
                    update.refresh = true;
                }
                _ => self.last_failure = None,
            }
        }
        Ok(update)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        time::timeout,
    };
    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[tokio::test]
    async fn rapid_changes_wait_for_termination_and_only_start_latest_endpoint() -> TestResult {
        let runtime = Handle::current();
        let mut controller = Controller::default();
        assert!(controller.is_stopped());
        assert!(!controller.poll(&runtime)?.refresh);
        assert!(controller.client.is_none());
        let a = Some("http://127.0.0.1:1/a".to_owned());
        controller.configure(a.clone());
        controller.poll(&runtime)?;
        controller.configure(Some("http://127.0.0.1:1/b".to_owned()));
        controller.configure(None);
        controller.configure(a.clone());
        // Current-thread runtime has not yielded: abort cannot have completed yet.
        for _ in 0..100 {
            assert!(!controller.poll(&runtime)?.refresh);
            assert!(matches!(controller.phase, Phase::Stopping(_)));
        }
        timeout(Duration::from_secs(2), async {
            while matches!(controller.phase, Phase::Stopping(_)) {
                tokio::task::yield_now().await;
                controller.poll(&runtime)?;
            }
            Ok::<_, Error>(())
        })
        .await??;
        assert_eq!(controller.desired, a);
        assert!(matches!(controller.phase, Phase::Running(_)));
        controller.configure(None);
        timeout(Duration::from_secs(2), async {
            while !controller.is_stopped() {
                tokio::task::yield_now().await;
                assert!(!controller.poll(&runtime)?.refresh);
            }
            Ok::<_, Error>(())
        })
        .await??;
        assert!(controller.client.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn repeated_enable_disable_releases_each_http_body() -> TestResult {
        let runtime = Handle::current();
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let endpoint = format!("http://{}/events", listener.local_addr()?);
        let mut controller = Controller::default();
        for _ in 0..10 {
            controller.configure(Some(endpoint.clone()));
            controller.poll(&runtime)?;
            let (mut socket, _) = timeout(Duration::from_secs(2), listener.accept()).await??;
            timeout(Duration::from_secs(2), async {
                let mut header = Vec::new();
                while !header.ends_with(b"\r\n\r\n") {
                    header.push(socket.read_u8().await?);
                    assert!(header.len() <= 4096);
                }
                socket
                    .write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n")
                    .await?;
                Ok::<_, std::io::Error>(())
            })
            .await??;
            timeout(Duration::from_secs(2), async {
                loop {
                    controller.poll(&runtime)?;
                    if matches!(&controller.phase, Phase::Running(subscription)
                        if matches!(subscription.state(), State::Receiving))
                    {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
                Ok::<_, Error>(())
            })
            .await??;
            controller.configure(None);
            timeout(Duration::from_secs(2), async {
                while !controller.is_stopped() {
                    assert!(!controller.poll(&runtime)?.refresh);
                    tokio::task::yield_now().await;
                }
                Ok::<_, Error>(())
            })
            .await??;
            let mut byte = [0];
            assert_eq!(
                timeout(Duration::from_secs(2), socket.read(&mut byte)).await??,
                0
            );
            assert!(controller.client.is_none());
        }
        Ok(())
    }
}
