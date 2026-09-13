//! Bounded HTTP response ownership and cancellation, independent of GUI and feature state.
use super::{FetchError, NetworkError};
use std::sync::mpsc;

/// Polling consumes the current operation. Only Pending retains a handle that
/// can be polled or cancelled again; Complete proves its worker has finished.
#[must_use = "retain Pending until completion or explicit cancellation"]
pub enum Progress<P, T> {
    Pending(P),
    Complete(T),
}

impl<T: Send + 'static, E: Send + 'static> Job<T, E> {
    pub(super) fn start(
        runtime: &tokio::runtime::Handle,
        client: &reqwest::Client,
        url: String,
        limit: usize,
        parse: fn(&[u8]) -> Result<T, E>,
    ) -> Self {
        let (tx, rx) = mpsc::sync_channel(1);
        let client = client.clone();
        let task = runtime.spawn(async move {
            let result = async {
                let mut response = client
                    .get(url)
                    .send()
                    .await
                    .map_err(NetworkError::from)?
                    .error_for_status()
                    .map_err(NetworkError::from)?;
                let mut bytes = Vec::new();
                while let Some(chunk) = response.chunk().await.map_err(NetworkError::from)? {
                    if bytes.len() + chunk.len() > limit {
                        return Err(NetworkError::ResponseTooLarge { limit }.into());
                    }
                    bytes.extend_from_slice(&chunk);
                }
                // Capacity includes spare buffer space; neither value is process RSS.
                // Log only the endpoint path, never server credentials or query data.
                tracing::debug!(
                    "HTTP_JSON path={} body_bytes={} buffer_capacity_bytes={}",
                    response.url().path(),
                    bytes.len(),
                    bytes.capacity()
                );
                parse(&bytes).map_err(FetchError::Parse)
            }
            .await;
            let _ = tx.try_send(result);
        });
        Job {
            task: Task(task),
            rx,
        }
    }
}

pub struct Job<T, E> {
    task: Task,
    rx: mpsc::Receiver<Result<T, FetchError<E>>>,
}
impl<T, E> Job<T, E> {
    /// Consume the result receiver before waiting for worker cancellation.
    pub fn cancel(self) -> Stopping {
        self.task.0.abort();
        // Discard a queued result immediately; future sends fail after this point.
        drop(self.rx);
        Stopping(self.task)
    }
    #[cfg(test)]
    pub fn is_finished(&self) -> bool {
        self.task.0.is_finished()
    }
    pub fn poll(self) -> Progress<Self, Result<T, FetchError<E>>> {
        if !self.task.0.is_finished() {
            return Progress::Pending(self);
        }
        Progress::Complete(
            self.rx
                .try_recv()
                .unwrap_or_else(|_| Err(NetworkError::WorkerStopped.into())),
        )
    }
}
/// A cancelled request has no result API. Retain Pending until Complete before
/// starting its replacement: abort requests cancellation but does not await it.
pub struct Stopping(Task);
impl Stopping {
    pub fn poll(self) -> Progress<Self, ()> {
        if self.0.0.is_finished() {
            Progress::Complete(())
        } else {
            Progress::Pending(self)
        }
    }
    #[cfg(test)]
    pub fn is_finished(&self) -> bool {
        self.0.0.is_finished()
    }
}

// Own the task independently of the result channel so cancellation can discard
// a completed response while preserving the worker's completion handle.
struct Task(tokio::task::JoinHandle<()>);
impl Drop for Task {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{channels, services::Request};
    #[test]
    fn queued_result_stays_owned_until_worker_completion() -> Result<(), Box<dyn std::error::Error>>
    {
        let runtime = tokio::runtime::Builder::new_current_thread().build()?;
        let (tx, rx) = mpsc::sync_channel(1);
        tx.send(Ok::<_, FetchError<channels::Error>>(42))?;
        let job = Job {
            task: Task(runtime.spawn(async {})),
            rx,
        };
        let job = match job.poll() {
            Progress::Pending(job) => job,
            Progress::Complete(_) => panic!("a queued result does not prove worker completion"),
        };
        runtime.block_on(tokio::task::yield_now());
        match job.poll() {
            Progress::Complete(Ok(value)) => assert_eq!(value, 42),
            _ => panic!("finished job must return its result"),
        }
        assert!(
            tx.send(Ok(7)).is_err(),
            "completed job releases its receiver"
        );
        Ok(())
    }

    #[test]
    fn finished_worker_without_result_is_reported_at_the_job_boundary() -> Result<(), std::io::Error>
    {
        let runtime = tokio::runtime::Builder::new_current_thread().build()?;
        let (_tx, rx) = mpsc::sync_channel(1);
        let job: Job<(), channels::Error> = Job {
            task: Task(runtime.spawn(async {})),
            rx,
        };
        runtime.block_on(tokio::task::yield_now());
        assert!(matches!(
            job.poll(),
            Progress::Complete(Err(FetchError::Network(NetworkError::WorkerStopped)))
        ));
        Ok(())
    }
    #[test]
    fn replaced_request_cannot_publish_an_old_result() -> Result<(), std::io::Error> {
        let runtime = tokio::runtime::Builder::new_current_thread().build()?;
        let (tx, rx) = mpsc::sync_channel(1);
        let task = runtime.spawn(std::future::pending());
        let handle = task.abort_handle();
        let request = Request {
            task: Task(task),
            rx,
        };
        drop(request);
        assert!(
            tx.try_send(Err(NetworkError::WorkerStopped.into()))
                .is_err()
        );
        runtime.block_on(tokio::task::yield_now());
        assert!(handle.is_finished());
        Ok(())
    }

    #[test]
    fn cancellation_releases_queued_result_before_worker_finishes()
    -> Result<(), Box<dyn std::error::Error>> {
        let runtime = tokio::runtime::Builder::new_current_thread().build()?;
        let (tx, rx) = mpsc::sync_channel(1);
        let value = std::sync::Arc::new([0_u8; 1024]);
        let weak = std::sync::Arc::downgrade(&value);
        tx.try_send(Ok::<_, FetchError<channels::Error>>(value))?;
        let task = runtime.spawn(std::future::pending());
        let request = Job {
            task: Task(task),
            rx,
        };
        assert!(weak.upgrade().is_some());
        let stopping = request.cancel();
        assert!(
            weak.upgrade().is_none(),
            "queued response is released at cancellation"
        );
        assert!(
            !stopping.is_finished(),
            "abort is not synchronous completion"
        );
        assert!(
            tx.try_send(Err(NetworkError::WorkerStopped.into()))
                .is_err()
        );
        runtime.block_on(tokio::task::yield_now());
        assert!(stopping.is_finished());
        Ok(())
    }
}
