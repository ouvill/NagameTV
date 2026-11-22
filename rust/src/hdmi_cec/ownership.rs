use super::{DeviceInfo, Error, Snapshot, State};
use std::{
    fs::{self, File, OpenOptions, TryLockError},
    io::{self, Write},
    path::{Path, PathBuf},
};

pub(super) trait Backend {
    fn enumerate(&mut self) -> Result<Vec<DeviceInfo>, Error>;
    fn open(&mut self, path: &str) -> Result<Box<dyn Device>, Error>;
}

pub(super) trait Device {
    fn name(&self) -> &str;
    fn supports_logical_addresses(&self) -> bool;
    fn supports_remote_control(&self) -> bool;
    fn physical_address(&self) -> Result<Option<u16>, Error>;
    fn logical_config(&self) -> Result<LogicalConfig, Error>;
    fn configure_playback(&self) -> Result<(), Error>;
    fn clear_logical_addresses(&self) -> Result<(), Error>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LogicalConfig {
    Empty,
    NagameTv,
    External { rc_passthrough: bool },
}

#[derive(PartialEq, Eq)]
struct Adapter {
    path: String,
    name: String,
}

enum Registration {
    Owned(OwnedAdapter),
    Untracked(Adapter),
    External { path: String, rc_passthrough: bool },
}

impl Registration {
    fn path(&self) -> &str {
        match self {
            Self::Owned(owned) => &owned.adapter.path,
            Self::Untracked(adapter) => &adapter.path,
            Self::External { path, .. } => path,
        }
    }

    fn state(&self) -> State {
        match self {
            Self::Owned(_) | Self::Untracked(_) => State::Configured,
            Self::External { rc_passthrough, .. } => State::External {
                rc_passthrough: *rc_passthrough,
            },
        }
    }
}

pub(super) struct Session {
    backend: Box<dyn Backend>,
    marker_dir: Option<PathBuf>,
    registered: Option<Registration>,
}

impl Session {
    pub(super) fn new(backend: impl Backend + 'static, runtime_dir: Option<PathBuf>) -> Self {
        Self {
            backend: Box::new(backend),
            marker_dir: runtime_dir
                .filter(|path| !path.as_os_str().is_empty())
                .map(|path| path.join("nagametv/hdmi-cec")),
            registered: None,
        }
    }

    pub(super) fn refresh(&mut self, selection: &str) -> Snapshot {
        let mut snapshot = Snapshot::new(State::NoDevice);
        let result = self.scan(selection, &mut snapshot.devices);
        match result {
            Ok(physical) => {
                snapshot.state = self
                    .registered
                    .as_ref()
                    .expect("successful configure")
                    .state();
                snapshot.physical_address = physical;
            }
            Err(error) => {
                tracing::warn!(%error, "HDMI-CEC configuration failed");
                snapshot.state = State::failed(error);
            }
        }
        snapshot
    }

    fn scan(
        &mut self,
        selection: &str,
        devices: &mut Vec<DeviceInfo>,
    ) -> Result<Option<u16>, Error> {
        *devices = self.backend.enumerate()?;
        if let Some(registered) = &self.registered {
            let path = registered.path().to_owned();
            if devices.iter().any(|info| info.path == path) {
                // A changed physical address should not switch the selected adapter.
                return self.configure(&path);
            }
            self.release()?;
        }

        let mut candidates: Vec<_> = devices
            .iter()
            .filter(|info| selection.is_empty() || info.path == selection)
            .collect();
        candidates.sort_by_key(|info| info.physical_address.is_none());
        let mut failure = Error::NoDevice;
        for info in candidates {
            match self.configure(&info.path) {
                Ok(physical) => return Ok(physical),
                Err(error) => {
                    tracing::debug!(%error, "HDMI-CEC skipped adapter");
                    failure = error;
                }
            }
        }
        Err(failure)
    }

