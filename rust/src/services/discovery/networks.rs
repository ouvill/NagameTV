use std::{collections::BTreeSet, io, net::Ipv4Addr};

#[derive(Clone, Copy, Ord, PartialOrd, Eq, PartialEq)]
pub(super) struct Network {
    base: Ipv4Addr,
    prefix: u8,
    // Keep the true subnet bounds to exclude its network/broadcast addresses,
    // which can differ from an artificial /24 boundary on a larger subnet.
    unusable: Option<(Ipv4Addr, Ipv4Addr)>,
}

impl std::fmt::Debug for Network {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.base, self.prefix)
    }
}

impl Network {
    pub(super) fn from_address(ip: Ipv4Addr, mask: Ipv4Addr) -> Option<Self> {
        let mask = u32::from(mask);
        let prefix = mask.leading_ones();
        if !ip.is_private() || mask.count_ones() != prefix {
            return None;
        }
        let subnet = u32::from(ip) & mask;
        let scan_mask = mask | 0xffff_ff00;
        Some(Self {
            base: (u32::from(ip) & scan_mask).into(),
            prefix: prefix.max(24) as u8,
            unusable: (prefix <= 30).then(|| (subnet.into(), (subnet | !mask).into())),
        })
    }
}

pub(super) fn addresses(networks: &BTreeSet<Network>) -> Vec<Ipv4Addr> {
    let mut result = vec![Ipv4Addr::LOCALHOST];
    let mut seen = BTreeSet::new();
    // Interleave subnets so different interfaces can make progress inside one deadline.
    for offset in 0..256 {
        for network in networks {
            if offset >= 1 << (32 - network.prefix) {
                continue;
            }
            let candidate = Ipv4Addr::from(u32::from(network.base) + offset);
            if network
                .unusable
                .is_some_and(|(first, last)| candidate == first || candidate == last)
            {
                continue;
            }
            if seen.insert(candidate) {
                result.push(candidate);
            }
        }
    }
    result
}

#[cfg(feature = "native_tests")]
static LOOPBACK_ONLY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(feature = "native_tests")]
pub(crate) fn limit_to_test_loopback() {
    LOOPBACK_ONLY.store(true, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(unix)]
pub(super) fn local_networks() -> io::Result<BTreeSet<Network>> {
    #[cfg(feature = "native_tests")]
    if LOOPBACK_ONLY.load(std::sync::atomic::Ordering::Relaxed) {
        return Ok(BTreeSet::new());
    }
    struct Interfaces(*mut libc::ifaddrs);
    impl Drop for Interfaces {
        fn drop(&mut self) {
            // SAFETY: getifaddrs allocated this list, and no references are retained.
            unsafe { libc::freeifaddrs(self.0) };
        }
    }
    let mut list = std::ptr::null_mut();
    // SAFETY: list is a valid out parameter for libc's owned interface list.
    if unsafe { libc::getifaddrs(&mut list) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let list = Interfaces(list);
    let mut cursor = list.0;
    let mut result = BTreeSet::new();
    // SAFETY: until Interfaces drops, each non-null pointer in the list is
    // owned and initialized by getifaddrs. AF_INET entries hold sockaddr_in.
    unsafe {
        while let Some(interface) = cursor.as_ref() {
            cursor = interface.ifa_next;
            if interface.ifa_flags & libc::IFF_UP as libc::c_uint == 0 {
                continue;
            }
            let (Some(address), Some(mask)) =
                (interface.ifa_addr.as_ref(), interface.ifa_netmask.as_ref())
            else {
                continue;
            };
            if address.sa_family as libc::c_int != libc::AF_INET
                || mask.sa_family as libc::c_int != libc::AF_INET
            {
                continue;
            }
            let address = (*interface.ifa_addr.cast::<libc::sockaddr_in>())
                .sin_addr
                .s_addr;
            let mask = (*interface.ifa_netmask.cast::<libc::sockaddr_in>())
                .sin_addr
                .s_addr;
            if let Some(network) =
                Network::from_address(address.to_ne_bytes().into(), mask.to_ne_bytes().into())
            {
                result.insert(network);
            }
        }
    }
    Ok(result)
}

#[cfg(not(unix))]
pub(super) fn local_networks() -> io::Result<BTreeSet<Network>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "LAN interface enumeration is unavailable on this platform",
    ))
}
