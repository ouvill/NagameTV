//! Opt-in gRPC / gRPC-Web control. No Qt, GStreamer or hardware dependencies.
pub mod config;
pub mod model;
mod projection;
mod rpc;

pub mod proto {
    tonic::include_proto!("viewer.v1");
}
pub const FILE_DESCRIPTOR_SET: &[u8] = tonic::include_file_descriptor_set!("viewer");

use model::{Command, CommandError, State};
use std::{
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    runtime::Handle,
    sync::{mpsc, oneshot, watch},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

const QUEUE_CAPACITY: usize = 32;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(2);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3);

/// A request can only be executed after its cancellation/deadline check.
pub struct Pending {
    command: Command,
    reply: oneshot::Sender<Result<(), CommandError>>,
    deadline: Instant,
    stop: CancellationToken,
}

pub struct Executing {
    command: Command,
    reply: oneshot::Sender<Result<(), CommandError>>,
}

impl Pending {
    pub fn claim(self) -> Option<Executing> {
        if self.stop.is_cancelled() || self.reply.is_closed() {
            None
        } else if Instant::now() >= self.deadline {
            let _ = self.reply.send(Err(CommandError::Expired));
            None
        } else {
            Some(Executing {
                command: self.command,
                reply: self.reply,
            })
        }
    }
}

impl Executing {
    pub fn command(&self) -> Command {
        self.command
    }

    /// Consume exactly one execution right. Disconnection never triggers replay.
    pub fn complete(self, result: Result<(), CommandError>) {
        let _ = self.reply.send(result);
    }
}

struct Published {
    state: proto::PlayerState,
    channels: Vec<proto::Channel>,
}

/// Owns a successfully bound socket before any task is allowed to start.
pub struct Bound {
    listener: std::net::TcpListener,
}

impl Bound {
    pub fn bind(config: config::Config) -> std::io::Result<Self> {
        let listener = std::net::TcpListener::bind(config.address)?;
        listener.set_nonblocking(true)?;
        Ok(Self { listener })
    }

    pub fn address(&self) -> std::io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    pub fn start(
        self,
        runtime: &Handle,
        state: State,
        channels: Vec<model::Channel>,
    ) -> std::io::Result<Session> {
        let _entered = runtime.enter();
        let address = self.listener.local_addr()?;
        let listener = tokio::net::TcpListener::from_std(self.listener)?;
        let (commands, requests) = mpsc::channel(QUEUE_CAPACITY);
        let (published, snapshots) = watch::channel(Arc::new(Published {
            state: projection::state(&state, 1),
            channels: channels.iter().map(projection::channel).collect(),
        }));
        let stop = CancellationToken::new();
        let rpc = rpc::Rpc::new(commands, snapshots, stop.clone());
        let service = proto::player_service_server::PlayerServiceServer::new(rpc)
            .max_decoding_message_size(4096)
            .max_encoding_message_size(4 * 1024 * 1024);
        let stopping = stop.clone();
        let job = runtime.spawn(async move {
            let server = tonic::transport::Server::builder()
                .accept_http1(true)
                .max_concurrent_streams(32)
                .concurrency_limit_per_connection(32)
                .load_shed(true)
                .layer(tonic_web::GrpcWebLayer::new())
                .add_service(service)
                .serve_with_incoming_shutdown(
                    tokio_stream::wrappers::TcpListenerStream::new(listener),
                    stopping.clone().cancelled_owned(),
                );
            tokio::pin!(server);
            tokio::select! {
                result = &mut server => result.map_err(ServerError::Transport),
                () = stopping.cancelled() => {
                    tokio::time::timeout(SHUTDOWN_TIMEOUT, &mut server).await
                        .map_err(|_| ServerError::ShutdownTimeout)?
                        .map_err(ServerError::Transport)
                }
            }
        });
        Ok(Session {
            address,
            requests,
            published,
            state,
            channels,
            task: Task { stop, job },
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("remote API transport failed: {0}")]
    Transport(#[source] tonic::transport::Error),
    #[error("remote API task failed: {0}")]
    Task(#[source] tokio::task::JoinError),
    #[error("remote API shutdown timed out")]
    ShutdownTimeout,
}

struct Task {
    stop: CancellationToken,
    job: JoinHandle<Result<(), ServerError>>,
}

impl Drop for Task {
    fn drop(&mut self) {
        self.stop.cancel();
        self.job.abort();
    }
}

/// The application thread owns command consumption and snapshot publication.
pub struct Session {
    address: SocketAddr,
    requests: mpsc::Receiver<Pending>,
    published: watch::Sender<Arc<Published>>,
    state: State,
    channels: Vec<model::Channel>,
    task: Task,
}

impl Session {
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    pub fn next_request(&mut self) -> Option<Pending> {
        self.requests.try_recv().ok()
    }

    pub fn is_finished(&self) -> bool {
        self.task.job.is_finished()
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    pub fn channels(&self) -> &[model::Channel] {
        &self.channels
    }

    /// Call with a complete state; subscriptions never see a partially updated value.
    pub fn publish(&mut self, state: State, channels: Option<Vec<model::Channel>>) {
        let state_changed = self.state != state;
        let channels_changed = channels
            .as_ref()
            .is_some_and(|channels| *channels != self.channels);
        if !state_changed && !channels_changed {
            return;
        }
        self.state = state;
        if let Some(channels) = channels {
            self.channels = channels;
        }
        let old = self.published.borrow().clone();
        // Keep catalog changes observable as well, so clients can refresh ListChannels.
        let revision = old
            .state
            .revision
            .checked_add(1)
            .expect("remote revision exhausted");
        self.published.send_replace(Arc::new(Published {
            state: projection::state(&self.state, revision),
            channels: if channels_changed {
                self.channels.iter().map(projection::channel).collect()
            } else {
                old.channels.clone()
            },
        }));
    }

    /// Cancels readers and discards queued commands before beginning the join.
    pub fn stop(mut self) -> Stopping {
        self.task.stop.cancel();
        self.requests.close();
        Stopping(self.task)
    }
}

pub struct Stopping(Task);

pub enum Progress {
    Pending(Stopping),
    Complete(Result<(), ServerError>),
}

impl Stopping {
    /// Nonblocking application-thread join. No new listener is started while pending.
    pub fn poll(mut self) -> Progress {
        use std::future::Future;
        use std::pin::Pin;
        use std::task::{Context, Poll, Waker};
        let mut cx = Context::from_waker(Waker::noop());
        match Pin::new(&mut self.0.job).poll(&mut cx) {
            Poll::Pending => Progress::Pending(self),
            Poll::Ready(result) => {
                Progress::Complete(result.map_err(ServerError::Task).and_then(|r| r))
            }
        }
    }

    pub async fn wait(mut self) -> Result<(), ServerError> {
        (&mut self.0.job).await.map_err(ServerError::Task)?
    }
}

#[cfg(test)]
mod tests;
