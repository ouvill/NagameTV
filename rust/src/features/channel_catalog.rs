//! Own one catalog acquisition until its task has released the previous generation.
use crate::{
    channels,
    services::{FetchError, Network, NetworkError, Request},
};

#[derive(Default)]
enum Phase {
    #[default]
    Idle,
    Fetching(Request),
    Cancelling(Request),
}
#[derive(Default)]
pub struct Acquisition {
    phase: Phase,
    pending: Option<String>,
}
impl Acquisition {
    pub fn request(&mut self, server: String) {
        self.cancel();
        self.pending = Some(server);
    }
    pub fn cancel(&mut self) {
        self.pending = None;
        self.phase = match std::mem::take(&mut self.phase) {
            Phase::Fetching(job) => {
                job.cancel();
                Phase::Cancelling(job)
            }
            phase => phase,
        };
    }
    pub fn is_busy(&self) -> bool {
        self.pending.is_some() || !matches!(self.phase, Phase::Idle)
    }
    pub fn poll(
        &mut self,
        network: &Network,
    ) -> Option<Result<Vec<channels::Channel>, FetchError<channels::Error>>> {
        let mut result = None;
        self.phase = match std::mem::take(&mut self.phase) {
            Phase::Cancelling(job) if job.is_finished() => Phase::Idle,
            Phase::Fetching(job) if job.is_finished() => {
                result = Some(
                    job.poll()
                        .unwrap_or_else(|| Err(NetworkError::WorkerStopped.into())),
                );
                Phase::Idle
            }
            phase => phase,
        };
        if matches!(self.phase, Phase::Idle)
            && let Some(server) = self.pending.take()
        {
            self.phase = Phase::Fetching(network.fetch(&server));
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        thread,
        time::{Duration, Instant},
    };

    #[test]
    fn completed_old_result_is_discarded_and_latest_request_wins()
    -> Result<(), Box<dyn std::error::Error>> {
        let network = Network::new()?;
        let mut acquisition = Acquisition::default();
        // Closed local endpoint produces a completed transport error without hardware.
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let server = format!("http://{}", listener.local_addr()?);
        drop(listener);
        acquisition.request(server.clone());
        assert!(acquisition.poll(&network).is_none());
        let deadline = Instant::now() + Duration::from_secs(3);
        while matches!(&acquisition.phase, Phase::Fetching(job) if !job.is_finished()) {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        }
        acquisition.request("http://obsolete.invalid".into());
        acquisition.request(server.clone());
        assert!(matches!(acquisition.phase, Phase::Cancelling(_)));
        assert_eq!(acquisition.pending.as_deref(), Some(server.as_str()));
        assert!(
            acquisition.poll(&network).is_none(),
            "old completed error must not escape"
        );
        assert!(matches!(acquisition.phase, Phase::Fetching(_)));
        assert!(acquisition.pending.is_none());
        let current = loop {
            assert!(Instant::now() < deadline);
            if let Some(result) = acquisition.poll(&network) {
                break result;
            }
            thread::sleep(Duration::from_millis(1));
        };
        assert!(matches!(
            current,
            Err(FetchError::Network(NetworkError::Http(_)))
        ));
        assert!(!acquisition.is_busy());
        // Cancelling a queued request must not initiate any further work.
        acquisition.request(server.clone());
        acquisition.cancel();
        assert!(!acquisition.is_busy());
        assert!(acquisition.poll(&network).is_none());
        acquisition.request(server);
        assert!(acquisition.poll(&network).is_none());
        acquisition.cancel();
        while acquisition.is_busy() {
            assert!(Instant::now() < deadline);
            assert!(acquisition.poll(&network).is_none());
            thread::sleep(Duration::from_millis(1));
        }
        assert!(acquisition.poll(&network).is_none());
        Ok(())
    }
}
