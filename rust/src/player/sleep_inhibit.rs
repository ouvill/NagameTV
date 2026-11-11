//! Read-only QObject projection of the Rust-owned viewing inhibition policy.
use super::ffi;
use crate::{settings, sleep_inhibit::Status};
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::{pin::Pin, time::Instant};

impl ffi::Player {
    pub fn sleep_inhibit_enabled(&self) -> bool {
        self.rust().preferences.preferences().sleep_inhibit_enabled
    }
    pub fn sleep_inhibit_state(&self) -> QString {
        QString::from(self.rust().sleep_inhibit.status().as_str())
    }
    pub fn sleep_inhibit_error(&self) -> QString {
        let source = match self.rust().sleep_inhibit.status() {
            Status::Unavailable => "This desktop does not allow apps to keep the screen on.",
            Status::Failed => "Could not keep the screen on while watching.",
            Status::Disabled | Status::Idle | Status::Acquiring | Status::Inhibited => {
                return QString::default();
            }
        };
        crate::qt::text::Text::source(source).render()
    }
    pub fn configure_sleep_inhibit(mut self: Pin<&mut Self>, enabled: bool) {
        let before = (
            self.sleep_inhibit_enabled(),
            self.rust().sleep_inhibit.status(),
        );
        if before.0 != enabled {
            self.as_mut().rust_mut().sleep_inhibit.reset_environment();
        }
        self.as_mut()
            .rust_mut()
            .preferences
            .change(settings::Change::SleepInhibit(enabled));
        synchronize(&mut self.as_mut().rust_mut());
        if before
            != (
                self.sleep_inhibit_enabled(),
                self.rust().sleep_inhibit.status(),
            )
        {
            self.as_mut().sleep_inhibit_changed();
        }
        self.save_settings();
    }
    pub(super) fn poll_sleep_inhibit(mut self: Pin<&mut Self>) {
        let before = self.rust().sleep_inhibit.status();
        synchronize(&mut self.as_mut().rust_mut());
        if before != self.rust().sleep_inhibit.status() {
            self.sleep_inhibit_changed();
        }
    }
    pub(super) fn stop_sleep_inhibit(mut self: Pin<&mut Self>) {
        let before = self.rust().sleep_inhibit.status();
        let enabled = self.sleep_inhibit_enabled();
        self.as_mut().rust_mut().sleep_inhibit.stop(enabled);
        if before != self.rust().sleep_inhibit.status() {
            self.sleep_inhibit_changed();
        }
    }
}

pub(super) fn synchronize(player: &mut super::PlayerRust) {
    player.sleep_inhibit.update(
        player.preferences.preferences().sleep_inhibit_enabled,
        player
            .stream_state
            .sleep_inhibit_viewing(player.recording_reconnecting),
        Instant::now(),
    );
}
