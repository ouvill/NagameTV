//! One bounded event subscription, using the caller's existing Tokio runtime.
use crate::{Decoder, RefreshGate};
use futures_util::{FutureExt, StreamExt};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{runtime::Handle, sync::watch, task::JoinHandle};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("EPG event HTTP failure: {0}")]
    Http(#[source] reqwest::Error),
    #[error("Timed out waiting for EPG event response headers")]
    HeadersTimeout,
    #[error("EPG event stream ended")]
    Ended,
    #[error("{0}")]
    Decode(#[from] crate::Error),
}
#[derive(Clone, Debug)]
pub enum State {
    Connecting,
    Receiving,
    Retrying(Arc<Error>),
    WorkerStopped,
}

/// This client intentionally has no total/read timeout: quiet event bodies stay open.
/// A distinct type prevents passing the finite-JSON client's total timeout by mistake.
#[derive(Clone)]
pub struct Client(reqwest::Client);
impl Client {
    pub fn new() -> Result<Self, reqwest::Error> {
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .pool_max_idle_per_host(1)
            .build()
            .map(Self)
    }
}
#[derive(Clone, Copy)]
struct Timing {
    headers: Duration,
    retry: Duration,
    tick: Duration,
    refresh: Duration,
}
const TIMING: Timing = Timing {
    headers: Duration::from_secs(10),
    retry: Duration::from_secs(5),
    tick: Duration::from_secs(1),
    refresh: Duration::from_secs(60),
};

pub struct Subscription {
    task: Option<JoinHandle<()>>,
    pending: Arc<AtomicBool>,
    state: watch::Receiver<State>,
}
impl Subscription {
    pub fn start(runtime: &Handle, client: &Client, url: String) -> Self {
        Self::start_with(runtime, client, url, TIMING)
    }
    fn start_with(runtime: &Handle, client: &Client, url: String, timing: Timing) -> Self {
        let pending = Arc::new(AtomicBool::new(false));
        let output = pending.clone();
        let (state_tx, state) = watch::channel(State::Connecting);
        let client = client.0.clone();
        let task = runtime.spawn(async move { run(client, url, output, state_tx, timing).await });
        Self {
            task: Some(task),
            pending,
            state,
        }
    }
    /// All notifications for this generation occupy one bit, never a growing queue.
    pub fn take_refresh(&self) -> bool {
        self.pending.swap(false, Ordering::AcqRel)
    }
    pub fn state(&self) -> State {
        if self.task.as_ref().is_none_or(JoinHandle::is_finished) {
            State::WorkerStopped
        } else {
            self.state.borrow().clone()
        }
    }
    /// Consuming the subscription makes its old notifications inaccessible immediately.
    pub fn stop(mut self) -> Stopping {
        let task = self.task.take();
        if let Some(task) = &task {
            task.abort();
        }
        Stopping(task)
    }
}
impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}
#[must_use = "Wait for termination before starting a replacement subscription"]
pub struct Stopping(Option<JoinHandle<()>>);
impl Stopping {
    /// Nonblocking completion for a GUI-driven poll loop. Pending retains ownership.
    pub fn try_finish(&mut self) -> Option<Result<(), tokio::task::JoinError>> {
        let Some(task) = self.0.as_mut() else {
            return Some(Ok(()));
        };
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        match task.poll_unpin(&mut context) {
            std::task::Poll::Pending => None,
            std::task::Poll::Ready(result) => {
                self.0 = None;
                Some(match result {
                    Err(error) if error.is_cancelled() => Ok(()),
                    other => other,
                })
            }
        }
    }
    pub fn is_finished(&self) -> bool {
        self.0.as_ref().is_none_or(JoinHandle::is_finished)
    }
    pub async fn wait(mut self) -> Result<(), tokio::task::JoinError> {
        if let Some(task) = self.0.take() {
            match task.await {
                Err(error) if error.is_cancelled() => Ok(()),
                result => result,
            }
        } else {
            Ok(())
        }
    }
}
impl Drop for Stopping {
    fn drop(&mut self) {
        if let Some(task) = &self.0 {
            task.abort();
        }
    }
}

