//! Own one catalog acquisition until its task has released the previous generation.
use crate::{
    channels,
    services::{FetchError, Network, Probe, Progress, ServerUrl, Stopping, VerifiedServer},
};

#[derive(Default)]
enum Phase {
    #[default]
    Idle,
    Fetching(Probe),
    Cancelling(Stopping),
}
#[derive(Default)]
pub struct Acquisition {
    phase: Phase,
    pending: Option<ServerUrl>,
}
impl Acquisition {
    pub fn request(&mut self, server: ServerUrl) {
        self.cancel();
        self.pending = Some(server);
    }
    pub fn cancel(&mut self) {
        self.pending = None;
        self.phase = match std::mem::take(&mut self.phase) {
            Phase::Fetching(job) => Phase::Cancelling(job.cancel()),
            phase => phase,
        };
    }
    pub fn is_busy(&self) -> bool {
        self.pending.is_some() || !matches!(self.phase, Phase::Idle)
    }
    pub fn poll(
        &mut self,
        network: &Network,
    ) -> Option<Result<VerifiedServer, FetchError<channels::Error>>> {
        let mut result = None;
        self.phase = match std::mem::take(&mut self.phase) {
            Phase::Cancelling(job) => match job.poll() {
                Progress::Pending(job) => Phase::Cancelling(job),
                Progress::Complete(()) => Phase::Idle,
            },
            Phase::Fetching(job) => match job.poll() {
                Progress::Pending(job) => Phase::Fetching(job),
                Progress::Complete(completed) => {
                    result = Some(completed);
                    Phase::Idle
                }
            },
            Phase::Idle => Phase::Idle,
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
    use crate::services::NetworkError;
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
        let server = ServerUrl::parse(&format!("http://{}", listener.local_addr()?))?;
        drop(listener);
        acquisition.request(server.clone());
        assert!(acquisition.poll(&network).is_none());
        let deadline = Instant::now() + Duration::from_secs(3);
        while matches!(&acquisition.phase, Phase::Fetching(job) if !job.is_finished()) {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        }
        acquisition.request(ServerUrl::parse("http://obsolete.invalid")?);
        acquisition.request(server.clone());
        assert!(matches!(acquisition.phase, Phase::Cancelling(_)));
        assert_eq!(
            acquisition.pending.as_ref().map(ServerUrl::as_str),
            Some(server.as_str())
        );
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

    #[test]
    fn cancellation_closes_partial_body_and_replacement_delivers_current_catalog()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::{
            io::{Read, Write},
            net::{TcpListener, TcpStream},
            sync::mpsc,
        };
        fn request(listener: &TcpListener) -> std::io::Result<TcpStream> {
            let deadline = Instant::now() + Duration::from_secs(3);
            let mut socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(1));
                    }
                    Err(error) => return Err(error),
                }
            };
            socket.set_read_timeout(Some(Duration::from_secs(3)))?;
            socket.set_write_timeout(Some(Duration::from_secs(3)))?;
            let mut header = Vec::new();
            while !header.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                socket.read_exact(&mut byte)?;
                header.push(byte[0]);
                if header.len() > 4096 {
                    return Err(std::io::ErrorKind::InvalidData.into());
                }
            }
            assert!(header.starts_with(b"GET /api/services "));
            Ok(socket)
        }
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let endpoint = ServerUrl::parse(&format!("http://{}", listener.local_addr()?))?;
        let (ready, started) = mpsc::sync_channel(1);
        let server = thread::spawn(move || -> std::io::Result<()> {
            let mut old = request(&listener)?;
            old.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n[")?;
            ready.send(()).map_err(|_| std::io::ErrorKind::BrokenPipe)?;
            let mut byte = [0];
            assert_eq!(old.read(&mut byte)?, 0, "cancelled partial body must close");
            let mut current = request(&listener)?;
            let body = br#"[{"id":42,"name":"Current","type":1,"networkId":3,"serviceId":20}]"#;
            write!(
                current,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )?;
            current.write_all(body)?;
            Ok(())
        });
        let network = Network::new()?;
        let mut acquisition = Acquisition::default();
        acquisition.request(endpoint.clone());
        assert!(acquisition.poll(&network).is_none());
        started.recv_timeout(Duration::from_secs(3))?;
        acquisition.cancel();
        acquisition.request(endpoint.clone());
        let deadline = Instant::now() + Duration::from_secs(3);
        let entries = loop {
            assert!(Instant::now() < deadline);
            if let Some(result) = acquisition.poll(&network) {
                break result?;
            }
            thread::sleep(Duration::from_millis(1));
        };
        assert_eq!(entries.channels().len(), 1);
        assert_eq!(entries.channels()[0].id, 42);
        assert_eq!(entries.url(), &endpoint);
        assert!(!acquisition.is_busy());
        assert!(
            acquisition.poll(&network).is_none(),
            "result is delivered only once"
        );
        server
            .join()
            .map_err(|_| "catalog test server panicked")??;
        Ok(())
    }
}
