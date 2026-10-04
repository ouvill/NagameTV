//! Receive raw TS before playback filtering and publish
//! web-bml's original WebSocket messages over a private loopback connection.
use arib_b24::transport::{ServiceReceiver, ServiceUpdate};
mod delivery;
mod metadata;
mod snapshot;
#[cfg(any(test, feature = "native_tests"))]
#[path = "../../../tests/fixtures/bml/transport.rs"]
pub(crate) mod test_fixture;
use futures_util::StreamExt;
use std::{
    net::{Ipv4Addr, TcpListener},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
    },
    thread,
    time::Duration,
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc};
use tokio_tungstenite::{
    accept_hdr_async,
    tungstenite::{
        Message,
        handshake::server::{Request, Response},
        http::StatusCode,
    },
};

const PACKET_BYTES: usize = 188;
const CHUNK_BYTES: usize = 64 * 1024;
const INPUT_CAPACITY: usize = 64; // At most 4 MiB of undecoded TS.
const OUTPUT_CAPACITY: usize = 32; // Shared updates, plus at most one frozen snapshot.
const OUTPUT_RESOURCE_BYTES: usize = 64 * 1024 * 1024;
const OUTPUT_WAIT: Duration = Duration::from_millis(250);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("data broadcast decoder: {0}")]
    Decoder(#[from] arib_b24::Error),
    #[error("data broadcast worker: {0}")]
    Worker(#[from] std::io::Error),
    #[error("data broadcast worker stopped")]
    WorkerStopped,
}

#[derive(Default)]
struct Capture {
    active: AtomicBool,
    demand: AtomicU8,
    connected: AtomicBool,
    generation: AtomicU64,
    overflow: AtomicBool,
    entry: AtomicU8,
}

/// The entry component's startup policy, discovered without downloading a carousel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Entry {
    #[default]
    Unknown,
    Absent,
    Manual,
    Automatic,
}

/// Reception and browser subscription have separate lifetimes. Prefetch keeps
/// the decoder/snapshot running without allowing a browser subscriber.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Demand {
    Stopped = 0,
    Prefetch = 1,
    Display = 2,
    /// Monitor PAT/PMT for automatic startup; do not download resources.
    Monitor = 3,
}

