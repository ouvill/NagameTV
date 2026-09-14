//! Read actual interface addresses; wildcard listen addresses are not client destinations.
#![forbid(unsafe_code)]

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

pub(super) fn for_listener(listener: SocketAddr) -> Vec<String> {
    if !listener.ip().is_unspecified() {
        return vec![listener.to_string()];
    }
    let mut addresses = local_addresses()
        .into_iter()
        .filter(|ip| {
            ip.is_ipv4() == listener.is_ipv4() && !ip.is_loopback() && !ip.is_unspecified()
        })
        .map(|ip| SocketAddr::new(ip, listener.port()).to_string())
        .collect::<Vec<_>>();
    addresses.sort();
    addresses.dedup();
    if addresses.is_empty() {
        let ip = if listener.is_ipv4() {
            IpAddr::V4(Ipv4Addr::LOCALHOST)
        } else {
            IpAddr::V6(Ipv6Addr::LOCALHOST)
        };
        addresses.push(SocketAddr::new(ip, listener.port()).to_string());
    }
    addresses
}

fn local_addresses() -> Vec<IpAddr> {
    match local_ip_address::list_afinet_netifas() {
        Ok(interfaces) => interfaces
            .into_iter()
            .map(|(_, ip)| ip)
            // Link-local IPv6 destinations need a client-side interface scope.
            .filter(|ip| match ip {
                IpAddr::V4(_) => true,
                IpAddr::V6(ip) => !ip.is_unicast_link_local(),
            })
            .collect(),
        Err(error) => {
            tracing::warn!("Could not list remote connection addresses: {error}");
            Vec::new()
        }
    }
}
