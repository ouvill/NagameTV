//! All device and lifecycle tests use an injected backend; none open /dev.
use super::*;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
const PRIMARY_PLAYBACK_MASK: u16 = 1 << 4;
const CONNECTED: Address = Address {
    physical: Some(0x2100),
    logical_mask: PRIMARY_PLAYBACK_MASK,
};
const UNPLUGGED: Address = Address {
    physical: None,
    logical_mask: 0,
};
#[derive(Clone)]
struct Fake {
    shared: Arc<Mutex<Shared>>,
    events: Arc<Mutex<Receiver<Result<Event, Error>>>>,
}
struct Shared {
    devices: Vec<DeviceInfo>,
    operations: Vec<String>,
    address: Address,
    failures: BTreeMap<String, io::ErrorKind>,
    claim_failure: bool,
    release_failure: Option<io::ErrorKind>,
    processed: usize,
    panic_on_poll: bool,
}
struct FakeDevice {
    fake: Fake,
    path: String,
}
impl Fake {
    fn new(address: Address) -> (Self, mpsc::Sender<Result<Event, Error>>) {
        let (events, receiver) = mpsc::channel();
        (
            Self {
                shared: Arc::new(Mutex::new(Shared {
                    devices: vec![info(0)],
                    operations: Vec::new(),
                    address,
                    failures: BTreeMap::new(),
                    claim_failure: false,
                    release_failure: None,
                    processed: 0,
                    panic_on_poll: false,
                })),
                events: Arc::new(Mutex::new(receiver)),
            },
            events,
        )
    }
    fn controller(&self, enabled: bool) -> Controller {
        Controller::with_backend(
            LaunchPlan::Preferences,
            enabled,
            String::new(),
            self.clone(),
        )
    }
    fn operations(&self) -> Vec<String> {
        self.shared.lock().unwrap().operations.clone()
    }
}
fn info(number: u8) -> DeviceInfo {
    DeviceInfo {
        path: format!("/dev/cec{number}"),
        name: format!("Fake adapter {number}"),
        driver: "test".into(),
        physical_address: CONNECTED.physical,
    }
}
impl Backend for Fake {
    type Device = FakeDevice;
    fn enumerate(&mut self) -> Result<Vec<DeviceInfo>, Error> {
        let mut shared = self.shared.lock().unwrap();
        shared.operations.push("enumerate".into());
        Ok(shared.devices.clone())
    }
    fn open(&mut self, info: &DeviceInfo) -> Result<Self::Device, Error> {
        let mut shared = self.shared.lock().unwrap();
        shared.operations.push(format!("open {}", info.path));
        if let Some(kind) = shared.failures.get(&info.path) {
            return Err(Error::io("open", &info.path, io::Error::from(*kind)));
        }
        Ok(FakeDevice {
            fake: self.clone(),
            path: info.path.clone(),
        })
    }
}
impl Device for FakeDevice {
    fn claim_playback(&mut self) -> Result<(), Error> {
        let mut shared = self.fake.shared.lock().unwrap();
        shared
            .operations
            .push(format!("claim {} {OSD_NAME}", self.path));
        if shared.claim_failure {
            return Err(Error::Claim);
        }
        if shared.address.physical.is_some() {
            shared.address.logical_mask = PRIMARY_PLAYBACK_MASK;
        }
        Ok(())
    }
    fn address(&mut self) -> Result<Address, Error> {
        Ok(self.fake.shared.lock().unwrap().address)
    }
    fn next_event(&mut self, timeout: Duration) -> Result<Option<Event>, Error> {
        let panic_on_poll = self.fake.shared.lock().unwrap().panic_on_poll;
        assert!(!panic_on_poll, "injected worker panic");
        match self.fake.events.lock().unwrap().recv_timeout(timeout) {
            Ok(Ok(event)) => {
                self.fake.shared.lock().unwrap().processed += 1;
                if let Event::StateChange(address) = &event {
                    self.fake.shared.lock().unwrap().address = *address;
                }
                Ok(Some(event))
            }
            Ok(Err(error)) => Err(error),
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(Error::Disconnected {
                path: self.path.clone(),
                source: io::ErrorKind::NotConnected.into(),
            }),
        }
    }
    fn release(&mut self) -> Result<(), Error> {
        let mut shared = self.fake.shared.lock().unwrap();
        shared.operations.push(format!("release {}", self.path));
        if let Some(kind) = shared.release_failure {
            return Err(Error::io(
                "release logical address",
                &self.path,
                io::Error::from(kind),
            ));
        }
        shared.address.logical_mask = 0;
        Ok(())
    }
}
impl Drop for FakeDevice {
    fn drop(&mut self) {
        self.fake
            .shared
            .lock()
            .unwrap()
            .operations
            .push(format!("close {}", self.path));
    }
}
fn wait_state(controller: &mut Controller, expected: &str) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        controller.poll();
        if controller.snapshot().state.as_str() == expected {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "expected {expected}, got {:?}",
            controller.snapshot()
        );
        thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn off_and_restricted_launch_never_enumerate_open_or_start_a_worker() {
    let (fake, _events) = Fake::new(CONNECTED);
    let mut controller = fake.controller(false);
    assert!(controller.allowed());
    assert!(matches!(controller.runtime, Runtime::Off));
    controller.refresh();
    assert!(controller.select("/dev/cec1".into()));
    assert!(controller.enable(false));
    drop(controller);
    assert!(fake.operations().is_empty());
    let mut restricted = Controller::with_backend(
        LaunchPlan::Restricted("none".parse().unwrap()),
        true,
        String::new(),
        fake.clone(),
    );
    assert!(!restricted.allowed());
    assert!(!restricted.enabled());
    assert!(!restricted.enable(true));
    restricted.refresh();
    assert!(fake.operations().is_empty());
}
#[test]
fn runtime_off_joins_and_releases_before_returning_and_can_enable_again() {
    let (fake, _events) = Fake::new(CONNECTED);
    let mut controller = fake.controller(true);
    wait_state(&mut controller, "active");
    assert!(controller.enable(false));
    assert!(!controller.enabled());
    assert!(matches!(controller.runtime, Runtime::Off));
    assert_eq!(controller.snapshot().state.as_str(), "disabled");
    assert_eq!(
        fake.operations(),
        [
            "enumerate",
            "open /dev/cec0",
            "claim /dev/cec0 NagameTV",
            "release /dev/cec0",
            "close /dev/cec0"
        ]
    );
    assert!(controller.enable(true));
    wait_state(&mut controller, "active");
    drop(controller);
    assert_eq!(
        fake.operations()
            .iter()
            .filter(|s| s.starts_with("release"))
            .count(),
        2
    );
}
#[test]
fn launch_feature_enables_transient_cec_and_rejects_duplicates() {
    let (fake, _events) = Fake::new(CONNECTED);
    let mut controller = Controller::with_backend(
        LaunchPlan::Restricted("hdmi-cec".parse().unwrap()),
        false,
        String::new(),
        fake,
    );
    assert!(controller.enabled());
    assert!(controller.allowed());
    wait_state(&mut controller, "active");
    assert!(
        "hdmi-cec,hdmi-cec"
            .parse::<crate::features::FeatureSet>()
            .is_err()
    );
    assert!(
        "hdmi-cec,remote-web"
            .parse::<crate::features::FeatureSet>()
            .is_ok()
    );
    assert!(!LaunchPlan::Preferences.hdmi_cec(false));
    assert!(LaunchPlan::Preferences.hdmi_cec(true));
}
#[test]
fn automatic_selection_prefers_a_connected_adapter_before_claiming() {
    let (fake, _events) = Fake::new(CONNECTED);
    {
        let mut shared = fake.shared.lock().unwrap();
        shared.devices[0].physical_address = None;
        shared.devices.push(info(1));
    }
    let mut controller = fake.controller(true);
    wait_state(&mut controller, "active");
    assert_eq!(
        fake.operations(),
        ["enumerate", "open /dev/cec1", "claim /dev/cec1 NagameTV"]
    );
    assert!(controller.enable(false));
    assert!(
        fake.operations()
            .ends_with(&["release /dev/cec1".into(), "close /dev/cec1".into()])
    );
}

