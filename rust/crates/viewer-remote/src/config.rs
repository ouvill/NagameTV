use std::net::SocketAddr;

/// Explicitly enabled listener configuration for a trusted home LAN.
pub struct Config {
    pub(crate) web_enabled: bool,
    pub(crate) address: SocketAddr,
}

impl Config {
    pub fn with_web_enabled(mut self, enabled: bool) -> Self {
        self.web_enabled = enabled;
        self
    }
    pub fn new(address: SocketAddr) -> Self {
        Self {
            address,
            web_enabled: false,
        }
    }
}
