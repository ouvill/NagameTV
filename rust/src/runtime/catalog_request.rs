use super::fetch::{FetchServicesError, FetchedCatalog, fetch_services};
use crate::network::NetworkTask;
use std::sync::mpsc;

/// The request owns its task and its single result slot. Cancelling drops both,
/// so a previous server's payload cannot reach a later request (including A→B→A).
#[derive(Default)]
pub(super) enum CatalogRequest {
    #[default]
    Idle,
    Loading {
        server: String,
        _task: NetworkTask,
        result: mpsc::Receiver<Result<FetchedCatalog, FetchServicesError>>,
    },
}

impl CatalogRequest {
    pub fn start(
        runtime: &tokio::runtime::Handle,
        client: reqwest::Client,
        server: String,
    ) -> Self {
        let (sender, result) = mpsc::sync_channel(1);
        let endpoint = server.clone();
        let task = runtime.spawn(async move {
            let response = fetch_services(&client, &endpoint).await;
            // A disconnected receiver means its owner cancelled the request.
            // There is exactly one producer and one result, so it cannot fill up.
            let _ = sender.try_send(response);
        });
        Self::Loading {
            server,
            _task: NetworkTask(task),
            result,
        }
    }

    #[cfg(test)]
    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading { .. })
    }

    pub fn cancel(&mut self) {
        *self = Self::Idle;
    }

    pub fn poll(&mut self) -> Option<(String, Result<FetchedCatalog, FetchServicesError>)> {
        let Self::Loading { server, result, .. } = self else {
            return None;
        };
        let response = match result.try_recv() {
            Ok(response) => response,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => Err(FetchServicesError::WorkerStopped),
        };
        let server = server.clone();
        self.cancel();
        Some((server, response))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::epg::EpgSnapshot;
    use std::sync::Arc;

    fn pending(runtime: &tokio::runtime::Runtime) -> NetworkTask {
        NetworkTask(runtime.spawn(std::future::pending()))
    }

    #[test]
    fn cancel_releases_queued_snapshot_and_aborts_task() -> Result<(), Box<dyn std::error::Error>> {
        let runtime = tokio::runtime::Builder::new_current_thread().build()?;
        let (sender, result) = mpsc::sync_channel(1);
        let snapshot = Arc::new(EpgSnapshot::default());
        let weak = Arc::downgrade(&snapshot);
        let catalog = crate::channels::build_catalog(&snapshot, 0);
        assert!(
            sender
                .try_send(Ok(FetchedCatalog { catalog, snapshot }))
                .is_ok()
        );
        let task = pending(&runtime);
        let abort = task.0.abort_handle();
        let mut request = CatalogRequest::Loading {
            server: "A".into(),
            _task: task,
            result,
        };
        request.cancel();
        assert!(!request.is_loading());
        assert!(
            weak.upgrade().is_none(),
            "cancel must release a queued EPG payload"
        );
        assert!(
            sender
                .try_send(Err(FetchServicesError::WorkerStopped))
                .is_err()
        );
        runtime.block_on(tokio::task::yield_now());
        assert!(abort.is_finished());
        Ok(())
    }

    #[test]
    fn replacement_cannot_receive_old_results_even_for_same_server()
    -> Result<(), Box<dyn std::error::Error>> {
        let runtime = tokio::runtime::Builder::new_current_thread().build()?;
        let (old_sender, old_result) = mpsc::sync_channel(1);
        let mut request = CatalogRequest::Loading {
            server: "A".into(),
            _task: pending(&runtime),
            result: old_result,
        };
        request.cancel();
        let (new_sender, new_result) = mpsc::sync_channel(1);
        request = CatalogRequest::Loading {
            server: "A".into(),
            _task: pending(&runtime),
            result: new_result,
        };
        assert!(
            old_sender
                .try_send(Err(FetchServicesError::WorkerStopped))
                .is_err()
        );
        assert!(request.poll().is_none());
        assert!(request.is_loading());
        drop(new_sender);
        assert!(matches!(
            request.poll(),
            Some((_, Err(FetchServicesError::WorkerStopped)))
        ));
        assert!(!request.is_loading());
        assert!(request.poll().is_none());
        Ok(())
    }

    #[test]
    fn completed_snapshot_is_published_only_after_acceptance()
    -> Result<(), Box<dyn std::error::Error>> {
        let runtime = tokio::runtime::Builder::new_current_thread().build()?;
        let mut public = Arc::new(EpgSnapshot::new(Vec::new(), Vec::new(), 100));
        let previous = public.clone();
        let snapshot = Arc::new(EpgSnapshot::new(Vec::new(), Vec::new(), 200));
        let catalog = crate::channels::build_catalog(&snapshot, 200);
        let (sender, result) = mpsc::sync_channel(1);
        assert!(
            sender
                .try_send(Ok(FetchedCatalog { catalog, snapshot }))
                .is_ok()
        );
        let mut request = CatalogRequest::Loading {
            server: "A".into(),
            _task: pending(&runtime),
            result,
        };
        assert_eq!(public.synced_at, 100);
        let Some((server, response)) = request.poll() else {
            return Err("queued result was not delivered".into());
        };
        assert_eq!(server, "A");
        public = response?.snapshot;
        assert_eq!(public.synced_at, 200);
        assert_eq!(
            previous.synced_at, 100,
            "existing readers keep a coherent snapshot"
        );
        assert!(!request.is_loading());
        assert!(request.poll().is_none());
        Ok(())
    }
}
