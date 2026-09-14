use std::net::SocketAddr;

/// Explicitly enabled listener configuration for a trusted home LAN.
pub struct Config {
    pub(crate) address: SocketAddr,
}

impl Config {
    pub fn new(address: SocketAddr) -> Self {
        Self { address }
    }
}
