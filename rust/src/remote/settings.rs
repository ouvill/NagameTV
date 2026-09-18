//! Remote preferences have their own file: unrelated Viewer saves cannot replace them.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
};

const MAX_BYTES: u64 = 4096;
pub const DEFAULT_PORT: u16 = 50051;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Stored", into = "Stored")]
pub struct Settings {
    enabled: bool,
    endpoint: SocketAddr,
}

#[derive(Serialize, Deserialize)]
#[serde(default)]
struct Stored {
    enabled: bool,
    address: IpAddr,
    port: u16,
}

impl Default for Stored {
    fn default() -> Self {
        Self {
            enabled: false,
            address: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            port: DEFAULT_PORT,
        }
    }
}
impl Default for Settings {
    fn default() -> Self {
        Stored::default().try_into().expect("valid defaults")
    }
}
impl TryFrom<Stored> for Settings {
    type Error = String;
    fn try_from(value: Stored) -> Result<Self, Self::Error> {
        if value.port == 0 {
            return Err("remote port must be between 1 and 65535".into());
        }
        Ok(Self {
            enabled: value.enabled,
            endpoint: SocketAddr::new(value.address, value.port),
        })
    }
}
impl From<Settings> for Stored {
    fn from(value: Settings) -> Self {
        Self {
            enabled: value.enabled,
            address: value.endpoint.ip(),
            port: value.endpoint.port(),
        }
    }
}
impl Settings {
    pub fn parse(enabled: bool, address: &str, port: i32) -> Result<Self, String> {
        Stored {
            enabled,
            address: address
                .trim()
                .parse()
                .map_err(|_| "remote address must be an IP address")?,
            port: port
                .try_into()
                .map_err(|_| "remote port must be between 1 and 65535")?,
        }
        .try_into()
    }
    pub fn enabled(self) -> bool {
        self.enabled
    }
    pub fn endpoint(self) -> SocketAddr {
        self.endpoint
    }
}

/// Environment values affect only this process, including later UI edits.
pub struct Overrides {
    enabled: Option<bool>,
    address: Option<IpAddr>,
    port: Option<u16>,
}
impl Overrides {
    pub fn from_env() -> Result<Self, String> {
        fn read(name: &str) -> Result<Option<String>, String> {
            match std::env::var(name) {
                Ok(value) => Ok(Some(value)),
                Err(std::env::VarError::NotPresent) => Ok(None),
                Err(std::env::VarError::NotUnicode(_)) => Err(format!("{name} must be Unicode")),
            }
        }
        Self::parse(
            read("NAGAMETV_REMOTE_ENABLED")?.as_deref(),
            read("NAGAMETV_REMOTE_ADDR")?.as_deref(),
            read("NAGAMETV_REMOTE_PORT")?.as_deref(),
        )
    }
    fn parse(
        enabled: Option<&str>,
        address: Option<&str>,
        port: Option<&str>,
    ) -> Result<Self, String> {
        let mut result = Self {
            enabled: None,
            address: None,
            port: None,
        };
        if let Some(address) = address {
            // Preserve the original IP:port launch option; new configurations split them.
            if let Ok(endpoint) = address.parse::<SocketAddr>() {
                if port.is_some() {
                    return Err(
                        "use an IP-only NAGAMETV_REMOTE_ADDR with NAGAMETV_REMOTE_PORT".into(),
                    );
                }
                result.address = Some(endpoint.ip());
                result.port = Some(endpoint.port());
                result.enabled = Some(true);
            } else {
                result.address = Some(
                    address
                        .parse()
                        .map_err(|_| "NAGAMETV_REMOTE_ADDR must be an IP address")?,
                );
            }
        }
        if let Some(port) = port {
            result.port = Some(
                port.parse()
                    .map_err(|_| "NAGAMETV_REMOTE_PORT must be between 1 and 65535")?,
            );
        }
        if result.port == Some(0) {
            return Err("NAGAMETV_REMOTE_PORT must be between 1 and 65535".into());
        }
        if let Some(enabled) = enabled {
            result.enabled = Some(match enabled {
                "1" | "true" => true,
                "0" | "false" => false,
                _ => return Err("NAGAMETV_REMOTE_ENABLED must be 0, 1, false or true".into()),
            });
        }
        Ok(result)
    }
    fn apply(self, mut settings: Settings) -> Option<Settings> {
        if self.enabled.is_none() && self.address.is_none() && self.port.is_none() {
            return None;
        }
        if let Some(enabled) = self.enabled {
            settings.enabled = enabled;
        }
        if let Some(address) = self.address {
            settings.endpoint.set_ip(address);
        }
        if let Some(port) = self.port {
            settings.endpoint.set_port(port);
        }
        Some(settings)
    }
}

