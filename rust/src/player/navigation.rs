//! Project the Rust history; legacy QML contributes only its visible screen at opt-in.
use super::ffi;
use crate::screen_navigation::{Availability, Projection, Screen};
use cxx_qt::CxxQtType;
use std::pin::Pin;

impl ffi::Player {
    pub fn screen_navigation(&self) -> bool {
        self.rust().navigation.enabled()
    }
    pub fn screen(&self) -> i32 {
        self.rust().navigation.projection().screen as i32
    }
    pub fn back_screen(&self) -> i32 {
        self.rust()
            .navigation
            .projection()
            .back
            .map_or(-1, |screen| screen as i32)
    }
    pub fn watching_available(&self) -> bool {
        self.rust().viewing.available()
    }
    fn screen_availability(&self) -> Availability {
        Availability {
            watching: self.watching_available(),
            guide: self.rust().epg_enabled,
        }
    }
    fn publish_navigation(mut self: Pin<&mut Self>, before: Projection) {
        if self.screen_navigation() {
            let guide = self.rust().navigation.projection().screen == Screen::Guide;
            self.as_mut().apply_guide_visibility(guide);
        }
        if before != self.rust().navigation.projection() {
            self.screen_changed();
        }
    }
    pub(super) fn reconcile_navigation(mut self: Pin<&mut Self>) {
        let before = self.rust().navigation.projection();
        let available = self.screen_availability();
        self.as_mut().rust_mut().navigation.reconcile(available);
        self.publish_navigation(before);
    }
    pub fn configure_screen_navigation(
        mut self: Pin<&mut Self>,
        enabled: bool,
        legacy_screen: i32,
    ) -> bool {
        if enabled && !self.rust().screen_navigation_allowed {
            return false;
        }
        if self.screen_navigation() != enabled {
            let before = self.rust().navigation.projection();
            if enabled {
                let Some(screen) = Screen::from_number(legacy_screen) else {
                    return false;
                };
                let available = self.screen_availability();
                if !self
                    .as_mut()
                    .rust_mut()
                    .navigation
                    .enable(screen, available)
                {
                    return false;
                }
            } else {
                self.as_mut().rust_mut().navigation.disable();
            }
            self.as_mut()
                .rust_mut()
                .preferences
                .change(crate::settings::Change::ScreenNavigationEnabled(enabled));
            self.as_mut().publish_navigation(before);
            self.as_mut().screen_navigation_changed();
        }
        self.save_settings();
        true
    }
    pub fn open_screen(mut self: Pin<&mut Self>, screen: i32) -> bool {
        let Some(screen) = Screen::from_number(screen) else {
            return false;
        };
        let before = self.rust().navigation.projection();
        let available = self.screen_availability();
        let accepted = self.as_mut().rust_mut().navigation.open(screen, available);
        self.publish_navigation(before);
        accepted
    }
    pub fn go_back(mut self: Pin<&mut Self>) -> bool {
        let before = self.rust().navigation.projection();
        let available = self.screen_availability();
        let accepted = self.as_mut().rust_mut().navigation.back(available);
        self.publish_navigation(before);
        accepted
    }
    pub(super) fn change_viewing(
        mut self: Pin<&mut Self>,
        change: impl FnOnce(&mut crate::playback::viewing::Viewing),
        open: bool,
    ) {
        let before = self.rust().navigation.projection();
        let was_available = self.watching_available();
        change(&mut self.as_mut().rust_mut().viewing);
        let available = self.screen_availability();
        if open {
            self.as_mut()
                .rust_mut()
                .navigation
                .open(Screen::Watching, available);
        } else {
            self.as_mut().rust_mut().navigation.reconcile(available);
        }
        self.as_mut().publish_navigation(before);
        if was_available != self.watching_available() {
            self.watching_available_changed();
        }
    }
}
