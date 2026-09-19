//! Explicit preference commits shared by UI actions and orderly shutdown.
use super::ffi;
use crate::settings::SaveStatus;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::pin::Pin;

impl ffi::Player {
    pub fn timeshift_storage(&self) -> QString {
        use crate::playback::input::Retention;
        QString::from(match self.rust().preferences.preferences().timeshift {
            Retention::Off => "off",
            Retention::Memory => "memory",
            Retention::Filesystem => "filesystem",
        })
    }
    pub fn timeshift_limits(&self) -> QString {
        use crate::playback::input::limits;
        let limits = self.rust().preferences.preferences().timeshift_limits;
        QString::from(
            serde_json::json!({
                "memory_mib": limits.memory_mib(), "filesystem_mib": limits.filesystem_mib(),
                "minutes": limits.minutes(), "min_mib": limits::MIN_CAPACITY_MIB,
                "max_mib": limits::MAX_CAPACITY_MIB, "min_minutes": limits::MIN_RETENTION_MINUTES,
                "max_minutes": limits::MAX_RETENTION_MINUTES,
            })
            .to_string(),
        )
    }
    pub fn configure_timeshift(self: Pin<&mut Self>, storage: QString) -> bool {
        let limits = self.rust().preferences.preferences().timeshift_limits;
        self.configure_timeshift_options(
            storage,
            limits.memory_mib() as i32,
            limits.filesystem_mib() as i32,
            limits.minutes() as i32,
        )
    }
    pub fn configure_timeshift_options(
        mut self: Pin<&mut Self>,
        storage: QString,
        memory_mib: i32,
        filesystem_mib: i32,
        minutes: i32,
    ) -> bool {
        use crate::playback::input::{Limits, Policy, Retention};
        let storage = match storage.to_string().as_str() {
            "off" => Retention::Off,
            "memory" => Retention::Memory,
            "filesystem" => Retention::Filesystem,
            _ => return false,
        };
        let Some(limits) = Limits::new(memory_mib as u32, filesystem_mib as u32, minutes as u32)
        else {
            return false;
        };
        let policy = Policy::new(storage, limits);
        if policy == self.rust().preferences.preferences().timeshift_policy() {
            self.as_mut().save_settings();
            return true;
        }
        if let Err(error) = self.as_mut().rust_mut().media.configure_timeshift(policy) {
            self.as_mut()
                .set_transport_message(super::transport::Message::Failure(QString::from(
                    error.to_string(),
                )));
            return false;
        }
        {
            let mut this = self.as_mut().rust_mut();
            this.preferences
                .change(crate::settings::Change::Timeshift(storage));
            this.preferences
                .change(crate::settings::Change::TimeshiftLimits(limits));
        }
        self.as_mut()
            .change_stream_state(|state| state.reconfigured_live(policy));
        self.as_mut().timeshift_storage_changed();
        self.as_mut().timeshift_limits_changed();
        self.as_mut().save_settings();
        self.as_mut()
            .set_transport_message(super::transport::Message::None);
        true
    }

    pub fn autoplay(&self) -> bool {
        self.rust().preferences.preferences().autoplay
    }

    pub fn configure_autoplay(mut self: Pin<&mut Self>, enabled: bool) {
        if self.autoplay() != enabled {
            self.as_mut()
                .rust_mut()
                .preferences
                .change(crate::settings::Change::Autoplay(enabled));
            self.as_mut().autoplay_changed();
        }
        self.save_settings();
    }

    pub fn save_settings(mut self: Pin<&mut Self>) {
        let result = self.as_mut().rust_mut().preferences.flush();
        match result {
            // Transient sessions include failed loads. Keep that diagnostic;
            // declining to write is not a successful repair of a corrupt file.
            Ok(SaveStatus::Transient) => {}
            Ok(SaveStatus::Saved | SaveStatus::Unchanged) => {
                self.set_settings_error(QString::default());
            }
            Err(error) => {
                tracing::error!("Settings save failed: {error}");
                self.set_settings_error(QString::from(error.to_string()));
            }
        }
    }
}
