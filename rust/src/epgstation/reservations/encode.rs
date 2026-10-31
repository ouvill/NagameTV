//! Optional server capabilities do not gate reservation reads or writes.
use super::{Error, http};
use crate::epgstation::client::Connection;
use crate::services::{FetchError, Job, Network, Progress, Stopping};
use serde::{Deserialize, Serialize};

const MAX_MODES: usize = 64;
const MAX_MODE_CHARS: usize = 256;

#[derive(Deserialize)]
struct Config {
    encode: Vec<String>,
}

pub(super) struct Modes(Vec<String>);
impl Modes {
    fn validate(config: Config) -> Result<Self, Error> {
        if config.encode.len() > MAX_MODES
            || config
                .encode
                .iter()
                .any(|mode| mode.chars().count() > MAX_MODE_CHARS)
        {
            return Err(Error::Limit);
        }
        if config.encode.iter().any(String::is_empty) {
            return Err(Error::Invalid);
        }
        let mut modes = Vec::new();
        for mode in config.encode {
            if !modes.contains(&mode) {
                modes.push(mode);
            }
        }
        Ok(Self(modes))
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Options {
    mode1: String,
    is_delete_original_after_encode: bool,
}

#[derive(Default)]
pub(super) enum Catalogue {
    #[default]
    Unknown,
    Fetching(Job<Modes, Error>),
    Fetched(Modes),
    Unavailable,
    Retiring(Stopping),
}
impl Catalogue {
    pub(super) fn modes(&self) -> &[String] {
        match self {
            Self::Fetched(modes) => &modes.0,
            Self::Unknown | Self::Fetching(_) | Self::Unavailable | Self::Retiring(_) => &[],
        }
    }
    pub(super) fn accepts(&self, mode: &str) -> bool {
        mode.is_empty() || self.modes().iter().any(|candidate| candidate == mode)
    }
    pub(super) fn selection(&self, mode: &str) -> Option<Options> {
        self.modes()
            .iter()
            .find(|candidate| *candidate == mode)
            .map(|mode| Options {
                mode1: mode.clone(),
                is_delete_original_after_encode: false,
            })
    }
    pub(super) fn reset(&mut self) {
        *self = match std::mem::take(self) {
            Self::Fetching(job) => Self::Retiring(job.cancel()),
            Self::Retiring(job) => Self::Retiring(job),
            Self::Unknown | Self::Fetched(_) | Self::Unavailable => Self::Unknown,
        };
    }
    pub(super) fn poll(&mut self, network: &Network, connection: Option<&Connection>) -> bool {
        let mut changed = false;
        *self = match std::mem::take(self) {
            Self::Fetching(job) => match job.poll() {
                Progress::Pending(job) => Self::Fetching(job),
                Progress::Complete(result) => {
                    changed = true;
                    match result {
                        Ok(modes) => Self::Fetched(modes),
                        Err(error) => {
                            tracing::warn!(
                                operation = "fetch reservation encode modes",
                                error = &error as &dyn std::error::Error,
                                "EPGStation encode choices are unavailable"
                            );
                            Self::Unavailable
                        }
                    }
                }
            },
            Self::Retiring(job) => match job.poll() {
                Progress::Pending(job) => Self::Retiring(job),
                Progress::Complete(()) => Self::Unknown,
            },
            state @ (Self::Unknown | Self::Fetched(_) | Self::Unavailable) => state,
        };
        if let (Self::Unknown, Some(connection)) = (&self, connection) {
            let connection = connection.clone();
            *self = Self::Fetching(network.job(async move {
                let result =
                    async { Modes::validate(http::get::<Config>(&connection, "config").await?) }
                        .await;
                result.map_err(FetchError::Parse)
            }));
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_keep_server_order_and_validate_the_entire_list_before_deduplication() {
        let parse = |bytes| Modes::validate(crate::json::from_slice(bytes).unwrap());
        assert_eq!(parse(br#"{"encode":["B","A","B"]}"#).unwrap().0, ["B", "A"]);
        assert!(parse(br#"{"encode":[]}"#).unwrap().0.is_empty());
        assert!(matches!(
            parse(br#"{"encode":["A",""]}"#),
            Err(Error::Invalid)
        ));
        for (count, chars, valid) in [(64, 256, true), (65, 1, false), (1, 257, false)] {
            let config = Config {
                encode: vec!["画".repeat(chars); count],
            };
            assert_eq!(Modes::validate(config).is_ok(), valid);
        }
    }
}
