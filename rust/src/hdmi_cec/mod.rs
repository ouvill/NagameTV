//! Rust-owned HDMI-CEC policy and worker. No Qt types or runtime fake devices.
mod diagnostics;
mod keys;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(all(test, target_os = "linux"))]
mod tests;
use crate::features::LaunchPlan;
use diagnostics::{Diagnostics, SharedDiagnostics};
pub(crate) use keys::{Input, InputKey};
use std::{
    io,
    sync::mpsc::{self, Receiver, SyncSender, TrySendError},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub const OSD_NAME: &str = "NagameTV";
const POLL_INTERVAL: Duration = Duration::from_millis(50);
const RETRY_INTERVAL: Duration = Duration::from_secs(3);
const CLAIM_TIMEOUT: Duration = Duration::from_secs(10);
const PLAYBACK_ADDRESS_MASK: u16 = (1 << 4) | (1 << 8) | (1 << 11);
const INPUT_QUEUE_SIZE: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceInfo {
    pub path: String,
    pub name: String,
    pub driver: String,
    pub(crate) physical_address: Option<u16>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("No HDMI-CEC device found. Connect a Linux CEC adapter.")]
    NoDevice,
    #[error("Permission denied for {path}. Grant access through the video group: {source}")]
    Permission { path: String, source: io::Error },
    #[error("CEC {operation} failed ({path}): {source}")]
    Io {
        operation: &'static str,
        path: String,
        source: io::Error,
    },
    #[error("CEC adapter {0} cannot claim a Playback logical address or receive messages")]
    Capabilities(String),
    #[error("CEC adapter {0} is already configured or in use by another application")]
    Busy(String),
    #[error("CEC Playback logical address claim failed")]
    Claim,
    #[error("CEC adapter disconnected ({path}): {source}")]
    Disconnected { path: String, source: io::Error },
    #[error("HDMI-CEC is supported only on Linux")]
    Unsupported,
    #[error("CEC worker failed: {0}")]
    Worker(#[source] io::Error),
}

#[derive(Debug)]
pub enum State {
    Disabled,
    NoDevice,
    PermissionDenied(Error),
    WaitingForTv,
    Active,
    Failed(Error),
    Disconnected(Error),
    Unsupported,
}
impl State {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::NoDevice => "no-device",
            Self::PermissionDenied(_) => "permission-denied",
            Self::WaitingForTv => "waiting-for-tv",
            Self::Active => "active",
            Self::Failed(_) => "error",
            Self::Disconnected(_) => "disconnected",
            Self::Unsupported => "unsupported",
        }
    }
    pub fn message(&self) -> String {
        match self {
            Self::NoDevice => Error::NoDevice.to_string(),
            Self::PermissionDenied(e) | Self::Failed(e) | Self::Disconnected(e) => e.to_string(),
            Self::WaitingForTv => "Waiting for a valid HDMI physical address and Playback logical address. Turn on the TV and check the HDMI connection.".into(),
            Self::Unsupported => Error::Unsupported.to_string(),
            Self::Disabled | Self::Active => String::new(),
        }
    }
    fn failure(error: Error) -> Self {
        match error {
            Error::NoDevice => Self::NoDevice,
            error @ Error::Permission { .. } => Self::PermissionDenied(error),
            error @ Error::Disconnected { .. } => Self::Disconnected(error),
            Error::Unsupported => Self::Unsupported,
            error @ (Error::Io { .. }
            | Error::Capabilities(_)
            | Error::Busy(_)
            | Error::Claim
            | Error::Worker(_)) => Self::Failed(error),
        }
    }
}
#[derive(Debug)]
pub struct Snapshot {
    generation: u64,
    pub state: State,
    pub devices: Vec<DeviceInfo>,
    pub physical_address: Option<u16>,
}
impl Snapshot {
    fn new(state: State) -> Self {
        Self {
            generation: 0,
            state,
            devices: Vec::new(),
            physical_address: None,
        }
    }
}