enum Payload {
    Bytes(Vec<u8>),
    Reset,
}
struct Ingress {
    generation: u64,
    payload: Payload,
}
/// Bounded, nonblocking raw TS input for one playback reader.
#[derive(Clone)]
pub(crate) struct Tap {
    sender: mpsc::Sender<Ingress>,
    capture: Arc<Capture>,
}
impl Tap {
    pub(crate) fn active(&self) -> bool {
        self.capture.active.load(Ordering::Acquire)
    }
    fn enqueue(&self, payload: Payload) {
        if !self.active() {
            return;
        }
        let generation = self.capture.generation.load(Ordering::Acquire);
        if self
            .sender
            .try_send(Ingress {
                generation,
                payload,
            })
            .is_err()
        {
            self.capture.active.store(false, Ordering::Release);
            self.capture.overflow.store(true, Ordering::Release);
        }
    }
    pub(crate) fn submit(&self, bytes: Vec<u8>) {
        if !bytes.is_empty() {
            debug_assert!(bytes.len() <= CHUNK_BYTES);
            self.enqueue(Payload::Bytes(bytes));
        }
    }
    pub(crate) fn push(&self, bytes: &[u8]) {
        for chunk in bytes.chunks(CHUNK_BYTES) {
            if !self.active() {
                break;
            }
            self.submit(chunk.to_vec());
        }
    }
    pub(crate) fn reset(&self) {
        self.enqueue(Payload::Reset);
    }
    #[cfg(test)]
    pub(crate) fn test_pair() -> (Self, TestReceiver) {
        let (sender, receiver) = mpsc::channel(INPUT_CAPACITY);
        let capture = Arc::new(Capture::default());
        capture.active.store(true, Ordering::Release);
        (Self { sender, capture }, TestReceiver { receiver })
    }
}
#[cfg(test)]
pub(crate) struct TestReceiver {
    receiver: mpsc::Receiver<Ingress>,
}
#[cfg(test)]
impl TestReceiver {
    pub(crate) fn try_bytes(&mut self) -> Vec<u8> {
        let mut bytes = Vec::new();
        while let Ok(item) = self.receiver.try_recv() {
            if let Payload::Bytes(chunk) = item.payload {
                bytes.extend(chunk);
            }
        }
        bytes
    }
}
pub(crate) const TAP_CHUNK_BYTES: usize = CHUNK_BYTES;
enum Command {
    Configure(Demand),
    Connected(u64, mpsc::Sender<Queued>),
    Disconnected(u64),
    Shutdown,
}
struct Client {
    id: u64,
    sender: mpsc::Sender<Queued>,
    budget: Arc<Semaphore>,
}
struct Queued {
    delivery: delivery::Delivery,
    budget: OwnedSemaphorePermit,
}
#[derive(Debug, thiserror::Error)]
enum QueueError {
    #[error("outbound resource size {0} exceeds the queue budget")]
    Size(usize),
    #[error("outbound resource budget exhausted: {0}")]
    Budget(#[from] tokio::sync::TryAcquireError),
    #[error("browser message queue is full")]
    Full,
    #[error("browser disconnected")]
    Closed,
}
impl Client {
    fn queue(&self, delivery: delivery::Delivery) -> Result<(), QueueError> {
        let bytes = delivery.resource_bytes();
        let count = u32::try_from(bytes).map_err(|_| QueueError::Size(bytes))?;
        let budget = self.budget.clone().try_acquire_many_owned(count)?;
        self.sender
            .try_send(Queued { delivery, budget })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => QueueError::Full,
                mpsc::error::TrySendError::Closed(_) => QueueError::Closed,
            })
    }
}

struct Decoder {
    receiver: ServiceReceiver,
    metadata: metadata::Metadata,
    pending: [u8; PACKET_BYTES],
    pending_len: usize,
    collecting: bool,
}
enum Update {
    Broadcast(ServiceUpdate),
    Metadata(metadata::Update),
}
impl Update {
    fn resource_bytes(&self) -> usize {
        match self {
            Self::Broadcast(ServiceUpdate::Module { resources, .. }) => resources
                .iter()
                .map(|r| {
                    r.data.len()
                        + r.name.len()
                        + r.module_name.len()
                        + r.media_type.as_ref().map_or(0, Vec::len)
                })
                .sum(),
            _ => 0, // Metadata is bounded by the message count, not the resource budget.
        }
    }
}
#[derive(Clone, Copy)]
struct ServiceIdentity {
    service_id: u16,
    original_network_id: Option<u16>,
}
impl Decoder {
    fn new(service: u16, original_network_id: Option<u16>) -> Result<Self, Error> {
        Ok(Self {
            receiver: ServiceReceiver::new(service)?,
            metadata: metadata::Metadata::new(service, original_network_id),
            pending: [0; PACKET_BYTES],
            pending_len: 0,
            collecting: true,
        })
    }
    fn consume(&mut self, mut bytes: &[u8]) -> Vec<Update> {
        let mut updates = Vec::new();
        if self.pending_len != 0 {
            let count = bytes.len().min(PACKET_BYTES - self.pending_len);
            self.pending[self.pending_len..self.pending_len + count]
                .copy_from_slice(&bytes[..count]);
            self.pending_len += count;
            bytes = &bytes[count..];
            if self.pending_len < PACKET_BYTES {
                return updates;
            }
            let packet = self.pending;
            self.packet(&packet, &mut updates);
            self.pending_len = 0;
        }
        let (packets, tail) = bytes.as_chunks::<PACKET_BYTES>();
        for packet in packets {
            self.packet(packet, &mut updates);
        }
        self.pending[..tail.len()].copy_from_slice(tail);
        self.pending_len = tail.len();
        updates
    }