    fn configure(&mut self, path: &str) -> Result<Option<u16>, Error> {
        let device = self.backend.open(path)?;
        if !device.supports_remote_control() {
            return Err(Error::NoRemoteControl(path.into()));
        }
        if !device.supports_logical_addresses() {
            return Err(Error::NoLogicalAddresses(path.into()));
        }
        let adapter = Adapter {
            path: path.into(),
            name: device.name().into(),
        };
        let physical = device.physical_address()?;
        match &self.registered {
            Some(Registration::Owned(owned)) => {
                if owned.adapter == adapter
                    && owned.slot.matches(&adapter)?
                    && device.logical_config()? == LogicalConfig::NagameTv
                {
                    return Ok(physical);
                }
                owned.slot.remove()?;
                self.registered = None;
            }
            Some(Registration::Untracked(id)) if id == &adapter => {
                if device.logical_config()? == LogicalConfig::NagameTv {
                    return Ok(physical);
                }
                self.registered = None;
            }
            _ => {}
        }

        let slot = open_slot(self.marker_dir.as_deref(), path)?;
        let registration = match device.logical_config()? {
            LogicalConfig::NagameTv => match slot {
                SlotAccess::Acquired(slot) if slot.matches(&adapter)? => {
                    Registration::Owned(OwnedAdapter { adapter, slot })
                }
                other => {
                    other.remove_marker()?;
                    Registration::External {
                        path: path.into(),
                        rc_passthrough: true,
                    }
                }
            },
            LogicalConfig::External { rc_passthrough } => {
                slot.remove_marker()?;
                Registration::External {
                    path: path.into(),
                    rc_passthrough,
                }
            }
            LogicalConfig::Empty => match slot {
                SlotAccess::InUse => return Err(Error::Configuring(path.into())),
                SlotAccess::Acquired(slot) => {
                    // A stale marker must not survive a failed configure.
                    slot.remove()?;
                    device.configure_playback()?;
                    match slot.write(&adapter) {
                        Ok(()) => Registration::Owned(OwnedAdapter { adapter, slot }),
                        Err(error) => {
                            tracing::warn!(%error, "HDMI-CEC configured; automatic release unavailable");
                            Registration::Untracked(adapter)
                        }
                    }
                }
                SlotAccess::Unavailable => {
                    device.configure_playback()?;
                    tracing::warn!(
                        path,
                        "HDMI-CEC configured without XDG_RUNTIME_DIR; automatic release unavailable"
                    );
                    Registration::Untracked(adapter)
                }
            },
        };
        self.registered = Some(registration);
        Ok(physical)
    }

    pub(super) fn release(&mut self) -> Result<(), Error> {
        if let Some(Registration::Owned(owned)) = &self.registered {
            owned.release(self.backend.as_mut())?;
        }
        self.registered = None;
        Ok(())
    }
}

enum SlotAccess {
    Unavailable,
    InUse,
    Acquired(MarkerSlot),
}

impl SlotAccess {
    fn remove_marker(&self) -> Result<(), Error> {
        match self {
            Self::Acquired(slot) => slot.remove(),
            Self::Unavailable | Self::InUse => Ok(()),
        }
    }
}

fn open_slot(directory: Option<&Path>, device_path: &str) -> Result<SlotAccess, Error> {
    let Some(directory) = directory else {
        return Ok(SlotAccess::Unavailable);
    };
    fs::create_dir_all(directory)
        .map_err(|e| marker_error("create ownership directory", directory, e))?;
    let name = device_path
        .strip_prefix("/dev/")
        .expect("validated CEC path");
    let lock_path = directory.join(format!("{name}.lock"));
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|e| marker_error("open ownership lock", &lock_path, e))?;
    match lock.try_lock() {
        Ok(()) => Ok(SlotAccess::Acquired(MarkerSlot {
            // Keep the same lock inode across releases so contenders coordinate.
            _lock: lock,
            path: directory.join(format!("{name}.owner")),
        })),
        Err(TryLockError::WouldBlock) => Ok(SlotAccess::InUse),
        Err(TryLockError::Error(e)) => Err(marker_error("lock ownership", &lock_path, e)),
    }
}

struct MarkerSlot {
    _lock: File,
    path: PathBuf,
}

impl MarkerSlot {
    fn contents(adapter: &Adapter) -> String {
        format!("{}\n{}\n", adapter.path, adapter.name)
    }

    fn matches(&self, adapter: &Adapter) -> Result<bool, Error> {
        match fs::read(&self.path) {
            Ok(contents) => Ok(contents == Self::contents(adapter).as_bytes()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(marker_error("read ownership marker", &self.path, e)),
        }
    }

    fn write(&self, adapter: &Adapter) -> Result<(), Error> {
        let operation = || -> io::Result<()> {
            let mut file =
                tempfile::NamedTempFile::new_in(self.path.parent().expect("marker directory"))?;
            file.write_all(Self::contents(adapter).as_bytes())?;
            file.persist(&self.path)?;
            Ok(())
        };
        operation().map_err(|e| marker_error("write ownership marker", &self.path, e))
    }

    fn remove(&self) -> Result<(), Error> {
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            // An absent marker already has the desired state.
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(marker_error("remove ownership marker", &self.path, e)),
        }
    }
}

fn marker_error(operation: &'static str, path: &Path, source: io::Error) -> Error {
    Error::Io {
        operation,
        path: path.display().to_string(),
        source,
    }
}

// Only successful configuration or adoption can construct this release guard.
struct OwnedAdapter {
    adapter: Adapter,
    slot: MarkerSlot,
}

impl OwnedAdapter {
    fn release(&self, backend: &mut dyn Backend) -> Result<(), Error> {
        if !self.slot.matches(&self.adapter)? {
            return Ok(());
        }
        let release = backend.open(&self.adapter.path).and_then(|device| {
            if device.name() == self.adapter.name
                && device.logical_config()? == LogicalConfig::NagameTv
            {
                device.clear_logical_addresses()?;
            }
            Ok(())
        });
        match release {
            // The adapter can vanish between open and either ioctl.
            Ok(()) | Err(Error::NoDevice) => {}
            Err(error) => return Err(error),
        }
        self.slot.remove()
    }
}
