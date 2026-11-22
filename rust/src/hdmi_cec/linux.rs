//! Linux CEC ioctl boundary. Adapter descriptors are scoped to one operation.
use super::{
    DeviceInfo, Error, OSD_NAME,
    ownership::{Backend, Device, LogicalConfig},
    valid_path,
};
use std::{
    fs::{self, File, OpenOptions},
    io,
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
};

impl Error {
    fn device_io(operation: &'static str, path: &str, source: io::Error) -> Self {
        match (source.kind(), source.raw_os_error()) {
            (io::ErrorKind::NotFound, _) | (_, Some(libc::ENODEV | libc::ENXIO)) => Self::NoDevice,
            (io::ErrorKind::PermissionDenied, _) => Self::Permission {
                operation,
                path: path.into(),
                source,
            },
            _ => Self::Io {
                operation,
                path: path.into(),
                source,
            },
        }
    }
}

const CEC_IOCTL_TYPE: u32 = b'a' as u32;
const MAX_LOGICAL_ADDRESSES: usize = 4;
const CAP_LOG_ADDRS: u32 = 1 << 1;
const CAP_RC: u32 = 1 << 4;
const VERSION_1_4: u8 = 5;
const PRIMARY_PLAYBACK: u8 = 4;
const LOG_ADDR_TYPE_PLAYBACK: u8 = 3;
const VENDOR_ID: u32 = 0x000c03;
const ALLOW_RC_PASSTHRU: u32 = 1 << 1;
const INVALID_PHYSICAL_ADDRESS: u16 = u16::MAX;

#[repr(C)]
#[derive(Default)]
struct Caps {
    driver: [u8; 32],
    name: [u8; 32],
    available_log_addrs: u32,
    capabilities: u32,
    version: u32,
}
#[repr(C)]
#[derive(Default)]
struct LogAddrs {
    log_addr: [u8; MAX_LOGICAL_ADDRESSES],
    log_addr_mask: u16,
    cec_version: u8,
    num_log_addrs: u8,
    vendor_id: u32,
    flags: u32,
    osd_name: [u8; 15],
    primary_device_type: [u8; MAX_LOGICAL_ADDRESSES],
    log_addr_type: [u8; MAX_LOGICAL_ADDRESSES],
    all_device_types: [u8; MAX_LOGICAL_ADDRESSES],
    features: [[u8; 12]; MAX_LOGICAL_ADDRESSES],
}