    fn packet(&mut self, packet: &[u8; PACKET_BYTES], updates: &mut Vec<Update>) {
        let parsed = match viewer_mpegts::TransportPacket::parse(packet) {
            Ok(parsed) => parsed,
            Err(error) => {
                tracing::warn!(
                    operation = "decode data broadcast TS header",
                    error = &error as &dyn std::error::Error
                );
                return;
            }
        };
        if !self.collecting
            && parsed.pid != viewer_mpegts::Pid::PAT
            && Some(parsed.pid.0) != self.receiver.pmt_pid()
        {
            return;
        }
        match self.receiver.push_updates(packet) {
            Ok(decoded) => updates.extend(decoded.into_iter().map(Update::Broadcast)),
            Err(error) => tracing::warn!(
                operation = "decode data broadcast TS packet",
                error = &error as &dyn std::error::Error
            ),
        }
        if !self.collecting {
            return;
        }
        updates.extend(
            self.metadata
                .push(self.receiver.program(), &parsed, packet)
                .into_iter()
                .map(Update::Metadata),
        );
    }

    fn entry(&self) -> Entry {
        if self.receiver.program().is_none() {
            return Entry::Unknown;
        }
        // The descriptor identifies an entry point on terrestrial and satellite
        // services. Do not assume every service uses terrestrial component 0x40.
        let entry = self
            .receiver
            .components()
            .find_map(|component| component.bxml_info()?.entry_point);
        match entry {
            Some(entry) if entry.auto_start => Entry::Automatic,
            Some(_) => Entry::Manual,
            None => Entry::Absent,
        }
    }
}

struct Receiving {
    generation: u64,
    epoch: u64,
    decoder: Decoder,
    snapshot: snapshot::Snapshot,
}
impl Receiving {
    fn new(identity: ServiceIdentity, generation: u64, epoch: u64) -> Result<Self, Error> {
        Ok(Self {
            generation,
            epoch,
            decoder: Decoder::new(identity.service_id, identity.original_network_id)?,
            snapshot: snapshot::Snapshot::new(identity),
        })
    }
    fn replay(&self) -> delivery::Delivery {
        delivery::Delivery::Snapshot {
            epoch: self.epoch,
            updates: self.snapshot.replay(),
        }
    }
}

