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
use tokio_util::sync::CancellationToken;

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
    #[error(
        "EPG notifications unsupported (open-array HTTP {open_array}, SSE HTTP {sse}); using periodic refresh"
    )]
    Unsupported {
        open_array: reqwest::StatusCode,
        sse: reqwest::StatusCode,
    },
}
#[derive(Clone, Debug)]
pub enum State {
    Connecting,
    Receiving,
    Retrying(Arc<Error>),
    Unsupported(Arc<Error>),
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
    refresh: crate::gate::INTERVAL,
};

pub struct Subscription {
    task: Task,
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
        let stop = CancellationToken::new();
        let stopping = stop.clone();
        let job = runtime.spawn(async move {
            tokio::select! {
                biased;
                () = stopping.cancelled() => {},
                () = run(client, url, output, state_tx, timing) => {},
            }
        });
        Self {
            task: Task { stop, job },
            pending,
            state,
        }
    }
    /// All notifications for this generation occupy one bit, never a growing queue.
    pub fn take_refresh(&self) -> bool {
        self.pending.swap(false, Ordering::AcqRel)
    }
    pub fn state(&self) -> State {
        let state = self.state.borrow().clone();
        if matches!(state, State::Unsupported(_)) {
            state
        } else if self.task.job.is_finished() {
            State::WorkerStopped
        } else {
            state
        }
    }
    /// Consuming the subscription makes its old notifications inaccessible immediately.
    pub fn stop(self) -> Stopping {
        self.task.stop.cancel();
        Stopping(Some(self.task))
    }
}

