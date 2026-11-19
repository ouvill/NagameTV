//! Linux CEC uAPI, isolated here so other platforms never reference libc or /dev.
//! Layouts and request numbers follow linux/cec.h. Only fixed-width integer
//! fields cross the kernel boundary; unknown kernel values are never Rust enums.
use super::{Address, Backend, Device, DeviceInfo, Error, Event, OSD_NAME, valid_path};
use std::{
    fs::{self, File, OpenOptions},
    io,
    os::{
        fd::{AsRawFd, RawFd},
        unix::fs::OpenOptionsExt,
    },
    time::Duration,
};

impl Error {
    pub(super) fn io(operation: &'static str, path: &str, source: io::Error) -> Self {
        if source.kind() == io::ErrorKind::PermissionDenied {
            Self::Permission {
                path: path.into(),
                source,
            }
        } else if matches!(source.raw_os_error(), Some(libc::ENODEV | libc::ENXIO)) {
            // Linux ENODEV/ENXIO: an unplugged adapter is distinct from no adapter at startup.
            Self::Disconnected {
                path: path.into(),
                source,
            }
        } else {
            Self::Io {
                operation,
                path: path.into(),
                source,
            }
        }
    }
}

const CEC_IOCTL_TYPE: u32 = b'a' as u32;
const MAX_MESSAGE_SIZE: usize = 16;
const MAX_LOGICAL_ADDRESSES: usize = 4;
const CAP_LOG_ADDRS: u32 = 1 << 1;
const CAP_TRANSMIT: u32 = 1 << 2;
const MODE_EXCLUSIVE_INITIATOR: u32 = 0x02;
const MODE_EXCLUSIVE_FOLLOWER: u32 = 0x20;
const VERSION_1_4: u8 = 5;
const PRIMARY_PLAYBACK: u8 = 4;
const LOG_ADDR_TYPE_PLAYBACK: u8 = 3;
// Use the same HDMI LLC compatibility ID as cec-ctl, so ordinary vendor
// discovery succeeds. libCEC's LG ID invokes a private SIMPLINK handshake.
// The captured TV traffic only demonstrates standard discovery and routing.
const VENDOR_ID: u32 = 0x000c03;
const INVALID_PHYSICAL_ADDRESS: u16 = u16::MAX;
const EVENT_STATE_CHANGE: u32 = 1;
const EVENT_LOST_MESSAGES: u32 = 2;
const EVENT_DROPPED: u32 = 1 << 1;
const USER_CONTROL_PRESSED: u8 = 0x44;
const USER_CONTROL_RELEASED: u8 = 0x45;
const RX_OK: u8 = 1;
const BROADCAST: u8 = 15;

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
#[repr(C)]
#[derive(Default)]
struct Message {
    tx_ts: u64,
    rx_ts: u64,
    len: u32,
    timeout: u32,
    sequence: u32,
    flags: u32,
    msg: [u8; MAX_MESSAGE_SIZE],
    reply: u8,
    rx_status: u8,
    tx_status: u8,
    tx_arb_lost_cnt: u8,
    tx_nack_cnt: u8,
    tx_low_drive_cnt: u8,
    tx_error_cnt: u8,
}
#[repr(C)]
#[derive(Default)]
struct KernelEvent {
    ts: u64,
    event: u32,
    flags: u32,
    raw: [u32; 16],
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
impl Uapi for Message {
    fn request(_: bool) -> libc::Ioctl {
        libc::_IOWR::<Self>(CEC_IOCTL_TYPE, 6)
    }
}
impl Uapi for KernelEvent {
    fn request(_: bool) -> libc::Ioctl {
        libc::_IOWR::<Self>(CEC_IOCTL_TYPE, 7)
    }
}
impl Uapi for u32 {
    fn request(_: bool) -> libc::Ioctl {
        libc::_IOW::<Self>(CEC_IOCTL_TYPE, 9)
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
    OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(path)
        .map_err(|e| Error::io("open", path, e))
}
fn caps(file: &File, path: &str) -> Result<Caps, Error> {
    let mut caps = Caps::default();
    ioctl(file, &mut caps, false).map_err(|e| Error::io("query capabilities", path, e))?;
    Ok(caps)
}
fn physical_address(file: &File, path: &str) -> Result<Option<u16>, Error> {
    let mut physical = INVALID_PHYSICAL_ADDRESS;
    ioctl(file, &mut physical, false).map_err(|e| Error::io("query physical address", path, e))?;
    Ok((physical != INVALID_PHYSICAL_ADDRESS).then_some(physical))
}

#[derive(Debug, PartialEq, Eq)]
enum Registration {
    Fresh,
    Reused,
    Reconfigured,
}

fn configure_playback(
    path: &str,
    current: &LogAddrs,
    mut set: impl FnMut(&mut LogAddrs) -> Result<(), Error>,
) -> Result<Registration, Error> {
    let mut requested = LogAddrs {
        cec_version: VERSION_1_4,
        num_log_addrs: 1,
        vendor_id: VENDOR_ID,
        ..Default::default()
    };
    requested.osd_name[..OSD_NAME.len()].copy_from_slice(OSD_NAME.as_bytes());
    requested.primary_device_type[0] = PRIMARY_PLAYBACK;
    requested.log_addr_type[0] = LOG_ADDR_TYPE_PLAYBACK;

    let registration = if current.num_log_addrs == 0 {
        Registration::Fresh
    } else {
        if current.num_log_addrs != 1
            || current.primary_device_type[0] != PRIMARY_PLAYBACK
            || current.log_addr_type[0] != LOG_ADDR_TYPE_PLAYBACK
            || text(&current.osd_name) != OSD_NAME
        {
            return Err(Error::Busy(path.into()));
        }
        if current.vendor_id == VENDOR_ID
            && current.cec_version == VERSION_1_4
            && current.flags == requested.flags
        {
            return Ok(Registration::Reused);
        }
        // Linux retains the LA configuration after close and requires a clear
        // before a replacement. Only replace our identity under exclusive mode.
        set(&mut LogAddrs::default())?;
        Registration::Reconfigured
    };
    // Keep RC passthrough disabled so the kernel does not also inject OS keys.
    set(&mut requested)?;
    Ok(registration)
}

fn text(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}
#[derive(Clone)]
pub(super) struct Linux;
pub(super) struct LinuxDevice {
    file: File,
    path: String,
}
impl Backend for Linux {
    type Device = LinuxDevice;
    fn enumerate(&mut self) -> Result<Vec<DeviceInfo>, Error> {
        let entries =
            fs::read_dir("/dev").map_err(|e| Error::io("enumerate adapters", "/dev", e))?;
        let mut devices = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| Error::io("enumerate adapter entry", "/dev", e))?;
            let path = entry.path().to_string_lossy().into_owned();
            if !valid_path(&path) {
                continue;
            }
            let info = match open(&path).and_then(|file| {
                let caps = caps(&file, &path)?;
                let physical = match physical_address(&file, &path) {
                    Ok(physical) => physical,
                    Err(error) => {
                        tracing::debug!(
                            error = &error as &dyn std::error::Error,
                            "HDMI-CEC physical address probe failed"
                        );
                        None
                    }
                };
                Ok((caps, physical))
            }) {
                Ok((caps, physical_address)) => DeviceInfo {
                    path,
                    name: text(&caps.name),
                    driver: text(&caps.driver),
                    physical_address,
                },
                Err(error) => {
                    // Discovery is a probe; opening the selected adapter reports
                    // the operational failure with the worker's retry policy.
                    tracing::debug!(
                        error = &error as &dyn std::error::Error,
                        "HDMI-CEC capability discovery failed"
                    );
                    // Keep inaccessible adapters selectable so open reports a useful error.
                    DeviceInfo {
                        name: path.clone(),
                        path,
                        driver: String::new(),
                        physical_address: None,
                    }
                }
            };
            devices.push(info);
        }
        devices.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(devices)
    }
    fn open(&mut self, info: &DeviceInfo) -> Result<Self::Device, Error> {
        let file = open(&info.path)?;
        let caps = caps(&file, &info.path)?;
        if caps.capabilities & (CAP_LOG_ADDRS | CAP_TRANSMIT) != CAP_LOG_ADDRS | CAP_TRANSMIT
            || caps.available_log_addrs == 0
        {
            return Err(Error::Capabilities(info.path.clone()));
        }
        let mut mode = MODE_EXCLUSIVE_INITIATOR | MODE_EXCLUSIVE_FOLLOWER;
        ioctl(&file, &mut mode, true).map_err(|error| {
            if error.raw_os_error() == Some(libc::EBUSY) {
                Error::Busy(info.path.clone())
            } else {
                Error::io("acquire exclusive follower", &info.path, error)
            }
        })?;
        Ok(LinuxDevice {
            file,
            path: info.path.clone(),
        })
    }
}
impl Device for LinuxDevice {
    fn claim_playback(&mut self) -> Result<(), Error> {
        let mut current = LogAddrs::default();
        ioctl(&self.file, &mut current, false)
            .map_err(|e| Error::io("query logical address configuration", &self.path, e))?;
        let registration = configure_playback(&self.path, &current, |addresses| {
            let operation = if addresses.num_log_addrs == 0 {
                "clear previous Playback address"
            } else {
                "claim Playback address"
            };
            ioctl(&self.file, addresses, true).map_err(|e| Error::io(operation, &self.path, e))
        })?;
        tracing::info!(
            path = self.path,
            ?registration,
            vendor_id = VENDOR_ID,
            "HDMI-CEC Playback address configuration"
        );
        Ok(())
    }
    fn address(&mut self) -> Result<Address, Error> {
        let physical = physical_address(&self.file, &self.path)?;
        let mut addresses = LogAddrs::default();
        ioctl(&self.file, &mut addresses, false)
            .map_err(|e| Error::io("query logical address", &self.path, e))?;
        Ok(Address {
            physical,
            logical_mask: addresses.log_addr_mask,
        })
    }
    fn next_event(&mut self, timeout: Duration) -> Result<Option<Event>, Error> {
        let ready = poll(self.file.as_raw_fd(), timeout)
            .map_err(|e| Error::io("poll adapter", &self.path, e))?;
        if ready & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0 {
            return Err(Error::Disconnected {
                path: self.path.clone(),
                source: io::Error::new(
                    io::ErrorKind::NotConnected,
                    format!("CEC poll returned error/hangup flags {ready:#x}"),
                ),
            });
        }
        if ready & libc::POLLPRI != 0 {
            let mut event = KernelEvent::default();
            match ioctl(&self.file, &mut event, false) {
                Ok(()) => {
                    if event.flags & EVENT_DROPPED != 0 {
                        tracing::warn!("HDMI-CEC state events dropped; refreshing adapter state");
                        return self.address().map(|a| Some(Event::StateChange(a)));
                    }
                    match event.event {
                        EVENT_STATE_CHANGE => {
                            return self.address().map(|a| Some(Event::StateChange(a)));
                        }
                        EVENT_LOST_MESSAGES => return Ok(Some(Event::LostMessages(event.raw[0]))),
                        code => tracing::debug!(code, "Unknown HDMI-CEC event ignored"),
                    }
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) => {}
                Err(e) => return Err(Error::io("receive state event", &self.path, e)),
            }
        }
        if ready & libc::POLLIN != 0 {
            let mut message = Message::default();
            match ioctl(&self.file, &mut message, false) {
                Ok(()) => return Ok(decode(&message)),
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) => {}
                Err(e) => return Err(Error::io("receive key", &self.path, e)),
            }
        }
        Ok(None)
    }
    fn release(&mut self) -> Result<(), Error> {
        ioctl(&self.file, &mut LogAddrs::default(), true)
            .map_err(|e| Error::io("release logical address", &self.path, e))
    }
}
fn poll(fd: RawFd, timeout: Duration) -> io::Result<i16> {
    let mut descriptor = libc::pollfd {
        fd,
        events: libc::POLLIN | libc::POLLPRI,
        revents: 0,
    };
    // SAFETY: descriptor is one initialized, writable pollfd and fd is borrowed
    // from the worker's File. poll cannot retain this pointer after it returns.
    let result = unsafe {
        libc::poll(
            &mut descriptor,
            1,
            timeout
                .as_millis()
                .try_into()
                .expect("bounded CEC poll timeout"),
        )
    };
    if result < 0 {
        let error = io::Error::last_os_error();
        if error.kind() == io::ErrorKind::Interrupted {
            Ok(0)
        } else {
            Err(error)
        }
    } else {
        Ok(descriptor.revents)
    }
}
fn decode(message: &Message) -> Option<Event> {
    if message.rx_status & RX_OK == 0 || !(2..=MAX_MESSAGE_SIZE as u32).contains(&message.len) {
        return None;
    }
    let initiator = message.msg[0] >> 4;
    let destination = message.msg[0] & 0x0f;
    if initiator == BROADCAST || destination == BROADCAST {
        return None;
    }
    match (message.msg[1], message.len) {
        (USER_CONTROL_PRESSED, 3) => Some(Event::Press {
            initiator,
            code: message.msg[2],
        }),
        (USER_CONTROL_RELEASED, 2) => Some(Event::Release { initiator }),
        (opcode, _) => {
            tracing::debug!(opcode, length = message.len, "HDMI-CEC message ignored");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn playback_addresses(vendor_id: u32) -> LogAddrs {
        let mut addresses = LogAddrs {
            log_addr: [8, 0xff, 0xff, 0xff],
            log_addr_mask: 1 << 8,
            cec_version: VERSION_1_4,
            num_log_addrs: 1,
            vendor_id,
            ..Default::default()
        };
        addresses.primary_device_type[0] = PRIMARY_PLAYBACK;
        addresses.log_addr_type[0] = LOG_ADDR_TYPE_PLAYBACK;
        addresses.osd_name[..OSD_NAME.len()].copy_from_slice(OSD_NAME.as_bytes());
        addresses
    }

    #[test]
    fn stale_nagametv_registration_is_cleared_then_updated_and_can_be_reused() {
        let mut writes = Vec::new();
        let registration = configure_playback("/dev/cec0", &playback_addresses(u32::MAX), |args| {
            writes.push((args.num_log_addrs, args.vendor_id, text(&args.osd_name)));
            Ok(())
        })
        .unwrap();
        assert_eq!(registration, Registration::Reconfigured);
        assert_eq!(
            writes,
            [(0, 0, String::new()), (1, 0x000c03, "NagameTV".into())]
        );
        let registration = configure_playback("/dev/cec0", &playback_addresses(VENDOR_ID), |_| {
            panic!("the retained Playback address should be reused without ioctl writes")
        })
        .unwrap();
        assert_eq!(registration, Registration::Reused);
    }

    #[test]
    fn another_app_or_device_type_keeps_its_registration() {
        let mut foreign_name = playback_addresses(VENDOR_ID);
        foreign_name.osd_name[0] = b'X';
        let mut foreign_type = playback_addresses(VENDOR_ID);
        foreign_type.log_addr_type[0] = 1;
        for current in [foreign_name, foreign_type] {
            assert!(matches!(
                configure_playback("/dev/cec0", &current, |_| panic!("foreign configuration changed")),
                Err(Error::Busy(path)) if path == "/dev/cec0"
            ));
        }
    }

    #[test]
    fn raw_uapi_layouts_match_linux_cec_h() {
        assert_eq!(size_of::<Caps>(), 76);
        assert_eq!(size_of::<LogAddrs>(), 92);
        assert_eq!(size_of::<Message>(), 56);
        assert_eq!(size_of::<KernelEvent>(), 80);
        assert_eq!(std::mem::offset_of!(LogAddrs, osd_name), 16);
        assert_eq!(std::mem::offset_of!(Message, msg), 32);
    }
    #[test]
    fn malformed_unknown_and_broadcast_messages_cannot_become_keys() {
        let mut message = Message {
            len: 3,
            rx_status: RX_OK,
            ..Default::default()
        };
        message.msg[..3].copy_from_slice(&[4, USER_CONTROL_PRESSED, 1]);
        assert!(matches!(
            decode(&message),
            Some(Event::Press {
                initiator: 0,
                code: 1
            })
        ));
        for length in [0, 1, 2, 4, 17, u32::MAX] {
            message.len = length;
            assert!(decode(&message).is_none());
        }
        message.len = 3;
        message.msg[0] = 0x0f;
        assert!(decode(&message).is_none());
        message.msg[0] = 0xf4;
        assert!(decode(&message).is_none());
        message.msg[0] = 4;
        message.msg[1] = 0xff;
        assert!(decode(&message).is_none());
        message.msg[1] = USER_CONTROL_RELEASED;
        message.len = 2;
        assert!(matches!(
            decode(&message),
            Some(Event::Release { initiator: 0 })
        ));
    }
}