enum Reception {
    Closed,
    Receiving(Box<Receiving>),
    Failed(&'static str),
}

fn publish(client: &mut Option<Client>, capture: &Capture, delivery: delivery::Delivery) {
    if let Some(current) = client {
        match current.queue(delivery) {
            Ok(()) => return,
            Err(QueueError::Closed) => tracing::debug!(
                operation = "queue data broadcast update",
                "browser disconnected"
            ),
            Err(error) => tracing::warn!(
                operation = "queue data broadcast update",
                error = &error as &dyn std::error::Error,
                "detach subscriber for resynchronization"
            ),
        }
    }
    *client = None;
    capture.connected.store(false, Ordering::Release);
}

fn check_overflow(reception: &mut Reception, client: &mut Option<Client>, capture: &Capture) {
    if capture.overflow.swap(false, Ordering::AcqRel)
        && matches!(reception, Reception::Receiving(_))
    {
        tracing::error!(
            operation = "receive data broadcast TS",
            "bounded input queue overflowed; reopen data broadcast to recover"
        );
        capture.active.store(false, Ordering::Release);
        publish(client, capture, delivery::Delivery::Fault("overflow"));
        *reception = Reception::Failed("overflow");
    }
    // An in-flight enqueue can report overflow after reception was stopped.
    // Discard that cancelled generation's notification while closed.
}

// tungstenite's handshake callback fixes the error type to an HTTP response;
// its size is imposed by that API, and the closure is used only at handshake.
#[allow(clippy::result_large_err)]
async fn connection(
    stream: tokio::net::TcpStream,
    token: String,
    id: u64,
    control: mpsc::UnboundedSender<Command>,
) {
    let path = format!("/data/{token}");
    let handshake = accept_hdr_async(stream, move |request: &Request, response: Response| {
        if request.uri().path() == path {
            Ok(response)
        } else {
            let denied = Response::builder()
                .status(StatusCode::FORBIDDEN)
                .body(Some("Forbidden".to_owned()))
                .expect("static HTTP response");
            Err(denied)
        }
    });
    let mut socket = match tokio::time::timeout(Duration::from_secs(3), handshake).await {
        Ok(Ok(socket)) => socket,
        Ok(Err(error)) => {
            tracing::warn!(
                operation = "accept data broadcast WebSocket",
                error = &error as &dyn std::error::Error
            );
            return;
        }
        Err(error) => {
            tracing::warn!(
                operation = "time out data broadcast WebSocket handshake",
                error = &error as &dyn std::error::Error
            );
            return;
        }
    };
    let (sender, mut outbound) = mpsc::channel::<Queued>(OUTPUT_CAPACITY);
    if control.send(Command::Connected(id, sender)).is_err() {
        return; // Session shutdown.
    }
    loop {
        tokio::select! {
            message = outbound.recv() => {
                let Some(message) = message else { break };
                let result = delivery::send(&mut socket, message.delivery).await;
                drop(message.budget);
                if let Err(error) = result {
                    tracing::warn!(operation = "send data broadcast WebSocket message", error = &error as &dyn std::error::Error);
                    break;
                }
            }
            message = socket.next() => match message {
                Some(Ok(Message::Close(_))) | None => break,
                Some(Ok(_)) => {}, // web-bml does not send application messages.
                Some(Err(error)) => {
                    tracing::warn!(operation = "read data broadcast WebSocket", error = &error as &dyn std::error::Error);
                    break;
                }
            }
        }
    }
    match tokio::time::timeout(OUTPUT_WAIT, socket.close(None)).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => tracing::debug!(
            operation = "close data broadcast WebSocket",
            error = &error as &dyn std::error::Error
        ),
        Err(error) => tracing::debug!(
            operation = "time out data broadcast WebSocket close",
            error = &error as &dyn std::error::Error
        ),
    }
    // The owner may have shut down; no work remains in that case.
    if control.send(Command::Disconnected(id)).is_err() {
        tracing::debug!(
            operation = "report data broadcast disconnect",
            "worker already stopped"
        );
    }
}