#[test]
fn automatic_selection_skips_failed_devices_but_explicit_selection_does_not() {
    let (fake, _events) = Fake::new(CONNECTED);
    {
        let mut shared = fake.shared.lock().unwrap();
        shared.devices.push(info(1));
        shared
            .failures
            .insert(info(0).path, io::ErrorKind::PermissionDenied);
    }
    let mut controller = fake.controller(true);
    wait_state(&mut controller, "active");
    assert_eq!(controller.snapshot().devices.len(), 2);
    assert!(controller.select(info(0).path));
    wait_state(&mut controller, "permission-denied");
    assert!(
        controller
            .snapshot()
            .state
            .message()
            .contains("video group")
    );
    assert_eq!(
        fake.operations()
            .iter()
            .filter(|s| s.as_str() == "open /dev/cec1")
            .count(),
        1
    );
    assert!(controller.select(info(1).path));
    wait_state(&mut controller, "active");
    assert!(controller.select("/dev/cec9".into()));
    wait_state(&mut controller, "no-device");
    for invalid in ["/dev/cec", "/dev/cec0/../sda", "/tmp/cec0", "/dev/cec-1"] {
        assert!(!controller.select(invalid.into()));
    }
    assert_eq!(controller.selection(), "/dev/cec9");
}
#[test]
fn tv_disconnect_and_state_change_reclaim_without_reopening() {
    let (fake, events) = Fake::new(UNPLUGGED);
    let mut controller = fake.controller(true);
    wait_state(&mut controller, "waiting-for-tv");
    events.send(Ok(Event::StateChange(CONNECTED))).unwrap();
    wait_state(&mut controller, "active");
    assert_eq!(controller.snapshot().physical_address, CONNECTED.physical);
    events.send(Ok(Event::StateChange(UNPLUGGED))).unwrap();
    wait_state(&mut controller, "waiting-for-tv");
    assert_eq!(controller.snapshot().physical_address, None);
    events.send(Ok(Event::StateChange(CONNECTED))).unwrap();
    wait_state(&mut controller, "active");
    drop(controller);
    assert_eq!(
        fake.operations()
            .iter()
            .filter(|s| s.starts_with("open"))
            .count(),
        1
    );
}
#[test]
fn no_device_claim_failure_and_disconnect_have_distinct_states_and_release() {
    let (fake, _events) = Fake::new(CONNECTED);
    fake.shared.lock().unwrap().devices.clear();
    let mut controller = fake.controller(true);
    wait_state(&mut controller, "no-device");
    assert!(controller.enable(false));
    fake.shared.lock().unwrap().devices.push(info(0));
    fake.shared.lock().unwrap().claim_failure = true;
    assert!(controller.enable(true));
    wait_state(&mut controller, "error");
    assert!(
        controller
            .snapshot()
            .state
            .message()
            .contains("claim failed")
    );
    assert!(controller.enable(false));
    fake.shared.lock().unwrap().claim_failure = false;
    assert!(controller.enable(true));
    wait_state(&mut controller, "active");
    drop(_events);
    wait_state(&mut controller, "disconnected");
    assert!(
        fake.operations()
            .ends_with(&["release /dev/cec0".into(), "close /dev/cec0".into()])
    );
}
#[test]
fn claim_timeout_uses_injected_time_and_releases_the_requested_address() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let (fake, _events) = Fake::new(CONNECTED);
    let requested = connect(&mut fake.clone(), "", &[info(0)], Diagnostics::shared()).unwrap();
    fake.shared.lock().unwrap().address.logical_mask = 0;
    let (_stop, stop) = mpsc::channel();
    let (snapshots, _receiver) = mpsc::channel();
    let (inputs, _input_receiver) = mpsc::sync_channel(4);
    let now = Instant::now();
    let calls = AtomicUsize::new(0);
    let result = run_device(
        requested,
        &[info(0)],
        &stop,
        &snapshots,
        &inputs,
        &mut 0,
        || {
            now + if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                Duration::ZERO
            } else {
                CLAIM_TIMEOUT
            }
        },
    );
    assert!(matches!(result, Err(Error::Claim)));
    assert!(
        fake.operations()
            .ends_with(&["release /dev/cec0".into(), "close /dev/cec0".into()])
    );
}
#[test]
fn lost_release_and_queue_pressure_do_not_block_off_or_deliver_after_off() {
    let (fake, events) = Fake::new(CONNECTED);
    let mut controller = fake.controller(true);
    wait_state(&mut controller, "active");
    for _ in 0..INPUT_QUEUE_SIZE * 2 {
        events
            .send(Ok(Event::Press {
                initiator: 0,
                code: 0x49,
            }))
            .unwrap();
        events.send(Ok(Event::Release { initiator: 0 })).unwrap();
    }
    events.send(Ok(Event::LostMessages(2))).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while fake.shared.lock().unwrap().processed < INPUT_QUEUE_SIZE * 4 + 1 {
        assert!(
            Instant::now() < deadline,
            "fake did not fill the input queue"
        );
        thread::sleep(Duration::from_millis(1));
    }
    assert!(controller.enable(false));
    assert_eq!(controller.next_input(), None);
    assert!(fake.operations().last().unwrap().starts_with("close"));
}