// The private trait binds each integer-only C layout to its own ioctl requests.
// No caller can supply a request whose buffer size differs from the value.
trait Uapi {
    fn request(write: bool) -> libc::Ioctl;
}
impl Uapi for Caps {
    fn request(_: bool) -> libc::Ioctl {
        libc::_IOWR::<Self>(CEC_IOCTL_TYPE, 0)
    }
}
impl Uapi for u16 {
    fn request(_: bool) -> libc::Ioctl {
        libc::_IOR::<Self>(CEC_IOCTL_TYPE, 1)
    }
}
impl Uapi for LogAddrs {
    fn request(write: bool) -> libc::Ioctl {
        if write {
            libc::_IOWR::<Self>(CEC_IOCTL_TYPE, 4)
        } else {
            libc::_IOR::<Self>(CEC_IOCTL_TYPE, 3)
        }
    }
}
fn ioctl<T: Uapi>(file: &File, value: &mut T, write: bool) -> io::Result<()> {
    // SAFETY: Uapi is private and implemented only for the initialized integer
    // layouts above, matching linux/cec.h. The unique buffer is valid for the
    // entire synchronous ioctl and the owned File keeps its descriptor alive.
    let result = unsafe { libc::ioctl(file.as_raw_fd(), T::request(write), value as *mut T) };
    if result < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
fn open(path: &str) -> Result<File, Error> {
    // S_LOG_ADDRS retains the request even without a physical address. With
    // one, O_NONBLOCK lets the kernel claim addresses without waiting here.
    // Keep the default, shared initiator mode.
    OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(path)
        .map_err(|e| Error::device_io("open", path, e))
}
fn caps(file: &File, path: &str) -> Result<Caps, Error> {
    let mut caps = Caps::default();
    ioctl(file, &mut caps, false).map_err(|e| Error::device_io("query capabilities", path, e))?;
    Ok(caps)
}
fn physical_address(file: &File, path: &str) -> Result<Option<u16>, Error> {
    let mut physical = INVALID_PHYSICAL_ADDRESS;
    ioctl(file, &mut physical, false)
        .map_err(|e| Error::device_io("query physical address", path, e))?;
    Ok((physical != INVALID_PHYSICAL_ADDRESS).then_some(physical))
}

impl LogAddrs {
    fn playback() -> Self {
        let mut requested = Self {
            cec_version: VERSION_1_4,
            num_log_addrs: 1,
            vendor_id: VENDOR_ID,
            flags: ALLOW_RC_PASSTHRU,
            ..Default::default()
        };
        requested.osd_name[..OSD_NAME.len()].copy_from_slice(OSD_NAME.as_bytes());
        requested.primary_device_type[0] = PRIMARY_PLAYBACK;
        requested.log_addr_type[0] = LOG_ADDR_TYPE_PLAYBACK;
        requested
    }

    fn classify(&self) -> LogicalConfig {
        // num_log_addrs is the requested count. The claimed addresses may be
        // invalid while the TV is off or the kernel is still polling the bus.
        if self.num_log_addrs == 0 {
            LogicalConfig::Empty
        } else if self.num_log_addrs == 1
            && self.primary_device_type[0] == PRIMARY_PLAYBACK
            && self.log_addr_type[0] == LOG_ADDR_TYPE_PLAYBACK
            && self.cec_version == VERSION_1_4
            && self.vendor_id == VENDOR_ID
            && self.flags == ALLOW_RC_PASSTHRU
            && c_string(&self.osd_name) == OSD_NAME.as_bytes()
        {
            LogicalConfig::NagameTv
        } else {
            LogicalConfig::External {
                rc_passthrough: self.flags & ALLOW_RC_PASSTHRU != 0,
            }
        }
    }
}

fn c_string(bytes: &[u8]) -> &[u8] {
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    &bytes[..end]
}

pub(super) struct Linux;

struct LinuxDevice {
    file: File,
    path: String,
    name: String,
    driver: String,
    capabilities: u32,
    available_log_addrs: u32,
}

impl LinuxDevice {
    fn open(path: &str) -> Result<Self, Error> {
        let file = open(path)?;
        let caps = caps(&file, path)?;
        Ok(Self {
            file,
            path: path.into(),
            name: String::from_utf8_lossy(c_string(&caps.name)).into_owned(),
            driver: String::from_utf8_lossy(c_string(&caps.driver)).into_owned(),
            capabilities: caps.capabilities,
            available_log_addrs: caps.available_log_addrs,
        })
    }

    fn discover(path: String) -> DeviceInfo {
        match Self::open(&path) {
            Ok(device) => {
                let physical_address = match device.physical_address() {
                    Ok(physical) => physical,
                    Err(error) => {
                        tracing::debug!(%error, "HDMI-CEC physical address probe failed");
                        None
                    }
                };
                DeviceInfo {
                    path,
                    name: device.name,
                    driver: device.driver,
                    physical_address,
                }
            }
            Err(error) => {
                tracing::debug!(%error, "HDMI-CEC capability probe failed");
                // Keep inaccessible nodes selectable so the open failure is reported.
                DeviceInfo {
                    name: path.clone(),
                    path,
                    driver: String::new(),
                    physical_address: None,
                }
            }
        }
    }
}

impl Backend for Linux {
    fn enumerate(&mut self) -> Result<Vec<DeviceInfo>, Error> {
        let entries =
            fs::read_dir("/dev").map_err(|e| Error::device_io("enumerate adapters", "/dev", e))?;
        let mut devices = Vec::new();
        for entry in entries {
            let entry =
                entry.map_err(|e| Error::device_io("enumerate adapter entry", "/dev", e))?;
            let path = entry.path().to_string_lossy().into_owned();
            if valid_path(&path) {
                devices.push(LinuxDevice::discover(path));
            }
        }
        devices.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(devices)
    }

    fn open(&mut self, path: &str) -> Result<Box<dyn Device>, Error> {
        Ok(Box::new(LinuxDevice::open(path)?))
    }
}

impl Device for LinuxDevice {
    fn name(&self) -> &str {
        &self.name
    }

    fn supports_logical_addresses(&self) -> bool {
        self.capabilities & CAP_LOG_ADDRS != 0 && self.available_log_addrs > 0
    }

    fn supports_remote_control(&self) -> bool {
        self.capabilities & CAP_RC != 0
    }

    fn physical_address(&self) -> Result<Option<u16>, Error> {
        physical_address(&self.file, &self.path)
    }

    fn logical_config(&self) -> Result<LogicalConfig, Error> {
        let mut current = LogAddrs::default();
        ioctl(&self.file, &mut current, false)
            .map_err(|e| Error::device_io("read logical addresses", &self.path, e))?;
        Ok(current.classify())
    }

    fn configure_playback(&self) -> Result<(), Error> {
        ioctl(&self.file, &mut LogAddrs::playback(), true)
            .map_err(|e| Error::device_io("configure Playback logical address", &self.path, e))
    }

    fn clear_logical_addresses(&self) -> Result<(), Error> {
        ioctl(&self.file, &mut LogAddrs::default(), true)
            .map_err(|e| Error::device_io("clear logical addresses", &self.path, e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ioctl_layouts_match_linux_cec_uapi() {
        assert_eq!(size_of::<Caps>(), 76);
        assert_eq!(size_of::<LogAddrs>(), 92);
    }

    #[test]
    fn only_the_complete_playback_identity_is_ours_even_before_a_claim() {
        let mut config = LogAddrs::playback();
        config.log_addr = [u8::MAX; MAX_LOGICAL_ADDRESSES];
        assert_eq!(config.classify(), LogicalConfig::NagameTv);
        config.flags = 0;
        assert_eq!(
            config.classify(),
            LogicalConfig::External {
                rc_passthrough: false
            }
        );
        config.flags = ALLOW_RC_PASSTHRU;
        config.cec_version += 1;
        assert_eq!(
            config.classify(),
            LogicalConfig::External {
                rc_passthrough: true
            }
        );
        config.num_log_addrs = 0;
        assert_eq!(config.classify(), LogicalConfig::Empty);
    }
}