/// The device boundary is injectable only at construction, never a runtime fallback.
trait Backend: Send + 'static {
    type Device: Device;
    fn enumerate(&mut self) -> Result<Vec<DeviceInfo>, Error>;
    fn open(&mut self, info: &DeviceInfo) -> Result<Self::Device, Error>;
}
trait Device: Send {
    fn claim_playback(&mut self) -> Result<(), Error>;
    fn address(&mut self) -> Result<Address, Error>;
    fn next_event(&mut self, timeout: Duration) -> Result<Option<Event>, Error>;
    fn release(&mut self) -> Result<(), Error>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Address {
    physical: Option<u16>,
    logical_mask: u16,
}
#[derive(Debug)]
enum Event {
    StateChange(Address),
    Press { initiator: u8, code: u8 },
    Release { initiator: u8 },
    LostMessages(u32),
}

struct Closed(DeviceInfo);
struct Opened<D: Device>(D);
struct Requested<D: Device>(Lease<D>);
struct Claimed<D: Device>(Lease<D>);
struct Lease<D: Device> {
    device: D,
    diagnostics: SharedDiagnostics,
}
impl Closed {
    fn open<B: Backend>(self, backend: &mut B) -> Result<Opened<B::Device>, Error> {
        backend.open(&self.0).map(Opened)
    }
}
impl<D: Device> Opened<D> {
    fn request(mut self, diagnostics: SharedDiagnostics) -> Result<Requested<D>, Error> {
        self.0.claim_playback()?;
        Ok(Requested(Lease {
            device: self.0,
            diagnostics,
        }))
    }
}
impl<D: Device> Requested<D> {
    fn claimed(self, address: Address) -> Result<Claimed<D>, Self> {
        if address.physical.is_some() && address.logical_mask & PLAYBACK_ADDRESS_MASK != 0 {
            Ok(Claimed(self.0))
        } else {
            Err(self)
        }
    }
}
impl<D: Device> Drop for Lease<D> {
    fn drop(&mut self) {
        let result = self.device.release();
        self.diagnostics.borrow_mut().released(result);
    }
}

enum Connection<D: Device> {
    Pending(Requested<D>),
    Active(Claimed<D>),
}
impl<D: Device> Connection<D> {
    fn device(&mut self) -> &mut D {
        match self {
            Self::Pending(device) => &mut device.0.device,
            Self::Active(device) => &mut device.0.device,
        }
    }
    fn update(self, address: Address) -> Self {
        let requested = match self {
            Self::Pending(d) => d,
            Self::Active(d) => Requested(d.0),
        };
        match requested.claimed(address) {
            Ok(d) => Self::Active(d),
            Err(d) => Self::Pending(d),
        }
    }
}

/// OFF owns no worker. Stopping joins before the UI can observe Disabled.
enum Runtime {
    Off,
    Running {
        stop: mpsc::Sender<()>,
        join: JoinHandle<()>,
    },
}
#[derive(Clone, Copy)]
struct QueuedInput {
    generation: u64,
    input: Input,
}
struct Worker {
    runtime: Runtime,
    snapshots: Receiver<Snapshot>,
    inputs: Receiver<QueuedInput>,
}
pub struct Controller {
    launcher: Box<dyn Fn(String) -> Result<Worker, Error>>,
    held_input: Option<QueuedInput>,
    allowed: bool,
    enabled: bool,
    selection: String,
    snapshot: Snapshot,
    runtime: Runtime,
    snapshots: Option<Receiver<Snapshot>>,
    inputs: Option<Receiver<QueuedInput>>,
}
impl Controller {
    pub fn new(plan: LaunchPlan, preference: bool, selection: String) -> Self {
        #[cfg(target_os = "linux")]
        {
            Self::with_backend(plan, preference, selection, linux::Linux)
        }
        #[cfg(not(target_os = "linux"))]
        {
            Self::create(
                plan,
                preference,
                selection,
                Box::new(|_| Err(Error::Unsupported)),
            )
        }
    }
    fn with_backend<B: Backend + Clone>(
        plan: LaunchPlan,
        preference: bool,
        selection: String,
        backend: B,
    ) -> Self {
        Self::create(
            plan,
            preference,
            selection,
            Box::new(move |selection| spawn_worker(backend.clone(), selection)),
        )
    }
    fn create(
        plan: LaunchPlan,
        preference: bool,
        selection: String,
        launcher: Box<dyn Fn(String) -> Result<Worker, Error>>,
    ) -> Self {
        let supported = cfg!(target_os = "linux");
        let mut controller = Self {
            launcher,
            held_input: None,
            allowed: supported && (!plan.locked() || plan.hdmi_cec(false)),
            enabled: supported && plan.hdmi_cec(preference),
            selection,
            snapshot: Snapshot::new(if supported {
                State::Disabled
            } else {
                State::Unsupported
            }),
            runtime: Runtime::Off,
            snapshots: None,
            inputs: None,
        };
        if controller.enabled {
            controller.start();
        }
        controller
    }
    pub fn allowed(&self) -> bool {
        self.allowed
    }
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    pub fn selection(&self) -> &str {
        &self.selection
    }
    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }
    pub fn enable(&mut self, enabled: bool) -> bool {
        if enabled && !self.allowed {
            return false;
        }
        if self.enabled != enabled {
            self.stop();
            self.enabled = enabled;
            if enabled {
                self.start();
            }
        }
        true
    }
    pub fn select(&mut self, selection: String) -> bool {
        if !selection.is_empty() && !valid_path(&selection) {
            return false;
        }
        if self.selection != selection {
            self.stop();
            self.selection = selection;
            if self.enabled {
                self.start();
            }
        }
        true
    }
    pub fn refresh(&mut self) {
        if self.enabled {
            self.stop();
            self.start();
        }
    }
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        let mut unexpected_exit = false;
        if let Some(receiver) = &self.snapshots {
            loop {
                match receiver.try_recv() {
                    Ok(snapshot) => {
                        self.snapshot = snapshot;
                        changed = true;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        unexpected_exit = true;
                        break;
                    }
                }
            }
        }
        if unexpected_exit {
            self.stop();
            self.snapshot.state = Diagnostics::default().failed(
                "poll worker",
                Error::Worker(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "CEC worker exited unexpectedly; search again to retry",
                )),
            );
            changed = true;
        }
        changed
    }
    pub fn next_input(&mut self) -> Option<Input> {
        loop {
            let queued = match self.held_input.take() {
                Some(input) => input,
                None => match self.inputs.as_ref()?.try_recv() {
                    Ok(input) => input,
                    // Empty and shutdown are normal, not device failures.
                    Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected) => {
                        return None;
                    }
                },
            };
            if queued.generation > self.snapshot.generation {
                // The state was published first but arrived after this UI tick's poll.
                self.held_input = Some(queued);
                return None;
            }
            if queued.generation == self.snapshot.generation
                && matches!(self.snapshot.state, State::Active)
            {
                return Some(queued.input);
            }
            // Old connection generations cannot act after disconnect/reconfiguration.
        }
    }
    fn start(&mut self) {
        self.snapshot = Snapshot::new(State::WaitingForTv);
        match (self.launcher)(self.selection.clone()) {
            Ok(worker) => {
                self.runtime = worker.runtime;
                self.snapshots = Some(worker.snapshots);
                self.inputs = Some(worker.inputs);
            }
            Err(error) => {
                self.snapshot.state = Diagnostics::default().failed("start worker", error)
            }
        }
    }
    pub fn stop(&mut self) {
        if let Runtime::Running { stop, join } = std::mem::replace(&mut self.runtime, Runtime::Off)
        {
            // A worker which already exited needs no cancellation notification.
            if stop.send(()).is_err() {
                tracing::debug!("HDMI-CEC worker already stopped");
            }
            if join.join().is_err() {
                tracing::error!("HDMI-CEC worker panicked while stopping");
            }
        }
        self.snapshots = None;
        self.inputs = None;
        self.held_input = None;
        self.snapshot = Snapshot::new(if cfg!(target_os = "linux") {
            State::Disabled
        } else {
            State::Unsupported
        });
    }
}
impl Drop for Controller {
    fn drop(&mut self) {
        self.stop();
    }
}
pub fn valid_path(path: &str) -> bool {
    path.strip_prefix("/dev/cec")
        .is_some_and(|number| !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()))
}

