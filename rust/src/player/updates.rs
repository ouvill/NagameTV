//! Read-only Qt projection of update checking and its persisted schedule.
use super::ffi::{Player, UpdateStatus};
use crate::updates::{Outcome, Status};
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QUrl};
use std::pin::Pin;

impl Player {
    pub fn auto_update_check(&self) -> bool {
        self.rust().updates.automatic_enabled()
    }
    pub fn automatic_updates_allowed(&self) -> bool {
        self.rust().updates.automatic_allowed()
    }
    pub fn update_last_checked(&self) -> f64 {
        self.rust()
            .updates
            .last_success()
            .map(|seconds| seconds as f64)
            .unwrap_or(-1.0)
    }
    pub fn update_history_error(&self) -> QString {
        self.rust()
            .updates
            .history_error()
            .map(|error| QString::from(error.to_string()))
            .unwrap_or_default()
    }
    pub fn configure_auto_update_check(mut self: Pin<&mut Self>, enabled: bool) {
        if self
            .as_mut()
            .rust_mut()
            .updates
            .configure_automatic(enabled)
        {
            self.as_mut()
                .rust_mut()
                .preferences
                .change(crate::settings::Change::AutoUpdateCheck(enabled));
            self.as_mut().updates_changed();
            self.save_settings();
        }
    }
    pub fn update_check_status(&self) -> UpdateStatus {
        match self.rust().updates.status() {
            Status::Idle => UpdateStatus::UpdateIdle,
            Status::Checking => UpdateStatus::UpdateChecking,
            Status::Checked(Outcome::Available(_)) => UpdateStatus::UpdateAvailable,
            Status::Checked(Outcome::UpToDate) => UpdateStatus::UpdateCurrent,
            Status::Checked(Outcome::NoRelease) => UpdateStatus::UpdateNoRelease,
            Status::Failed(_) => UpdateStatus::UpdateFailed,
        }
    }

    pub fn update_version(&self) -> QString {
        match self.rust().updates.status() {
            Status::Checked(Outcome::Available(release)) => {
                QString::from(release.version().to_string())
            }
            Status::Idle
            | Status::Checking
            | Status::Checked(Outcome::UpToDate | Outcome::NoRelease)
            | Status::Failed(_) => QString::default(),
        }
    }

    pub fn update_error(&self) -> QString {
        match self.rust().updates.status() {
            Status::Failed(error) => QString::from(error.to_string()),
            Status::Idle | Status::Checking | Status::Checked(_) => self
                .rust()
                .updates
                .open_error()
                .map(|error| QString::from(error.to_string()))
                .unwrap_or_default(),
        }
    }

    pub fn check_updates(mut self: Pin<&mut Self>) {
        let state = self.as_mut().rust_mut();
        let state = state.get_mut();
        if state.updates.check(state.network.as_ref()) {
            self.updates_changed();
        }
    }

    pub fn open_update_release(mut self: Pin<&mut Self>) -> bool {
        let opened = self.as_mut().rust_mut().updates.open_release(|url| {
            crate::qt::ffi::open_external_url(&QUrl::from(&QString::from(url)))
        });
        self.updates_changed();
        opened
    }

    pub(super) fn poll_updates(mut self: Pin<&mut Self>) {
        let state = self.as_mut().rust_mut();
        let state = state.get_mut();
        if state
            .updates
            .tick(state.network.as_ref(), std::time::SystemTime::now())
        {
            self.updates_changed();
        }
    }
}