#[test]
fn queued_input_from_a_disconnected_generation_is_discarded() {
    let (fake, events) = Fake::new(CONNECTED);
    let mut controller = fake.controller(true);
    wait_state(&mut controller, "active");
    events
        .send(Ok(Event::Press {
            initiator: 0,
            code: 0x45,
        }))
        .unwrap();
    events
        .send(Err(Error::Disconnected {
            path: info(0).path,
            source: io::ErrorKind::NotConnected.into(),
        }))
        .unwrap();
    wait_state(&mut controller, "disconnected");
    assert_eq!(controller.next_input(), None);
}

#[test]
fn unexpected_worker_exit_reports_failure_and_releases_the_device() {
    let (fake, _events) = Fake::new(CONNECTED);
    fake.shared.lock().unwrap().panic_on_poll = true;
    let mut controller = fake.controller(true);
    wait_state(&mut controller, "error");
    assert!(
        controller
            .snapshot()
            .state
            .message()
            .contains("exited unexpectedly")
    );
    assert!(matches!(controller.runtime, Runtime::Off));
    assert!(
        fake.operations()
            .ends_with(&["release /dev/cec0".into(), "close /dev/cec0".into()])
    );
}

#[derive(Debug)]
struct DiagnosticEvent {
    level: tracing::Level,
    fields: BTreeMap<String, String>,
}
impl tracing::field::Visit for DiagnosticEvent {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.fields
            .insert(field.name().into(), format!("{value:?}"));
    }
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.fields.insert(field.name().into(), value.into());
    }
    fn record_error(
        &mut self,
        field: &tracing::field::Field,
        value: &(dyn std::error::Error + 'static),
    ) {
        self.fields.insert(field.name().into(), value.to_string());
    }
}
#[derive(Clone, Default)]
struct DiagnosticEvents(Arc<Mutex<Vec<DiagnosticEvent>>>);
impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for DiagnosticEvents {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        if event.metadata().target().ends_with("hdmi_cec::diagnostics") {
            let mut recorded = DiagnosticEvent {
                level: *event.metadata().level(),
                fields: BTreeMap::new(),
            };
            event.record(&mut recorded);
            self.0.lock().unwrap().push(recorded);
        }
    }
}
impl DiagnosticEvents {
    fn capture(&self, run: impl FnOnce()) {
        use tracing_subscriber::prelude::*;
        tracing::subscriber::with_default(tracing_subscriber::registry().with(self.clone()), run);
    }
    fn levels(&self) -> Vec<tracing::Level> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .map(|event| event.level)
            .collect()
    }
}
fn fake_failure(fake: &Fake, diagnostics: &SharedDiagnostics) -> State {
    let devices = fake.clone().enumerate().unwrap();
    let error = match connect(&mut fake.clone(), "", &devices, diagnostics.clone()) {
        Err(error) => error,
        Ok(_) => panic!("fake was expected to fail connecting"),
    };
    diagnostics.borrow_mut().failed("connect adapter", error)
}
fn stopped_fake_session(fake: &Fake, diagnostics: &SharedDiagnostics) -> State {
    let requested = connect(&mut fake.clone(), "", &[info(0)], diagnostics.clone()).unwrap();
    let (cancel, stop) = mpsc::channel();
    cancel.send(()).unwrap();
    let (snapshots, receiver) = mpsc::channel();
    let (inputs, _input_receiver) = mpsc::sync_channel(INPUT_QUEUE_SIZE);
    run_device(
        requested,
        &[info(0)],
        &stop,
        &snapshots,
        &inputs,
        &mut 0,
        Instant::now,
    )
    .unwrap();
    receiver.recv().unwrap().state
}

