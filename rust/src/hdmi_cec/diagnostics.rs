//! Worker-local diagnostics distinguish retained failures from new incidents.
use super::{Address, Error, State};
use std::{cell::RefCell, error::Error as _, io, mem::Discriminant, rc::Rc};

pub(super) type SharedDiagnostics = Rc<RefCell<Diagnostics>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Change {
    New,
    Repeated,
}

#[derive(PartialEq, Eq)]
struct Failure {
    kind: Discriminant<Error>,
    message: String,
    causes: Vec<String>,
    io: Option<(io::ErrorKind, Option<i32>)>,
}
impl Failure {
    fn new(error: &Error) -> Self {
        let mut causes = Vec::new();
        let mut source = error.source();
        while let Some(cause) = source {
            causes.push(cause.to_string());
            source = cause.source();
        }
        let io = match error {
            Error::Permission { source, .. }
            | Error::Io { source, .. }
            | Error::Disconnected { source, .. }
            | Error::Worker(source) => Some((source.kind(), source.raw_os_error())),
            Error::NoDevice
            | Error::Capabilities(_)
            | Error::Busy(_)
            | Error::Claim
            | Error::Unsupported => None,
        };
        Self {
            kind: std::mem::discriminant(error),
            message: error.to_string(),
            causes,
            io,
        }
    }
}
#[derive(Default)]
pub(super) struct FailureLog {
    previous: Option<Failure>,
}
impl FailureLog {
    pub(super) fn observe(&mut self, error: &Error) -> Change {
        let failure = Failure::new(error);
        if self.previous.as_ref() == Some(&failure) {
            return Change::Repeated;
        }
        self.previous = Some(failure);
        Change::New
    }
    fn recovered(&mut self) {
        self.previous = None;
    }
    fn report(&mut self, operation: &'static str, error: &Error, severity: Severity) {
        match (self.observe(error), severity) {
            (Change::Repeated, _) => tracing::debug!(
                operation,
                error = error as &dyn std::error::Error,
                "HDMI-CEC operation still failing"
            ),
            (Change::New, Severity::Warning) => tracing::warn!(
                operation,
                error = error as &dyn std::error::Error,
                "HDMI-CEC operation failed"
            ),
            (Change::New, Severity::Error) => tracing::error!(
                operation,
                error = error as &dyn std::error::Error,
                "HDMI-CEC operation failed"
            ),
        }
    }
}
enum Severity {
    Warning,
    Error,
}
#[derive(Default)]
enum InputQueue {
    #[default]
    Available,
    Full,
}
#[derive(Default)]
pub(super) struct Diagnostics {
    failure: FailureLog,
    release: FailureLog,
    transmit: FailureLog,
    waiting: Option<Address>,
    input_queue: InputQueue,
}
impl Diagnostics {
    pub(super) fn shared() -> SharedDiagnostics {
        Rc::new(RefCell::new(Self::default()))
    }
    pub(super) fn failed(&mut self, operation: &'static str, error: Error) -> State {
        self.failure.report(operation, &error, Severity::Warning);
        State::failure(error)
    }
    pub(super) fn active(&mut self) {
        // Starting another pending claim does not prove that the failure recovered.
        self.failure.recovered();
        self.waiting = None;
    }
    pub(super) fn released(&mut self, result: Result<(), Error>) {
        match result {
            Ok(()) => self.release.recovered(),
            Err(error) => self
                .release
                .report("release logical address", &error, Severity::Error),
        }
    }
    pub(super) fn transmitted(&mut self, result: Result<(), Error>) {
        match result {
            Ok(()) => self.transmit.recovered(),
            Err(error) => self
                .transmit
                .report("queue CEC message", &error, Severity::Warning),
        }
    }
    pub(super) fn waiting(&mut self, address: Address) {
        if self.waiting == Some(address) {
            tracing::debug!(operation = "claim Playback address", physical_address = ?address.physical, logical_mask = address.logical_mask, "HDMI-CEC still waiting for TV/logical address");
        } else {
            tracing::warn!(operation = "claim Playback address", physical_address = ?address.physical, logical_mask = address.logical_mask, "HDMI-CEC waiting for TV/logical address");
            self.waiting = Some(address);
        }
    }
    pub(super) fn input_accepted(&mut self) {
        self.input_queue = InputQueue::Available;
    }
    pub(super) fn input_dropped(&mut self) {
        match self.input_queue {
            InputQueue::Available => tracing::warn!(
                operation = "enqueue remote input",
                "HDMI-CEC input queue full; discarding key"
            ),
            InputQueue::Full => tracing::debug!(
                operation = "enqueue remote input",
                "HDMI-CEC input queue still full; discarding key"
            ),
        }
        self.input_queue = InputQueue::Full;
    }
}
