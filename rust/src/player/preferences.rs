//! Explicit preference commits shared by UI actions and orderly shutdown.
use super::ffi;
use crate::settings::SaveStatus;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::pin::Pin;

impl ffi::Player {
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