#[test]
fn retrying_fake_failures_only_warn_when_the_cause_changes_or_playback_recovers() {
    use tracing::Level;
    let (fake, _events) = Fake::new(CONNECTED);
    let diagnostics = Diagnostics::shared();
    let recorded = DiagnosticEvents::default();
    recorded.capture(|| {
        fake.shared.lock().unwrap().devices.clear();
        for _ in 0..2 {
            let state = fake_failure(&fake, &diagnostics);
            assert!(matches!(state, State::NoDevice));
            for _ in 0..3 {
                assert!(!state.message().is_empty());
            }
        }
        {
            let mut shared = fake.shared.lock().unwrap();
            shared.devices.push(info(0));
            shared
                .failures
                .insert(info(0).path, io::ErrorKind::PermissionDenied);
        }
        for _ in 0..2 {
            assert!(matches!(
                fake_failure(&fake, &diagnostics),
                State::PermissionDenied(_)
            ));
        }
        fake.shared
            .lock()
            .unwrap()
            .failures
            .insert(info(0).path, io::ErrorKind::BrokenPipe);
        for _ in 0..2 {
            assert!(matches!(
                fake_failure(&fake, &diagnostics),
                State::Failed(_)
            ));
        }
        fake.shared.lock().unwrap().failures.clear();
        assert!(matches!(
            stopped_fake_session(&fake, &diagnostics),
            State::Active
        ));
        fake.shared
            .lock()
            .unwrap()
            .failures
            .insert(info(0).path, io::ErrorKind::BrokenPipe);
        assert!(matches!(
            fake_failure(&fake, &diagnostics),
            State::Failed(_)
        ));
    });
    assert_eq!(
        recorded.levels(),
        [
            Level::WARN,
            Level::DEBUG,
            Level::WARN,
            Level::DEBUG,
            Level::WARN,
            Level::DEBUG,
            Level::WARN
        ]
    );
    let events = recorded.0.lock().unwrap();
    assert!(
        events
            .iter()
            .all(|event| event.fields["operation"] == "connect adapter")
    );
    assert!(events[0].fields["error"].contains("No HDMI-CEC device found"));
    assert!(events[2].fields["error"].contains("/dev/cec0"));
    assert!(events[2].fields["error"].contains("Permission denied"));
}

