//! Per-process remote listener, with explicit reconfiguration and shutdown ownership.
mod addresses;
pub(crate) mod settings;
use settings::{Preferences, Settings};
use viewer_remote::{Bound, Session, Stopping, model};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Settings(#[from] settings::Error),
    #[error("{0}")]
    Directory(#[from] crate::settings::Error),
    #[error("network runtime is unavailable")]
    NetworkUnavailable,
    #[error("{address}: port is already in use")]
    AddressInUse {
        address: std::net::SocketAddr,
        source: std::io::Error,
    },
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Session(#[from] viewer_remote::ServerError),
    #[error("remote API stopped unexpectedly")]
    Stopped,
}

enum AfterStop {
    Disabled,
    Restart,
    Failed(Error),
}
enum Phase {
    Disabled,
    Ready,
    Running(Box<Session>),
    Stopping { task: Stopping, next: AfterStop },
    Failed(Error),
}

pub struct Control {
    preferences: Preferences,
    phase: Phase,
    save_error: Option<settings::Error>,
}

impl Control {
    pub fn load(transient: bool) -> Self {
        let loaded = if transient {
            Ok(Preferences::transient(Settings::default()))
        } else {
            crate::settings::settings_path()
                .map_err(Error::from)
                .and_then(|path| {
                    Preferences::open(path.with_file_name("remote-control.toml"))
                        .map_err(Error::from)
                })
        };
        let loaded = loaded.and_then(|preferences| {
            settings::Overrides::from_env()
                .map(|values| preferences.with_overrides(values))
                .map_err(Error::from)
        });
        match loaded {
            Ok(preferences) => Self::new(preferences),
            Err(error) => {
                tracing::error!("Remote configuration failed: {error}");
                Self {
                    preferences: Preferences::transient(Settings::default()),
                    phase: Phase::Failed(error),
                    save_error: None,
                }
            }
        }
    }
    pub fn new(preferences: Preferences) -> Self {
        let phase = if preferences.current().enabled() {
            Phase::Ready
        } else {
            Phase::Disabled
        };
        Self {
            preferences,
            phase,
            save_error: None,
        }
    }
    pub fn settings(&self) -> Settings {
        self.preferences.current()
    }
    pub fn session_only(&self) -> bool {
        self.preferences.session_only()
    }
    pub fn status(&self) -> &'static str {
        match self.phase {
            Phase::Disabled => "disabled",
            Phase::Ready => "starting",
            Phase::Running(_) => "listening",
            Phase::Stopping { .. } => "stopping",
            Phase::Failed(_) => "failed",
        }
    }
    pub fn error(&self) -> Option<&Error> {
        match &self.phase {
            Phase::Failed(error) => Some(error),
            Phase::Disabled | Phase::Ready | Phase::Running(_) | Phase::Stopping { .. } => None,
        }
    }
    pub fn save_error(&self) -> Option<&settings::Error> {
        self.save_error.as_ref()
    }
    pub fn session(&self) -> Option<&Session> {
        match &self.phase {
            Phase::Running(session) => Some(session),
            Phase::Disabled | Phase::Ready | Phase::Stopping { .. } | Phase::Failed(_) => None,
        }
    }
    pub fn session_mut(&mut self) -> Option<&mut Session> {
        match &mut self.phase {
            Phase::Running(session) => Some(session),
            Phase::Disabled | Phase::Ready | Phase::Stopping { .. } | Phase::Failed(_) => None,
        }
    }
    pub fn endpoints(&self) -> String {
        self.session()
            .map(|session| addresses::for_listener(session.address()).join("\n"))
            .unwrap_or_default()
    }
    pub fn configure(&mut self, settings: Settings) {
        let unchanged = settings == self.settings();
        self.save_error = self
            .preferences
            .configure(settings)
            .inspect_err(|error| {
                tracing::error!(%error, "Remote settings save failed");
            })
            .err();
        if unchanged && matches!(self.phase, Phase::Running(_) | Phase::Disabled) {
            return;
        }
        let next = if settings.enabled() {
            AfterStop::Restart
        } else {
            AfterStop::Disabled
        };
        self.phase = match std::mem::replace(&mut self.phase, Phase::Disabled) {
            Phase::Running(session) => Phase::Stopping {
                task: (*session).stop(),
                next,
            },
            Phase::Stopping { task, .. } => Phase::Stopping { task, next },
            Phase::Disabled | Phase::Ready | Phase::Failed(_) => Self::after_stop(next),
        };
    }
    fn after_stop(next: AfterStop) -> Phase {
        match next {
            AfterStop::Disabled => Phase::Disabled,
            AfterStop::Restart => Phase::Ready,
            AfterStop::Failed(error) => {
                tracing::error!(%error, "Remote API stopped unexpectedly");
                Phase::Failed(error)
            }
        }
    }
    pub fn stop(&mut self) {
        self.phase = match std::mem::replace(&mut self.phase, Phase::Disabled) {
            Phase::Running(session) => Phase::Stopping {
                task: (*session).stop(),
                next: AfterStop::Disabled,
            },
            Phase::Stopping { task, .. } => Phase::Stopping {
                task,
                next: AfterStop::Disabled,
            },
            Phase::Disabled | Phase::Ready | Phase::Failed(_) => Phase::Disabled,
        };
    }
    /// Returns true only when the UI-visible lifecycle state changed.
    pub fn poll(&mut self) -> bool {
        let before = self.status();
        self.phase = match std::mem::replace(&mut self.phase, Phase::Disabled) {
            Phase::Running(session) if session.is_finished() => Phase::Stopping {
                task: (*session).stop(),
                next: AfterStop::Failed(Error::Stopped),
            },
            Phase::Stopping { task, next } => match task.poll() {
                viewer_remote::Progress::Pending(task) => Phase::Stopping { task, next },
                viewer_remote::Progress::Complete(Ok(())) => Self::after_stop(next),
                viewer_remote::Progress::Complete(Err(error)) => {
                    tracing::error!(
                        error = &error as &dyn std::error::Error,
                        "Remote API shutdown failed"
                    );
                    Phase::Failed(error.into())
                }
            },
            phase @ (Phase::Disabled | Phase::Ready | Phase::Running(_) | Phase::Failed(_)) => {
                phase
            }
        };
        self.status() != before
    }
    pub fn needs_start(&self) -> bool {
        matches!(self.phase, Phase::Ready)
    }
    pub fn start(
        &mut self,
        network: Option<&crate::services::Network>,
        state: model::State,
        channels: Vec<model::Channel>,
    ) {
        if !self.needs_start() {
            return;
        }
        let result = (|| {
            let network = network.ok_or(Error::NetworkUnavailable)?;
            let bound = Bound::bind(viewer_remote::config::Config::new(
                self.settings().endpoint(),
            ))
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::AddrInUse {
                    Error::AddressInUse {
                        address: self.settings().endpoint(),
                        source: error,
                    }
                } else {
                    Error::Io(error)
                }
            })?;
            network
                .start_remote(bound, state, channels)
                .map_err(Error::from)
        })();
        self.phase = match result {
            Ok(session) => {
                tracing::info!(
                    "Remote API listening on {} (gRPC and gRPC-Web)",
                    session.address()
                );
                Phase::Running(Box::new(session))
            }
            Err(error) => {
                tracing::error!(
                    error = &error as &dyn std::error::Error,
                    "Remote API could not start"
                );
                Phase::Failed(error)
            }
        };
    }
}

pub fn band(band: crate::channels::Band) -> model::Band {
    match band {
        crate::channels::Band::Terrestrial => model::Band::Terrestrial,
        crate::channels::Band::Bs => model::Band::Bs,
        crate::channels::Band::Cs => model::Band::Cs,
        crate::channels::Band::Sky => model::Band::Sky,
        crate::channels::Band::Other => model::Band::Other,
    }
}