async fn run(
    listener: TcpListener,
    token: String,
    identity: ServiceIdentity,
    capture: Arc<Capture>,
    mut input: mpsc::Receiver<Ingress>,
    mut control: mpsc::UnboundedReceiver<Command>,
    commands: mpsc::UnboundedSender<Command>,
) -> Result<(), std::io::Error> {
    let listener = tokio::net::TcpListener::from_std(listener)?;
    let output_budget = Arc::new(Semaphore::new(OUTPUT_RESOURCE_BYTES));
    let mut reception = Reception::Closed;
    let mut demand = Demand::Stopped;
    let mut client: Option<Client> = None;
    let mut next_client = 0u64;
    let mut epoch = 0u64;
    let mut connections = tokio::task::JoinSet::new();
    let mut overflow_check = tokio::time::interval(Duration::from_millis(100));
    // Reception can be disabled for hours. Resume checking once, without
    // replaying the ticks skipped while the receiver was closed.
    overflow_check.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            biased;
            command = control.recv() => match command {
                Some(Command::Configure(next)) => {
                    // Observe a pending prefetch failure before deciding whether
                    // an explicit open can reuse or must rebuild the snapshot.
                    check_overflow(&mut reception, &mut client, &capture);
                    let previous = demand;
                    demand = next;
                    match demand {
                        Demand::Stopped => {
                            capture.active.store(false, Ordering::Release);
                            capture.overflow.store(false, Ordering::Release);
                            reception = Reception::Closed;
                            capture.entry.store(Entry::Unknown as u8, Ordering::Release);
                        }
                        Demand::Monitor | Demand::Prefetch | Demand::Display => {
                            let restart = match &reception {
                                Reception::Closed => true,
                                // Retry a failed prefetch only on an explicit open.
                                // Background reconciliation never loops on failure.
                                Reception::Failed(_) => demand == Demand::Display,
                                Reception::Receiving(_) => demand == Demand::Monitor && previous != Demand::Monitor,
                            };
                            if restart {
                                epoch = epoch.wrapping_add(1);
                                let generation = capture.generation.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
                                capture.overflow.store(false, Ordering::Release);
                                let receiving = Receiving::new(identity, generation, epoch).map_err(std::io::Error::other)?;
                                publish(&mut client, &capture, receiving.replay());
                                reception = Reception::Receiving(Box::new(receiving));
                                capture.active.store(true, Ordering::Release);
                            }
                            if let Reception::Receiving(receiving) = &mut reception {
                                receiving.decoder.collecting = demand != Demand::Monitor;
                            }
                        }
                    }
                    if demand != Demand::Display {
                        publish(&mut client, &capture, delivery::Delivery::Fault("closed"));
                        client = None;
                        capture.connected.store(false, Ordering::Release);
                    }
                }
                Some(Command::Connected(id, sender)) => {
                    client = Some(Client { id, sender, budget: output_budget.clone() });
                    if demand != Demand::Display {
                        publish(&mut client, &capture, delivery::Delivery::Fault("closed"));
                        client = None;
                        capture.connected.store(false, Ordering::Release);
                        continue;
                    }
                    capture.connected.store(true, Ordering::Release);
                    let delivery = match &reception {
                        Reception::Receiving(receiving) => receiving.replay(),
                        Reception::Closed => delivery::Delivery::Fault("closed"),
                        Reception::Failed(reason) => delivery::Delivery::Fault(reason),
                    };
                    publish(&mut client, &capture, delivery);
                }
                Some(Command::Disconnected(id)) => {
                    if client.as_ref().is_some_and(|client| client.id == id) {
                        capture.connected.store(false, Ordering::Release);
                        client = None;
                    }
                }
                Some(Command::Shutdown) | None => break,
            },
            accepted = listener.accept() => {
                match accepted {
                    Ok((stream, peer)) if peer.ip().is_loopback() => {
                        next_client = next_client.wrapping_add(1);
                        connections.spawn(connection(stream, token.clone(), next_client, commands.clone()));
                    }
                    Ok(_) => {}, // The listener is bound only to IPv4 loopback.
                    Err(error) => tracing::error!(operation = "accept data broadcast loopback socket", error = &error as &dyn std::error::Error),
                }
            }
            result = connections.join_next(), if !connections.is_empty() => {
                if let Some(Err(error)) = result {
                    tracing::error!(operation = "join data broadcast connection", error = &error as &dyn std::error::Error);
                }
            }
            item = input.recv() => {
                let Some(item) = item else { break };
                let Reception::Receiving(receiving) = &mut reception else { continue };
                if receiving.generation != item.generation { continue; }
                match item.payload {
                    Payload::Reset => {
                        epoch = epoch.wrapping_add(1);
                        **receiving = Receiving::new(identity, item.generation, epoch).map_err(std::io::Error::other)?;
                        receiving.decoder.collecting = demand != Demand::Monitor;
                        capture.entry.store(Entry::Unknown as u8, Ordering::Release);
                        publish(&mut client, &capture, receiving.replay());
                    }
                    Payload::Bytes(bytes) => {
                        let updates = receiving.decoder.consume(&bytes);
                        capture.entry.store(receiving.decoder.entry() as u8, Ordering::Release);
                        for update in updates {
                            let update = Arc::new(update);
                            if let Err(error) = receiving.snapshot.apply(update.clone()) {
                                tracing::error!(operation = "retain data broadcast snapshot", error = &error as &dyn std::error::Error);
                                publish(&mut client, &capture, delivery::Delivery::Fault("snapshot"));
                                reception = Reception::Failed("snapshot");
                                capture.active.store(false, Ordering::Release);
                                break;
                            }
                            publish(&mut client, &capture, delivery::Delivery::Update { epoch: receiving.epoch, update });
                            // Let the subscriber drain even when a file supplies TS faster than live playback.
                            tokio::task::yield_now().await;
                        }
                    }
                }
            }
            _ = overflow_check.tick(), if matches!(reception, Reception::Receiving(_)) => {
                check_overflow(&mut reception, &mut client, &capture);
            }
        }
    }
    connections.abort_all();
    while let Some(result) = connections.join_next().await {
        if let Err(error) = result
            && !error.is_cancelled()
        {
            tracing::error!(
                operation = "stop data broadcast connection",
                error = &error as &dyn std::error::Error
            );
        }
    }
    capture.connected.store(false, Ordering::Release);
    capture.active.store(false, Ordering::Release);
    Ok(())
}

