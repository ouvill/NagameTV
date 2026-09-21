//! One connection generation, driven by the caller's existing Tokio runtime.
use crate::{Comment, Decoder, Event, MAX_MESSAGE_BYTES, MAX_THREAD_LIST_BYTES, ThreadId};
use crate::{
    retry::Failure,
    service::{Attempt, Authorized, HttpError, ResponseError},
    termination::Termination,
};
use futures_util::{FutureExt, SinkExt, StreamExt};
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};
use tokio::{runtime::Handle, sync::watch, task::JoinHandle, time::timeout};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async_with_config,
    tungstenite::{self, Message, protocol::WebSocketConfig},
};

pub const QUEUE_CAPACITY: usize = 256;
const IO_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("実況HTTP通信に失敗しました: {0}")]
    Http(#[from] HttpError),
    #[error("実況WebSocket通信に失敗しました: {0}")]
    WebSocket(#[from] tungstenite::Error),
    #[error("実況通信がタイムアウトしました")]
    Timeout(#[from] tokio::time::error::Elapsed),
    #[error("{0}")]
    Protocol(#[from] crate::Error),
    #[error("実況受信タスクが予期せず終了しました")]
    WorkerStopped,
    #[error("{0}")]
    Terminated(#[from] Termination),
}
impl Error {
    pub(crate) fn failure(&self) -> Option<Failure> {
        match self {
            Self::Http(error) => error.failure(),
            Self::WebSocket(tungstenite::Error::Http(response)) => {
                Some(ResponseError::new(response.status(), response.headers()).failure())
            }
            Self::Terminated(reason) => Some(reason.failure()),
            Self::WebSocket(_) | Self::Timeout(_) | Self::Protocol(_) | Self::WorkerStopped => {
                Some(Failure::Temporary)
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum State {
    Connecting,
    Receiving,
    Ended(Termination),
    Failed(Arc<Error>),
}

/// URLs are supplied by the adapter; ordinary tests use loopback.
/// Explicitly ignored manual tests may use caller-supplied public endpoints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoints {
    pub threads: String,
    pub comments: String,
}

pub struct Connection {
    task: Option<JoinHandle<()>>,
    comments: mpsc::Receiver<Comment>,
    state: watch::Receiver<State>,
    dropped: Arc<AtomicU64>,
}

impl Connection {
    pub(crate) fn start(runtime: &Handle, request: Authorized<Endpoints>) -> Self {
        let (tx, comments) = mpsc::sync_channel(QUEUE_CAPACITY);
        let (status, state) = watch::channel(State::Connecting);
        let dropped = Arc::new(AtomicU64::new(0));
        let counter = dropped.clone();
        let task = runtime.spawn(async move {
            let (endpoints, attempt) = request.into_parts();
            // Record failures before publishing their result. Selection changes
            // and cancellation cannot discard an already observed server limit.
            match receive(&attempt, endpoints, &tx, &status, &counter).await {
                Ok(closing) => closing.finish(&attempt, &status).await,
                Err(error) => {
                    attempt.failed(error.failure(), error.to_string());
                    status.send_replace(State::Failed(Arc::new(error)));
                }
            }
        });
        Self {
            task: Some(task),
            comments,
            state,
            dropped,
        }
    }

    pub fn state(&self) -> State {
        let state = self.state.borrow().clone();
        if matches!(state, State::Connecting | State::Receiving)
            && self.task.as_ref().is_some_and(JoinHandle::is_finished)
        {
            State::Failed(Arc::new(Error::WorkerStopped))
        } else {
            state
        }
    }

    pub fn try_next(&self) -> Option<Comment> {
        self.comments.try_recv().ok()
    }

    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Discard this generation's queued comments immediately. Wait for Stopping
    /// before starting its replacement when enforcing one active task per app.
    pub fn stop(mut self) -> Stopping {
        let task = self.task.take();
        if let Some(task) = &task {
            task.abort();
        }
        Stopping(task)
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

#[must_use = "Observe completion before starting a replacement connection"]
pub struct Stopping(Option<JoinHandle<()>>);
impl Stopping {
    /// Poll the join result without blocking the GUI. Pending retains ownership;
    /// the caller's existing poll loop retries, so no wake-up task is needed.
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
                Err(error) if !error.is_cancelled() => return Err(error),
                _ => {}
            }
        }
        Ok(())
    }
}

async fn active_thread(attempt: &Attempt) -> Result<ThreadId, Error> {
    let mut response = attempt.get().await?;
    if response
        .content_length()
        .is_some_and(|len| len > MAX_THREAD_LIST_BYTES as u64)
    {
        return Err(crate::Error::TooLarge {
            limit: MAX_THREAD_LIST_BYTES,
        }
        .into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(HttpError::from)? {
        if chunk.len() > MAX_THREAD_LIST_BYTES - bytes.len() {
            return Err(crate::Error::TooLarge {
                limit: MAX_THREAD_LIST_BYTES,
            }
            .into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(ThreadId::active_in(&bytes)?)
}

struct Closing {
    socket: WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
    reason: Termination,
}
impl Closing {
    async fn finish(mut self, attempt: &Attempt, status: &watch::Sender<State>) {
        // Commit the received signal before awaiting the close reply. Neither
        // cancellation nor a failed flush may lose a rejection or server wait.
        attempt.failed(Some(self.reason.failure()), self.reason.to_string());
        // Stop advertising uninterrupted reception as soon as the close is
        // observed, even if its acknowledgement stalls or is cancelled.
        status.send_replace(if self.reason.graceful() {
            State::Ended(self.reason)
        } else {
            State::Failed(Arc::new(Error::Terminated(self.reason)))
        });
        let _ = timeout(IO_TIMEOUT, self.socket.flush()).await;
    }
}

async fn receive(
    attempt: &Attempt,
    endpoints: Endpoints,
    comments: &mpsc::SyncSender<Comment>,
    status: &watch::Sender<State>,
    dropped: &AtomicU64,
) -> Result<Closing, Error> {
    // Bound the entire HTTP body transfer, not only response headers.
    let thread = timeout(IO_TIMEOUT, active_thread(attempt)).await??;
    // Another NX endpoint may have returned 429 during the thread-list request.
    attempt.check().map_err(HttpError::from)?;
    let config = WebSocketConfig::default()
        .read_buffer_size(8 * 1024)
        .write_buffer_size(0)
        .max_write_buffer_size(MAX_MESSAGE_BYTES)
        .max_message_size(Some(MAX_MESSAGE_BYTES))
        .max_frame_size(Some(MAX_MESSAGE_BYTES));
    let (mut socket, _) = timeout(
        IO_TIMEOUT,
        connect_async_with_config(endpoints.comments, Some(config), false),
    )
    .await??;
    timeout(
        IO_TIMEOUT,
        socket.send(Message::Text(thread.subscription()?.into())),
    )
    .await??;
    status.send_replace(State::Receiving);
    let _reception = attempt.receiving();
    let mut decoder = Decoder::default();
    while let Some(message) = socket.next().await {
        match message? {
            Message::Text(text) => {
                // main skips malformed packets. One bad comment must not discard
                // the connection; the transport already bounds frame/message size.
                if let Ok(Event::Comment(comment)) = decoder.decode(text.as_bytes()) {
                    match comments.try_send(comment) {
                        Ok(()) => {}
                        Err(mpsc::TrySendError::Full(_)) => {
                            dropped.fetch_add(1, Ordering::Relaxed);
                        }
                        Err(mpsc::TrySendError::Disconnected(_)) => {
                            return Ok(Closing {
                                socket,
                                reason: Termination::NoStatus,
                            });
                        }
                    }
                }
            }
            // Tungstenite queues automatic pong/close replies; flush them even
            // when the application never sends another text message.
            Message::Ping(_) => {
                timeout(IO_TIMEOUT, socket.flush()).await??;
            }
            Message::Close(frame) => {
                return Ok(Closing {
                    socket,
                    reason: Termination::from_close(frame),
                });
            }
            _ => {}
        }
    }
    Ok(Closing {
        socket,
        reason: Termination::NoStatus,
    })
}

#[cfg(test)]
mod tests;
