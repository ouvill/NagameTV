//! Apply the optional appliance connection seed through the preference owner.
use super::{Preferences, load, save, settings_path};
use crate::{epgstation::Endpoint, services::ServerUrl};
use std::{
    io::{self, Read},
    path::Path,
};

const MAX_SEED_BYTES: u64 = 8 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Could not read connection TOML: {0}")]
    Read(#[from] io::Error),
    #[error("Connection TOML exceeds {MAX_SEED_BYTES} bytes")]
    TooLarge,
    #[error("Expected connection TOML with a server URL and an optional epgstation_server URL")]
    Format,
    #[error("Invalid server URL: {0}")]
    Tuner(#[from] crate::services::Error),
    #[error("Use a server URL without credentials")]
    Credentials,
    #[error("Invalid epgstation_server URL: {0}")]
    Recordings(#[from] crate::epgstation::Error),
    #[error(transparent)]
    Settings(#[from] super::Error),
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Seed {
    server: String,
    epgstation_server: Option<String>,
}

struct Connection {
    server: ServerUrl,
    recordings: Option<Endpoint>,
}
impl Connection {
    fn read(input: impl Read) -> Result<Self, Error> {
        let mut bytes = Vec::new();
        input.take(MAX_SEED_BYTES + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_SEED_BYTES {
            return Err(Error::TooLarge);
        }
        // Full TOML diagnostics can include a URL with credentials.
        let text = std::str::from_utf8(&bytes).map_err(|_| Error::Format)?;
        let seed: Seed = toml::from_str(text).map_err(|_| Error::Format)?;
        let server = ServerUrl::parse(&seed.server)?;
        let address = url::Url::parse(server.as_str()).map_err(crate::services::Error::from)?;
        if !address.username().is_empty() || address.password().is_some() {
            return Err(Error::Credentials);
        }
        let recordings = seed
            .epgstation_server
            .as_deref()
            .map(Endpoint::parse)
            .transpose()?;
        Ok(Self { server, recordings })
    }

    fn apply(self, preferences: &mut Preferences) {
        if preferences.server != self.server.as_str() {
            preferences.server = self.server.as_str().to_owned();
            // An opaque service ID belongs to the previous server's catalogue.
            preferences.service_id.clear();
        }
        if let Some(recordings) = self.recordings {
            preferences.epgstation_server = recordings.as_str().to_owned();
        }
        preferences.autoplay = true;
    }

    fn install(self, path: &Path) -> Result<(), Error> {
        let mut preferences = load(path)?;
        self.apply(&mut preferences);
        save(path, &preferences)?;
        Ok(())
    }
}

pub fn import_connection(input: impl Read) -> Result<(), Error> {
    Connection::read(input)?.install(&settings_path()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installs_the_real_preferences_and_keeps_other_values()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("nagametv/settings.toml");
        save(
            &path,
            &Preferences {
                server: "http://previous.local:40772".into(),
                service_id: "123".into(),
                volume: super::super::Volume::from(63.0),
                ..Default::default()
            },
        )?;
        let seed = br#"server = "http://tuner.local:40772/"
epgstation_server = "https://recordings.local:8888""#;
        Connection::read(seed.as_slice())?.install(&path)?;
        let preferences = load(&path)?;
        assert_eq!(preferences.server, "http://tuner.local:40772");
        assert_eq!(
            preferences.epgstation_server,
            "https://recordings.local:8888"
        );
        assert!(preferences.autoplay);
        assert!(preferences.service_id.is_empty());
        assert_eq!(preferences.volume, super::super::Volume::from(63.0));
        Ok(())
    }

    #[test]
    fn requires_a_bounded_seed_with_valid_credential_free_urls() {
        for bad in [
            "",
            "server = true",
            "server = 'file:///tmp/config'",
            "server = 'http://user:secret@tuner/'",
            "server = 'http://tuner/?key=secret'",
            "server = 'http://tuner'\nepgstation_server = 'http://user:secret@recordings'",
            "server = 'http://tuner'\nautoplay = false",
        ] {
            let error = Connection::read(bad.as_bytes())
                .err()
                .expect("invalid seed");
            assert!(!error.to_string().contains("secret"));
        }
        assert!(matches!(
            Connection::read(vec![b' '; MAX_SEED_BYTES as usize + 1].as_slice()),
            Err(Error::TooLarge)
        ));
    }
}