enum Persistence {
    File { path: PathBuf, saved: Settings },
    Session,
}
pub struct Preferences {
    current: Settings,
    persistence: Persistence,
}
impl Preferences {
    pub fn open(path: PathBuf) -> Result<Self, String> {
        let current = match fs::File::open(&path) {
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take(MAX_BYTES + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| e.to_string())?;
                if bytes.len() as u64 > MAX_BYTES {
                    return Err("remote settings exceed 4 KiB".into());
                }
                toml::from_str(std::str::from_utf8(&bytes).map_err(|e| e.to_string())?)
                    .map_err(|e| format!("remote settings: {e}"))?
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Settings::default(),
            Err(error) => return Err(error.to_string()),
        };
        Ok(Self {
            current,
            persistence: Persistence::File {
                path,
                saved: current,
            },
        })
    }
    pub fn transient(current: Settings) -> Self {
        Self {
            current,
            persistence: Persistence::Session,
        }
    }
    pub fn with_overrides(self, overrides: Overrides) -> Self {
        match overrides.apply(self.current) {
            Some(current) => Self::transient(current),
            None => self,
        }
    }
    pub fn current(&self) -> Settings {
        self.current
    }
    pub fn session_only(&self) -> bool {
        matches!(self.persistence, Persistence::Session)
    }
    pub fn configure(&mut self, current: Settings) -> Result<(), String> {
        self.current = current;
        match &mut self.persistence {
            Persistence::Session => Ok(()),
            Persistence::File { path, saved } => {
                if *saved == current {
                    return Ok(());
                }
                let parent = path
                    .parent()
                    .ok_or("remote settings directory is missing")?;
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                let text = toml::to_string_pretty(&current).map_err(|e| e.to_string())?;
                let mut temporary =
                    tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
                temporary
                    .write_all(text.as_bytes())
                    .map_err(|e| e.to_string())?;
                temporary.as_file().sync_all().map_err(|e| e.to_string())?;
                temporary.persist(&*path).map_err(|e| e.to_string())?;
                *saved = current;
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_overrides_and_validation() {
        let defaults = Settings::default();
        assert!(!defaults.enabled());
        assert_eq!(defaults.endpoint(), "0.0.0.0:50051".parse().unwrap());
        let enabled = Overrides::parse(Some("1"), None, None)
            .unwrap()
            .apply(defaults)
            .unwrap();
        assert!(enabled.enabled());
        assert_eq!(enabled.endpoint(), defaults.endpoint());
        let port = Overrides::parse(None, None, Some("50052"))
            .unwrap()
            .apply(enabled)
            .unwrap();
        assert!(port.enabled());
        assert_eq!(port.endpoint().port(), 50052);
        assert!(
            !Overrides::parse(None, None, Some("50052"))
                .unwrap()
                .apply(defaults)
                .unwrap()
                .enabled()
        );
        assert!(
            Overrides::parse(None, Some("127.0.0.1:50053"), None)
                .unwrap()
                .apply(defaults)
                .unwrap()
                .enabled()
        );
        assert!(
            !Overrides::parse(Some("0"), Some("127.0.0.1:50053"), None)
                .unwrap()
                .apply(defaults)
                .unwrap()
                .enabled()
        );
        for (enabled, address, port) in [
            (Some("yes"), None, None),
            (None, Some("localhost"), None),
            (None, None, Some("0")),
            (None, None, Some("65536")),
            (None, Some("127.0.0.1:5"), Some("6")),
        ] {
            assert!(Overrides::parse(enabled, address, port).is_err());
        }
        for port in [-1, 0, 65536] {
            assert!(Settings::parse(true, "::1", port).is_err());
        }
    }
    #[test]
    fn preferences_roundtrip_and_launch_overrides_do_not_replace_shared_settings() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("remote-control.toml");
        let mut first = Preferences::open(path.clone()).unwrap();
        let stale = Preferences::open(path.clone()).unwrap();
        let enabled = Settings::parse(true, "0.0.0.0", 50051).unwrap();
        first.configure(enabled).unwrap();
        assert_eq!(Preferences::open(path.clone()).unwrap().current(), enabled);
        let mut override_session =
            stale.with_overrides(Overrides::parse(Some("1"), None, Some("50052")).unwrap());
        override_session
            .configure(Settings::parse(false, "::1", 50053).unwrap())
            .unwrap();
        assert_eq!(Preferences::open(path.clone()).unwrap().current(), enabled);
        let disabled = Settings::parse(false, "0.0.0.0", 50051).unwrap();
        first.configure(disabled).unwrap();
        assert_eq!(Preferences::open(path.clone()).unwrap().current(), disabled);
        fs::write(&path, "port = 0").unwrap();
        assert!(Preferences::open(path).is_err());
    }
}