fn spawn_worker<B: Backend>(backend: B, selection: String) -> Result<Worker, Error> {
    let (stop, cancellation) = mpsc::channel();
    let (snapshots, receiver) = mpsc::channel();
    let (inputs, input_receiver) = mpsc::sync_channel(INPUT_QUEUE_SIZE);
    let join = thread::Builder::new()
        .name("hdmi-cec".into())
        .spawn(move || worker(backend, selection, cancellation, snapshots, inputs))
        .map_err(Error::Worker)?;
    Ok(Worker {
        runtime: Runtime::Running { stop, join },
        snapshots: receiver,
        inputs: input_receiver,
    })
}

fn worker<B: Backend>(
    mut backend: B,
    selection: String,
    stop: Receiver<()>,
    snapshots: mpsc::Sender<Snapshot>,
    inputs: SyncSender<QueuedInput>,
) {
    let mut generation = 0;
    let diagnostics = Diagnostics::shared();
    loop {
        match stop.try_recv() {
            Ok(()) | Err(mpsc::TryRecvError::Disconnected) => return,
            Err(mpsc::TryRecvError::Empty) => {}
        }
        let devices = match backend.enumerate() {
            Ok(devices) => devices,
            Err(error) => {
                generation += 1;
                let mut snapshot =
                    Snapshot::new(diagnostics.borrow_mut().failed("enumerate adapters", error));
                snapshot.generation = generation;
                if snapshots.send(snapshot).is_err() {
                    return;
                }
                if cancelled(&stop, RETRY_INTERVAL) {
                    return;
                }
                continue;
            }
        };
        let result = connect(&mut backend, &selection, &devices, diagnostics.clone());
        let (operation, error) = match result {
            Ok(device) => {
                match run_device(
                    device,
                    &devices,
                    &stop,
                    &snapshots,
                    &inputs,
                    &mut generation,
                    Instant::now,
                ) {
                    Ok(()) => return,
                    Err(error) => ("receive from adapter", error),
                }
            }
            Err(error) => ("connect adapter", error),
        };
        generation += 1;
        let mut snapshot = Snapshot::new(diagnostics.borrow_mut().failed(operation, error));
        snapshot.generation = generation;
        snapshot.devices = devices;
        // Receiver shutdown is expected while the owning Player is being destroyed.
        if snapshots.send(snapshot).is_err() || cancelled(&stop, RETRY_INTERVAL) {
            return;
        }
    }
}
fn cancelled(stop: &Receiver<()>, timeout: Duration) -> bool {
    match stop.recv_timeout(timeout) {
        Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => true,
        Err(mpsc::RecvTimeoutError::Timeout) => false,
    }
}
fn connect<B: Backend>(
    backend: &mut B,
    selection: &str,
    devices: &[DeviceInfo],
    diagnostics: SharedDiagnostics,
) -> Result<Requested<B::Device>, Error> {
    let mut failure: Option<Error> = None;
    let mut candidates: Vec<_> = devices
        .iter()
        .filter(|d| selection.is_empty() || d.path == selection)
        .collect();
    candidates.sort_by_key(|info| info.physical_address.is_none());
    for info in candidates {
        match Closed(info.clone())
            .open(backend)
            .and_then(|opened| opened.request(diagnostics.clone()))
        {
            Ok(device) => {
                if let Some(error) = failure {
                    tracing::debug!(
                        error = &error as &dyn std::error::Error,
                        "HDMI-CEC skipped unusable adapter"
                    );
                }
                tracing::info!(
                    path = info.path,
                    physical_address = ?info.physical_address,
                    automatic = selection.is_empty(),
                    "HDMI-CEC selected adapter"
                );
                return Ok(device);
            }
            Err(error) => {
                if let Some(previous) = failure.replace(error) {
                    tracing::debug!(
                        error = &previous as &dyn std::error::Error,
                        "HDMI-CEC skipped unusable adapter"
                    );
                }
            }
        }
    }
    // Candidate probes may recover through another adapter. The worker handles
    // the final failure and distinguishes changed failures from repeated retries.
    Err(failure.unwrap_or(Error::NoDevice))
}