/// One playback generation. The browser URL is private to this session.
pub struct Session {
    tap: Tap,
    capture: Arc<Capture>,
    control: mpsc::UnboundedSender<Command>,
    worker: Option<thread::JoinHandle<()>>,
    url: String,
}
impl Session {
    pub fn start(service: u16, original_network_id: Option<u16>) -> Result<Self, Error> {
        ServiceReceiver::new(service)?;
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let mut secret = [0u8; 16];
        getrandom::fill(&mut secret).map_err(std::io::Error::other)?;
        let token: String = secret.iter().map(|byte| format!("{byte:02x}")).collect();
        let url = format!("ws://127.0.0.1:{port}/data/{token}");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()?;
        let (sender, input) = mpsc::channel(INPUT_CAPACITY);
        let (control, commands) = mpsc::unbounded_channel();
        let capture = Arc::new(Capture::default());
        let worker_capture = capture.clone();
        let worker_control = control.clone();
        let worker = thread::Builder::new()
            .name("data-broadcast".into())
            .spawn(move || {
                if let Err(error) = runtime.block_on(run(
                    listener,
                    token,
                    ServiceIdentity {
                        service_id: service,
                        original_network_id,
                    },
                    worker_capture,
                    input,
                    commands,
                    worker_control,
                )) {
                    tracing::error!(
                        operation = "run data broadcast worker",
                        error = &error as &dyn std::error::Error
                    );
                }
            })?;
        Ok(Self {
            tap: Tap {
                sender,
                capture: capture.clone(),
            },
            capture,
            control,
            worker: Some(worker),
            url,
        })
    }
    pub(crate) fn tap(&self) -> Tap {
        self.tap.clone()
    }
    pub fn configure(&self, demand: Demand) -> Result<(), Error> {
        // Retain even a failed request so polling does not repeat its diagnostic.
        // The worker reports operational failure when it stops.
        if self.capture.demand.swap(demand as u8, Ordering::AcqRel) == demand as u8 {
            return Ok(());
        }
        if demand == Demand::Stopped {
            self.capture.active.store(false, Ordering::Release);
        }
        self.control
            .send(Command::Configure(demand))
            .map_err(|_| Error::WorkerStopped)
    }
    pub fn url(&self) -> &str {
        &self.url
    }
    pub fn connected(&self) -> bool {
        self.capture.connected.load(Ordering::Acquire)
    }
    pub fn receiving(&self) -> bool {
        self.tap.active() && self.capture.demand.load(Ordering::Acquire) != Demand::Monitor as u8
    }
    pub fn entry(&self) -> Entry {
        match self.capture.entry.load(Ordering::Acquire) {
            0 => Entry::Unknown,
            1 => Entry::Absent,
            2 => Entry::Manual,
            3 => Entry::Automatic,
            _ => unreachable!("only Entry values are stored"),
        }
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.capture.active.store(false, Ordering::Release);
        if self.control.send(Command::Shutdown).is_err() {
            tracing::debug!(
                operation = "stop data broadcast worker",
                "worker already stopped"
            );
        }
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            tracing::error!(operation = "join data broadcast worker", "worker panicked");
        }
    }
}

#[cfg(test)]
mod tests;