async fn run(
    client: reqwest::Client,
    url: String,
    pending: Arc<AtomicBool>,
    state: watch::Sender<State>,
    timing: Timing,
) {
    let mut gate = RefreshGate::with_interval(Instant::now(), timing.refresh);
    loop {
        state.send_replace(State::Connecting);
        let error = receive(&client, &url, &pending, &state, &mut gate, timing).await;
        // Reconcile changes lost on EOF, malformed data, or a failed reconnect.
        gate.changed();
        state.send_replace(State::Retrying(Arc::new(error)));
        let retry = tokio::time::sleep(timing.retry);
        tokio::pin!(retry);
        loop {
            flush(&mut gate, &pending);
            tokio::select! {
                _ = &mut retry => break,
                _ = tokio::time::sleep(timing.tick) => {}
            }
        }
    }
}
fn flush(gate: &mut RefreshGate, pending: &AtomicBool) {
    if gate.take_due(Instant::now()) {
        pending.store(true, Ordering::Release);
    }
}
async fn receive(
    client: &reqwest::Client,
    url: &str,
    pending: &AtomicBool,
    state: &watch::Sender<State>,
    gate: &mut RefreshGate,
    timing: Timing,
) -> Error {
    let response = match tokio::time::timeout(timing.headers, client.get(url).send()).await {
        Err(_) => return Error::HeadersTimeout,
        Ok(Err(error)) => return Error::Http(error.without_url()),
        Ok(Ok(response)) => match response.error_for_status() {
            Ok(response) => response,
            Err(error) => return Error::Http(error.without_url()),
        },
    };
    state.send_replace(State::Receiving);
    let mut body = response.bytes_stream();
    let mut decoder = Decoder::default();
    let mut tick = tokio::time::interval(timing.tick);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        flush(gate, pending);
        tokio::select! {
            _ = tick.tick() => {},
            chunk = body.next() => match chunk {
                None => return Error::Ended,
                Some(Err(error)) => return Error::Http(error.without_url()),
                Some(Ok(bytes)) => match decoder.push(&bytes) {
                    Ok(true) => gate.changed(),
                    Ok(false) => {},
                    Err(error) => return Error::Decode(error),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
        time::timeout,
    };
    type TestResult = Result<(), Box<dyn std::error::Error>>;
    #[tokio::test]
    #[ignore = "manual real-server subscription; requires MIRAKURUN_EVENT_URL"]
    async fn real_server_subscription_stays_open_and_stops() -> TestResult {
        let url = std::env::var("MIRAKURUN_EVENT_URL")?;
        let client = Client::new()?;
        let subscription = Subscription::start(&Handle::current(), &client, url);
        // Observe beyond the production 60-second refresh gate. A quiet server
        // may still produce zero refreshes; report the count rather than inventing events.
        let deadline = Instant::now() + Duration::from_secs(90);
        let mut receiving = false;
        let mut refreshes = 0;
        let mut failure = None;
        while Instant::now() < deadline {
            match subscription.state() {
                State::Receiving => receiving = true,
                State::Connecting => {}
                State::Retrying(error) => {
                    failure = Some(error.to_string());
                    break;
                }
                State::WorkerStopped => {
                    failure = Some("subscription worker stopped".into());
                    break;
                }
            }
            refreshes += usize::from(subscription.take_refresh());
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        // Always await cancellation before evaluating the observation result.
        timeout(Duration::from_secs(5), subscription.stop().wait()).await??;
        if let Some(failure) = failure {
            return Err(failure.into());
        }
        assert!(receiving, "server never supplied response headers");
        eprintln!(
            "REAL_EPG_SUBSCRIPTION receiving=true refresh_notifications={refreshes} stopped=true"
        );
        Ok(())
    }

    const TEST_TIMING: Timing = Timing {
        headers: Duration::from_millis(200),
        retry: Duration::from_millis(20),
        tick: Duration::from_millis(5),
        refresh: Duration::from_millis(40),
    };
    async fn request(listener: &TcpListener) -> std::io::Result<TcpStream> {
        let (mut socket, _) = listener.accept().await?;
        let mut bytes = [0; 2048];
        let mut used = 0;
        while !bytes[..used].windows(4).any(|part| part == b"\r\n\r\n") {
            let read = socket.read(&mut bytes[used..]).await?;
            if read == 0 {
                return Err(std::io::ErrorKind::UnexpectedEof.into());
            }
            used += read;
        }
        Ok(socket)
    }
    async fn headers(socket: &mut TcpStream) -> std::io::Result<()> {
        socket.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Type: application/json\r\n\r\n").await
    }
    async fn chunk(socket: &mut TcpStream, bytes: &[u8]) -> std::io::Result<()> {
        socket
            .write_all(format!("{:x}\r\n", bytes.len()).as_bytes())
            .await?;
        socket.write_all(bytes).await?;
        socket.write_all(b"\r\n").await
    }
    async fn refreshed(subscription: &Subscription) -> Result<(), tokio::time::error::Elapsed> {
        timeout(Duration::from_secs(2), async {
            while !subscription.take_refresh() {
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
    }
    #[tokio::test]
    async fn quiet_body_outlives_header_deadline_and_stop_releases_socket() -> TestResult {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let url = format!("http://{}/events", listener.local_addr()?);
        let server = tokio::spawn(async move {
            let mut socket = request(&listener).await?;
            headers(&mut socket).await?;
            chunk(&mut socket, br#"[{"resource":"program","type":"update"},"#).await?;
            let mut byte = [0];
            socket.read(&mut byte).await
        });
        let subscription =
            Subscription::start_with(&Handle::current(), &Client::new()?, url, TEST_TIMING);
        refreshed(&subscription).await?;
        assert!(!subscription.take_refresh());
        tokio::time::sleep(TEST_TIMING.headers * 2).await;
        assert!(matches!(subscription.state(), State::Receiving));
        assert!(!subscription.take_refresh());
        timeout(Duration::from_secs(2), subscription.stop().wait()).await??;
        assert_eq!(timeout(Duration::from_secs(2), server).await???, 0);
        Ok(())
    }
    #[tokio::test]
    async fn reconnect_without_new_events_still_reconciles_missed_changes() -> TestResult {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let url = format!("http://{}/events", listener.local_addr()?);
        let (connected, ready) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let mut first = request(&listener).await?;
            headers(&mut first).await?;
            first.write_all(b"0\r\n\r\n").await?;
            drop(first);
            let mut second = request(&listener).await?;
            headers(&mut second).await?;
            chunk(&mut second, b"[\n").await?;
            let _ = connected.send(());
            let mut byte = [0];
            second.read(&mut byte).await
        });
        let subscription =
            Subscription::start_with(&Handle::current(), &Client::new()?, url, TEST_TIMING);
        timeout(Duration::from_secs(2), ready).await??;
        refreshed(&subscription).await?;
        assert!(!subscription.take_refresh());
        timeout(Duration::from_secs(2), subscription.stop().wait()).await??;
        assert_eq!(timeout(Duration::from_secs(2), server).await???, 0);
        Ok(())
    }
    #[tokio::test]
    async fn header_timeout_and_cancellation_both_release_the_waiting_connection() -> TestResult {
        for cancel in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").await?;
            let url = format!("http://{}/events", listener.local_addr()?);
            let (connected, ready) = tokio::sync::oneshot::channel();
            let server = tokio::spawn(async move {
                let mut socket = request(&listener).await?;
                let _ = connected.send(());
                let mut byte = [0];
                socket.read(&mut byte).await
            });
            let timing = Timing {
                retry: Duration::from_secs(30),
                ..TEST_TIMING
            };
            let subscription =
                Subscription::start_with(&Handle::current(), &Client::new()?, url, timing);
            timeout(Duration::from_secs(2), ready).await??;
            if !cancel {
                timeout(Duration::from_secs(2), async {
                    loop {
                        if let State::Retrying(error) = subscription.state() {
                            assert!(matches!(*error, Error::HeadersTimeout));
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(2)).await;
                    }
                })
                .await?;
            }
            timeout(Duration::from_secs(2), subscription.stop().wait()).await??;
            assert_eq!(timeout(Duration::from_secs(2), server).await???, 0);
        }
        Ok(())
    }
}