#[test]
fn failure_identity_includes_error_operation_kind_device_and_the_underlying_causes() {
    use super::diagnostics::{Change, FailureLog};
    let mut log = FailureLog::default();
    for error in [
        Error::NoDevice,
        Error::Permission {
            path: info(0).path,
            source: io::ErrorKind::PermissionDenied.into(),
        },
        Error::Disconnected {
            path: info(0).path,
            source: io::ErrorKind::NotConnected.into(),
        },
        Error::Disconnected {
            path: info(1).path,
            source: io::ErrorKind::NotConnected.into(),
        },
        Error::Busy(info(0).path),
        Error::Capabilities(info(0).path),
        Error::Claim,
        Error::Worker(io::ErrorKind::BrokenPipe.into()),
        Error::Unsupported,
    ] {
        assert_eq!(log.observe(&error), Change::New);
        assert_eq!(log.observe(&error), Change::Repeated);
    }
    let operation = |operation| Error::Io {
        operation,
        path: info(0).path,
        source: io::ErrorKind::BrokenPipe.into(),
    };
    assert_eq!(log.observe(&operation("open")), Change::New);
    assert_eq!(log.observe(&operation("receive")), Change::New);
    let same_message = |kind| Error::Worker(io::Error::new(kind, "adapter not ready"));
    assert_eq!(
        log.observe(&same_message(io::ErrorKind::TimedOut)),
        Change::New
    );
    assert_eq!(
        log.observe(&same_message(io::ErrorKind::WouldBlock)),
        Change::New
    );

    #[derive(Debug, thiserror::Error)]
    #[error("adapter not ready")]
    struct Source(#[source] io::Error);
    let cause =
        |detail: &'static str| Error::Worker(io::Error::other(Source(io::Error::other(detail))));
    let first = cause("bus disconnected");
    let second = cause("claim rejected");
    assert_eq!(first.to_string(), second.to_string());
    assert_eq!(log.observe(&first), Change::New);
    assert_eq!(log.observe(&first), Change::Repeated);
    assert_eq!(log.observe(&second), Change::New);
}

#[test]
fn waiting_retries_stay_at_debug_until_an_active_fake_session_recovers() {
    use tracing::Level;
    let (fake, _events) = Fake::new(UNPLUGGED);
    let diagnostics = Diagnostics::shared();
    let recorded = DiagnosticEvents::default();
    recorded.capture(|| {
        for _ in 0..2 {
            assert!(matches!(
                stopped_fake_session(&fake, &diagnostics),
                State::WaitingForTv
            ));
        }
        fake.shared.lock().unwrap().address = CONNECTED;
        assert!(matches!(
            stopped_fake_session(&fake, &diagnostics),
            State::Active
        ));
        fake.shared.lock().unwrap().address = UNPLUGGED;
        assert!(matches!(
            stopped_fake_session(&fake, &diagnostics),
            State::WaitingForTv
        ));
    });
    assert_eq!(recorded.levels(), [Level::WARN, Level::DEBUG, Level::WARN]);
}

#[test]
fn release_retries_and_a_full_input_queue_report_once_until_their_operation_recovers() {
    use tracing::Level;
    let (fake, _events) = Fake::new(CONNECTED);
    let diagnostics = Diagnostics::shared();
    let recorded = DiagnosticEvents::default();
    recorded.capture(|| {
        fake.shared.lock().unwrap().release_failure = Some(io::ErrorKind::BrokenPipe);
        for _ in 0..2 {
            drop(connect(&mut fake.clone(), "", &[info(0)], diagnostics.clone()).unwrap());
        }
        fake.shared.lock().unwrap().release_failure = None;
        drop(connect(&mut fake.clone(), "", &[info(0)], diagnostics.clone()).unwrap());
        fake.shared.lock().unwrap().release_failure = Some(io::ErrorKind::BrokenPipe);
        drop(connect(&mut fake.clone(), "", &[info(0)], diagnostics.clone()).unwrap());
        diagnostics.borrow_mut().input_dropped();
        diagnostics.borrow_mut().input_dropped();
        diagnostics.borrow_mut().input_accepted();
        diagnostics.borrow_mut().input_dropped();
    });
    assert_eq!(
        recorded.levels(),
        [
            Level::ERROR,
            Level::DEBUG,
            Level::ERROR,
            Level::WARN,
            Level::DEBUG,
            Level::WARN
        ]
    );
}

#[test]
fn retry_stage_changes_do_not_warn_again_for_the_same_disconnected_adapter() {
    use tracing::Level;
    let mut diagnostics = Diagnostics::default();
    let recorded = DiagnosticEvents::default();
    recorded.capture(|| {
        for operation in ["receive from adapter", "connect adapter", "connect adapter"] {
            diagnostics.failed(
                operation,
                Error::Disconnected {
                    path: info(0).path,
                    source: io::ErrorKind::NotConnected.into(),
                },
            );
        }
    });
    assert_eq!(recorded.levels(), [Level::WARN, Level::DEBUG, Level::DEBUG]);
}