// Explicit stop cooperates with the worker; abandoning either the subscription
// or its stopping handle also aborts it. Keep the handle owned across await so
// dropping the wait future cannot detach a still-running task.
struct Task {
    stop: CancellationToken,
    job: JoinHandle<()>,
}
impl Drop for Task {
    fn drop(&mut self) {
        self.stop.cancel();
        self.job.abort();
    }
}
#[must_use = "Wait for termination before starting a replacement subscription"]
pub struct Stopping(Option<Task>);
impl Stopping {
    /// Nonblocking completion for a GUI-driven poll loop. Pending retains ownership.
    pub fn try_finish(&mut self) -> Option<Result<(), tokio::task::JoinError>> {
        let Some(task) = self.0.as_mut() else {
            return Some(Ok(()));
        };
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        match task.job.poll_unpin(&mut context) {
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
        self.0.as_ref().is_none_or(|task| task.job.is_finished())
    }
    pub async fn wait(mut self) -> Result<(), tokio::task::JoinError> {
        if let Some(task) = self.0.as_mut() {
            match (&mut task.job).await {
                Err(error) if error.is_cancelled() => Ok(()),
                result => result,
            }
        } else {
            Ok(())
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
    let mut endpoint = Endpoint::OpenArray(url);
    loop {
        state.send_replace(State::Connecting);
        let error = match receive(&client, &endpoint, &pending, &state, &mut gate, timing).await {
            Received::Missing { status, mut url } => match endpoint {
                Endpoint::OpenArray(_) => {
                    let prefix = url.path().strip_suffix("/api/events/stream").unwrap_or("");
                    url.set_path(&format!("{prefix}/events"));
                    url.set_query(None);
                    endpoint = Endpoint::Sse {
                        url: url.into(),
                        open_array_status: status,
                    };
                    continue;
                }
                Endpoint::Sse {
                    open_array_status, ..
                } => {
                    pending.store(false, Ordering::Release);
                    state.send_replace(State::Unsupported(Arc::new(Error::Unsupported {
                        open_array: open_array_status,
                        sse: status,
                    })));
                    return;
                }
            },
            Received::Failed(error) => error,
        };
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

enum Endpoint {
    OpenArray(String),
    Sse {
        url: String,
        open_array_status: reqwest::StatusCode,
    },
}
impl Endpoint {
    fn url(&self) -> &str {
        match self {
            Self::OpenArray(url) | Self::Sse { url, .. } => url,
        }
    }
    fn decoder(&self) -> EventDecoder {
        match self {
            Self::OpenArray(_) => EventDecoder::OpenArray(Decoder::default()),
            Self::Sse { .. } => EventDecoder::Sse(crate::sse::Decoder::default()),
        }
    }
}
enum EventDecoder {
    OpenArray(Decoder),
    Sse(crate::sse::Decoder),
}
impl EventDecoder {
    fn push(&mut self, bytes: &[u8]) -> Result<bool, crate::Error> {
        match self {
            Self::OpenArray(decoder) => decoder.push(bytes),
            Self::Sse(decoder) => decoder.push(bytes),
        }
    }
}
enum Received {
    Missing {
        status: reqwest::StatusCode,
        url: reqwest::Url,
    },
    Failed(Error),
}
fn flush(gate: &mut RefreshGate, pending: &AtomicBool) {
    if gate.take_due(Instant::now()) {
        pending.store(true, Ordering::Release);
    }
}
async fn receive(
    client: &reqwest::Client,
    endpoint: &Endpoint,
    pending: &AtomicBool,
    state: &watch::Sender<State>,
    gate: &mut RefreshGate,
    timing: Timing,
) -> Received {
    let request = client.get(endpoint.url()).header(
        reqwest::header::ACCEPT,
        match endpoint {
            Endpoint::OpenArray(_) => "application/json",
            Endpoint::Sse { .. } => "text/event-stream",
        },
    );
    let response = match tokio::time::timeout(timing.headers, request.send()).await {
        Err(_) => return Received::Failed(Error::HeadersTimeout),
        Ok(Err(error)) => return Received::Failed(Error::Http(error.without_url())),
        Ok(Ok(response)) => response,
    };
    if matches!(
        response.status(),
        reqwest::StatusCode::NOT_FOUND | reqwest::StatusCode::METHOD_NOT_ALLOWED
    ) {
        return Received::Missing {
            status: response.status(),
            url: response.url().clone(),
        };
    }
    let response = match response.error_for_status() {
        Ok(response) => response,
        Err(error) => return Received::Failed(Error::Http(error.without_url())),
    };
    state.send_replace(State::Receiving);
    let mut body = response.bytes_stream();
    let mut decoder = endpoint.decoder();
    let mut tick = tokio::time::interval(timing.tick);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        flush(gate, pending);
        tokio::select! {
            _ = tick.tick() => {},
            chunk = body.next() => match chunk {
                None => return Received::Failed(Error::Ended),
                Some(Err(error)) => return Received::Failed(Error::Http(error.without_url())),
                Some(Ok(bytes)) => match decoder.push(&bytes) {
                    Ok(true) => gate.changed(),
                    Ok(false) => {},
                    Err(error) => return Received::Failed(Error::Decode(error)),
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

    #[tokio::test(start_paused = true)]
    async fn retry_obeys_deadline_and_stop_waits_for_worker_completion() -> TestResult {
        // Invalid URLs fail without I/O, isolating retry scheduling from sockets
        // and the OS clock. Exercise production timings without a real wait.
        let subscription =
            Subscription::start(&Handle::current(), &Client::new()?, "invalid URL".into());
        let mut state = subscription.state.clone();
        state
            .wait_for(|state| matches!(state, State::Retrying(_)))
            .await?;
        state.borrow_and_update();
        tokio::time::advance(TIMING.retry - TIMING.tick).await;
        assert!(!state.has_changed()?, "retried before its deadline");
        tokio::time::advance(TIMING.tick).await;
        state.changed().await?;
        assert!(matches!(*state.borrow_and_update(), State::Retrying(_)));

        let mut stopping = subscription.stop();
        assert!(!stopping.is_finished(), "cancellation is not completion");
        assert!(stopping.try_finish().is_none());
        stopping.wait().await?;
        tokio::time::advance(TIMING.retry).await;
        assert!(
            state.changed().await.is_err(),
            "stopped worker published another state"
        );
        Ok(())
    }

    #[tokio::test]
    async fn dropping_a_pending_stop_wait_aborts_its_owned_task() -> TestResult {
        let job = tokio::spawn(std::future::pending());
        let handle = job.abort_handle();
        let stopping = Stopping(Some(Task {
            stop: CancellationToken::new(),
            job,
        }));
        let mut wait = Box::pin(stopping.wait());
        assert!(futures_util::poll!(&mut wait).is_pending());
        drop(wait);
        tokio::task::yield_now().await;
        assert!(
            handle.is_finished(),
            "dropping the waiter detached its task"
        );
        Ok(())
    }

    #[tokio::test]
    #[ignore = "manual real-server subscription; requires NAGAMETV_EVENT_URL"]
    async fn real_server_subscription_stays_open_and_stops() -> TestResult {
        let url = std::env::var("NAGAMETV_EVENT_URL")?;
        let client = Client::new()?;
        let subscription = Subscription::start(&Handle::current(), &client, url);
        // Observe beyond the production refresh gate. A quiet server
        // may still produce zero refreshes; report the count rather than inventing events.
        let deadline = Instant::now() + Duration::from_secs(90);
        let mut receiving = false;
        let mut refreshes = 0;
        let mut failure = None;
        while Instant::now() < deadline {
            match subscription.state() {
                State::Receiving => receiving = true,
                State::Connecting => {}
                State::Retrying(error) | State::Unsupported(error) => {
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

    pub(super) const TEST_TIMING: Timing = Timing {
        headers: Duration::from_millis(200),
        retry: Duration::from_millis(20),
        tick: Duration::from_millis(5),
        refresh: Duration::from_millis(40),
    };
    async fn request(listener: &TcpListener) -> std::io::Result<TcpStream> {
        requested(listener).await.map(|(socket, _)| socket)
    }
    pub(super) async fn requested(listener: &TcpListener) -> std::io::Result<(TcpStream, String)> {
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
        Ok((socket, String::from_utf8_lossy(&bytes[..used]).into_owned()))
    }
    async fn headers(socket: &mut TcpStream) -> std::io::Result<()> {
        socket.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Type: application/json\r\n\r\n").await
    }
    pub(super) async fn chunk(socket: &mut TcpStream, bytes: &[u8]) -> std::io::Result<()> {
        socket
            .write_all(format!("{:x}\r\n", bytes.len()).as_bytes())
            .await?;
        socket.write_all(bytes).await?;
        socket.write_all(b"\r\n").await
    }
    pub(super) async fn refreshed(
        subscription: &Subscription,
    ) -> Result<(), tokio::time::error::Elapsed> {
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

#[cfg(test)]
mod compatibility_tests {
    use super::tests::{TEST_TIMING, chunk, refreshed, requested};
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        time::timeout,
    };
    type TestResult = Result<(), Box<dyn std::error::Error>>;

    async fn unavailable(socket: &mut tokio::net::TcpStream, status: u16) -> std::io::Result<()> {
        socket.write_all(format!("HTTP/1.1 {status} Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes()).await
    }

    #[tokio::test]
    async fn unsupported_array_switches_to_sse_and_reconnects_without_redetecting() -> TestResult {
        for status in [404, 405] {
            let listener = TcpListener::bind("127.0.0.1:0").await?;
            let url = format!(
                "http://{}/prefix/api/events/stream?resource=program",
                listener.local_addr()?
            );
            let (close, closed) = tokio::sync::oneshot::channel();
            let (reconnected, ready) = tokio::sync::oneshot::channel();
            let server = tokio::spawn(async move {
                let (mut socket, request) = requested(&listener).await?;
                assert!(request.starts_with("GET /prefix/api/events/stream?resource=program "));
                unavailable(&mut socket, status).await?;
                drop(socket);
                let (mut socket, request) = requested(&listener).await?;
                assert!(request.starts_with("GET /prefix/events "));
                assert!(request.contains("accept: text/event-stream"));
                socket.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Type: text/event-stream\r\n\r\n").await?;
                for _ in 0..62 {
                    chunk(
                        &mut socket,
                        b"event: epg.programs-updated\r\ndata: {\"serviceId\":3272402081}\r\n\r\n",
                    )
                    .await?;
                }
                closed.await.map_err(std::io::Error::other)?;
                socket.write_all(b"0\r\n\r\n").await?;
                drop(socket);
                let (mut socket, request) = requested(&listener).await?;
                assert!(request.starts_with("GET /prefix/events "));
                socket.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Type: text/event-stream\r\n\r\n").await?;
                chunk(&mut socket, b": quiet\n\n").await?;
                reconnected
                    .send(())
                    .map_err(|_| std::io::Error::other("test stopped before reconnect"))?;
                let mut byte = [0];
                socket.read(&mut byte).await
            });
            let subscription =
                Subscription::start_with(&Handle::current(), &Client::new()?, url, TEST_TIMING);
            refreshed(&subscription).await?;
            tokio::time::sleep(TEST_TIMING.headers * 2).await;
            assert!(matches!(subscription.state(), State::Receiving));
            assert!(
                !subscription.take_refresh(),
                "connect burst should produce one refresh"
            );
            close.send(()).map_err(|_| "server stopped")?;
            timeout(Duration::from_secs(2), ready).await??;
            refreshed(&subscription).await?;
            timeout(Duration::from_secs(2), subscription.stop().wait()).await??;
            assert_eq!(timeout(Duration::from_secs(2), server).await???, 0);
        }
        Ok(())
    }

    #[tokio::test]
    async fn temporary_and_auth_failures_retry_the_same_endpoint() -> TestResult {
        for status in [500, 401] {
            let listener = TcpListener::bind("127.0.0.1:0").await?;
            let url = format!(
                "http://{}/api/events/stream?resource=program",
                listener.local_addr()?
            );
            let server = tokio::spawn(async move {
                for _ in 0..2 {
                    let (mut socket, request) = requested(&listener).await?;
                    assert!(request.starts_with("GET /api/events/stream?resource=program "));
                    unavailable(&mut socket, status).await?;
                }
                Ok::<_, std::io::Error>(())
            });
            let subscription =
                Subscription::start_with(&Handle::current(), &Client::new()?, url, TEST_TIMING);
            timeout(Duration::from_secs(2), server).await???;
            subscription.stop().wait().await?;
        }
        Ok(())
    }

    #[tokio::test]
    async fn unsupported_subscription_reports_once_and_server_change_restarts_detection()
    -> TestResult {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let base = format!("http://{}", listener.local_addr()?);
        let server = tokio::spawn(async move {
            for prefix in ["", "/new"] {
                for path in ["/api/events/stream?resource=program", "/events"] {
                    let (mut socket, request) = requested(&listener).await?;
                    assert!(request.starts_with(&format!("GET {prefix}{path} ")));
                    unavailable(&mut socket, if path == "/events" { 405 } else { 404 }).await?;
                }
            }
            // Keep the listener alive: an erroneous retry must be observable.
            assert!(
                timeout(Duration::from_millis(150), listener.accept())
                    .await
                    .is_err()
            );
            Ok::<_, std::io::Error>(())
        });
        let mut controller = crate::controller::Controller::default();
        for prefix in ["", "/new"] {
            let url = format!("{base}{prefix}/api/events/stream?resource=program");
            controller.configure(Some(url.clone()));
            timeout(Duration::from_secs(2), async {
                loop {
                    let update = controller.poll(&Handle::current())?;
                    assert!(!update.refresh);
                    if let Some(error) = update.failure {
                        assert!(matches!(*error, Error::Unsupported { .. }));
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
                Ok::<_, crate::controller::Error>(())
            })
            .await??;
            controller.configure(Some(url));
            for _ in 0..10 {
                let update = controller.poll(&Handle::current())?;
                assert!(!update.refresh);
                assert!(update.failure.is_none());
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
        timeout(Duration::from_secs(2), server).await???;
        controller.configure(None);
        timeout(Duration::from_secs(2), async {
            while !controller.is_stopped() {
                controller.poll(&Handle::current())?;
                tokio::task::yield_now().await;
            }
            Ok::<_, crate::controller::Error>(())
        })
        .await??;
        Ok(())
    }
}
