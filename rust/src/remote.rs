//! Per-process remote listener, with explicit reconfiguration and shutdown ownership.
mod addresses;
pub(crate) mod settings;
use settings::{Preferences, Settings};
use viewer_remote::{Bound, Session, Stopping, model};

enum AfterStop {
    Disabled,
    Restart,
    Failed(String),
}
enum Phase {
    Disabled,
    Ready,
    Running(Box<Session>),
    Stopping { task: Stopping, next: AfterStop },
    Failed(String),
}

pub struct Control {
    preferences: Preferences,
    phase: Phase,
    save_error: String,
}

impl Control {
    pub fn load(transient: bool) -> Self {
        let loaded = if transient {
            Ok(Preferences::transient(Settings::default()))
        } else {
            crate::settings::settings_path()
                .map_err(|e| e.to_string())
                .and_then(|path| Preferences::open(path.with_file_name("remote-control.toml")))
        };
        let loaded = loaded.and_then(|preferences| {
            settings::Overrides::from_env().map(|values| preferences.with_overrides(values))
        });
        match loaded {
            Ok(preferences) => Self::new(preferences),
            Err(error) => {
                tracing::error!("Remote configuration failed: {error}");
                Self {
                    preferences: Preferences::transient(Settings::default()),
                    phase: Phase::Failed(error),
                    save_error: String::new(),
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
            save_error: String::new(),
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
    pub fn error(&self) -> &str {
        match &self.phase {
            Phase::Failed(error) => error,
            Phase::Disabled | Phase::Ready | Phase::Running(_) | Phase::Stopping { .. } => "",
        }
    }
    pub fn save_error(&self) -> &str {
        &self.save_error
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
            .err()
            .unwrap_or_default();
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
            AfterStop::Failed(error) => Phase::Failed(error),
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
                next: AfterStop::Failed("remote API stopped unexpectedly".into()),
            },
            Phase::Stopping { task, next } => match task.poll() {
                viewer_remote::Progress::Pending(task) => Phase::Stopping { task, next },
                viewer_remote::Progress::Complete(Ok(())) => Self::after_stop(next),
                viewer_remote::Progress::Complete(Err(error)) => Phase::Failed(error.to_string()),
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
            let network = network.ok_or_else(|| "network runtime is unavailable".to_owned())?;
            let bound = Bound::bind(viewer_remote::config::Config::new(
                self.settings().endpoint(),
            ))
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::AddrInUse {
                    format!("{}: port is already in use", self.settings().endpoint())
                } else {
                    error.to_string()
                }
            })?;
            network
                .start_remote(bound, state, channels)
                .map_err(|error| error.to_string())
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
                tracing::error!("Remote API could not start: {error}");
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
