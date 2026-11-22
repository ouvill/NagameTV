//! One-shot HDMI-CEC configuration and ownership, independent of Qt.
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
mod ownership;
#[cfg(all(test, target_os = "linux"))]
mod tests;

use crate::features::LaunchPlan;
#[cfg(target_os = "linux")]
use ownership::Session;
#[cfg(target_os = "linux")]
use std::io;

pub const OSD_NAME: &str = "NagameTV";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceInfo {
    pub path: String,
    pub name: String,
    pub driver: String,
    pub(crate) physical_address: Option<u16>,
}

#[cfg(target_os = "linux")]
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("No HDMI-CEC device found")]
    NoDevice,
    #[error("CEC {operation} denied ({path}): {source}")]
    Permission {
        operation: &'static str,
        path: String,
        source: io::Error,
    },
    #[error("CEC {operation} failed ({path}): {source}")]
    Io {
        operation: &'static str,
        path: String,
        source: io::Error,
    },
    #[error("CEC adapter {0} cannot be configured with a logical address")]
    NoLogicalAddresses(String),
    #[error("CEC adapter {0} has no remote-control capability")]
    NoRemoteControl(String),
    #[error("Another NagameTV process is configuring CEC adapter {0}")]
    Configuring(String),
}

#[derive(Debug)]
pub(crate) enum State {
    Disabled,
    #[cfg(not(target_os = "linux"))]
    Unsupported,
    #[cfg(target_os = "linux")]
    NoDevice,
    #[cfg(target_os = "linux")]
    PermissionDenied(Error),
    #[cfg(target_os = "linux")]
    NoRemoteControl,
    #[cfg(target_os = "linux")]
    Configured,
    #[cfg(target_os = "linux")]
    External {
        rc_passthrough: bool,
    },
    #[cfg(target_os = "linux")]
    Failed(Error),
}

impl State {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            #[cfg(not(target_os = "linux"))]
            Self::Unsupported => "unsupported",
            #[cfg(target_os = "linux")]
            Self::NoDevice => "no-device",
            #[cfg(target_os = "linux")]
            Self::PermissionDenied(_) => "permission-denied",
            #[cfg(target_os = "linux")]
            Self::NoRemoteControl => "no-remote-control",
            #[cfg(target_os = "linux")]
            Self::Configured => "configured",
            #[cfg(target_os = "linux")]
            Self::External { .. } => "external",
            #[cfg(target_os = "linux")]
            Self::Failed(_) => "error",
        }
    }

    pub fn message(&self) -> String {
        match self {
            #[cfg(target_os = "linux")]
            Self::PermissionDenied(error) | Self::Failed(error) => error.to_string(),
            _ => String::new(),
        }
    }

    pub fn external_without_passthrough(&self) -> bool {
        match self {
            #[cfg(target_os = "linux")]
            Self::External { rc_passthrough } => !rc_passthrough,
            _ => false,
        }
    }

    #[cfg(target_os = "linux")]
    fn failed(error: Error) -> Self {
        match error {
            Error::NoDevice => Self::NoDevice,
            error @ Error::Permission { .. } => Self::PermissionDenied(error),
            Error::NoRemoteControl(_) => Self::NoRemoteControl,
            error @ (Error::Io { .. } | Error::NoLogicalAddresses(_) | Error::Configuring(_)) => {
                Self::Failed(error)
            }
        }
    }
}

pub struct Snapshot {
    pub state: State,
    pub devices: Vec<DeviceInfo>,
    pub physical_address: Option<u16>,
}

impl Snapshot {
    fn new(state: State) -> Self {
        Self {
            state,
            devices: Vec::new(),
            physical_address: None,
        }
    }
}

#[derive(PartialEq, Eq)]
enum Mode {
    #[cfg(not(target_os = "linux"))]
    Unsupported,
    Disabled,
    Enabled,
}

impl Mode {
    fn inactive_state(&self) -> State {
        match self {
            #[cfg(not(target_os = "linux"))]
            Self::Unsupported => State::Unsupported,
            Self::Disabled | Self::Enabled => State::Disabled,
        }
    }
}

pub(crate) enum SelectionError {
    InvalidPath,
    ReleaseFailed,
}

pub struct Controller {
    #[cfg(target_os = "linux")]
    session: Session,
    allowed: bool,
    mode: Mode,
    selection: String,
    snapshot: Snapshot,
}

impl Controller {
    pub fn new(_plan: LaunchPlan, _preference: bool, selection: String) -> Self {
        #[cfg(target_os = "linux")]
        {
            Self::with_session(
                _plan,
                _preference,
                selection,
                Session::new(
                    linux::Linux,
                    std::env::var_os("XDG_RUNTIME_DIR").map(Into::into),
                ),
            )
        }
        #[cfg(not(target_os = "linux"))]
        {
            Self {
                allowed: false,
                mode: Mode::Unsupported,
                selection,
                snapshot: Snapshot::new(State::Unsupported),
            }
        }
    }

    #[cfg(target_os = "linux")]
    fn with_session(
        plan: LaunchPlan,
        preference: bool,
        selection: String,
        session: Session,
    ) -> Self {
        let mut controller = Self {
            session,
            allowed: !plan.locked() || plan.hdmi_cec(false),
            mode: Mode::Disabled,
            selection,
            snapshot: Snapshot::new(State::Disabled),
        };
        controller.enable(plan.hdmi_cec(preference));
        controller
    }

    pub fn allowed(&self) -> bool {
        self.allowed
    }

    pub fn enabled(&self) -> bool {
        self.mode == Mode::Enabled
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
        if !enabled {
            self.stop();
        } else if !self.enabled() {
            self.mode = Mode::Enabled;
            self.refresh();
        }
        true
    }

    pub fn select(&mut self, selection: String) -> Result<(), SelectionError> {
        if !selection.is_empty() && !valid_path(&selection) {
            tracing::warn!(
                path = selection,
                "HDMI-CEC selection rejected: expected /dev/cecN or automatic"
            );
            return Err(SelectionError::InvalidPath);
        }
        if self.selection != selection {
            if !self.release() {
                return Err(SelectionError::ReleaseFailed);
            }
            self.selection = selection;
            self.refresh();
        }
        Ok(())
    }

    pub fn refresh(&mut self) {
        #[cfg(target_os = "linux")]
        if self.enabled() {
            self.snapshot = self.session.refresh(&self.selection);
        }
    }

    fn release(&mut self) -> bool {
        #[cfg(target_os = "linux")]
        if let Err(error) = self.session.release() {
            tracing::warn!(%error, "HDMI-CEC release failed");
            self.snapshot.state = State::Failed(error);
            self.snapshot.physical_address = None;
            return false;
        }
        true
    }

    pub fn stop(&mut self) {
        if self.enabled() {
            self.mode = Mode::Disabled;
        }
        self.snapshot = Snapshot::new(self.mode.inactive_state());
        self.release();
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        self.release();
    }
}

pub fn valid_path(path: &str) -> bool {
    path.strip_prefix("/dev/cec")
        .is_some_and(|number| !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()))
}