fn run_device<D: Device>(
    device: Requested<D>,
    devices: &[DeviceInfo],
    stop: &Receiver<()>,
    snapshots: &mpsc::Sender<Snapshot>,
    inputs: &SyncSender<QueuedInput>,
    generation: &mut u64,
    now: impl Fn() -> Instant,
) -> Result<(), Error> {
    let diagnostics = device.0.diagnostics.clone();
    let mut connection = Connection::Pending(device);
    let mut address = connection.device().address()?;
    connection = connection.update(address);
    let mut claim_started = address.physical.map(|_| now());
    let mut keys = keys::HeldKey::default();
    let mut previous = None;
    loop {
        let active = matches!(connection, Connection::Active(_));
        let marker = (active, address);
        if previous != Some(marker) {
            if active {
                diagnostics.borrow_mut().active();
            } else {
                diagnostics.borrow_mut().waiting(address);
                keys.clear();
            }
            *generation += 1;
            let snapshot = Snapshot {
                generation: *generation,
                state: if active {
                    State::Active
                } else {
                    State::WaitingForTv
                },
                devices: devices.into(),
                physical_address: address.physical,
            };
            if snapshots.send(snapshot).is_err() {
                return Ok(());
            }
            previous = Some(marker);
        }
        match stop.try_recv() {
            Ok(()) | Err(mpsc::TryRecvError::Disconnected) => return Ok(()),
            Err(mpsc::TryRecvError::Empty) => {}
        }
        if !active
            && claim_started
                .is_some_and(|start| now().saturating_duration_since(start) >= CLAIM_TIMEOUT)
        {
            return Err(Error::Claim);
        }
        match connection.device().next_event(POLL_INTERVAL)? {
            Some(Event::StateChange(next)) => {
                if next.physical != address.physical {
                    keys.clear();
                }
                if next.physical.is_none() {
                    claim_started = None;
                } else if address.physical.is_none() {
                    claim_started = Some(now());
                }
                address = next;
                connection = connection.update(address);
                if matches!(connection, Connection::Active(_)) {
                    claim_started = None;
                } else if address.physical.is_some() && claim_started.is_none() {
                    claim_started = Some(now());
                }
            }
            Some(Event::Press { initiator, code }) if active => {
                if let Some(input) = keys.press(initiator, code, now()) {
                    match inputs.try_send(QueuedInput {
                        generation: *generation,
                        input,
                    }) {
                        Ok(()) => diagnostics.borrow_mut().input_accepted(),
                        Err(TrySendError::Full(_)) => diagnostics.borrow_mut().input_dropped(),
                        Err(TrySendError::Disconnected(_)) => return Ok(()),
                    }
                }
            }
            Some(Event::Release { initiator }) => keys.release(initiator),
            Some(Event::LostMessages(count)) => {
                keys.clear();
                tracing::warn!(count, "HDMI-CEC receive messages lost");
            }
            Some(Event::Press { .. }) | None => {}
        }
    }
}

#[cfg(all(test, not(target_os = "linux")))]
#[test]
fn unsupported_platform_never_starts_a_worker() {
    let mut controller = Controller::new(LaunchPlan::Preferences, true, String::new());
    assert!(!controller.allowed());
    assert!(!controller.enabled());
    assert_eq!(controller.snapshot().state.as_str(), "unsupported");
    assert!(!controller.enable(true));
    controller.refresh();
    assert!(matches!(controller.runtime, Runtime::Off));
}
