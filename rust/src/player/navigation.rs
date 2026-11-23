//! Project the authoritative Rust screen history to QML.
use super::ffi;
use crate::screen_navigation::{Availability, Projection, Screen};
use cxx_qt::CxxQtType;
use std::pin::Pin;

impl ffi::Player {
    fn navigation_projection(&self) -> Projection {
        self.rust()
            .navigation
            .projection(self.screen_availability())
    }
    pub fn screen(&self) -> i32 {
        self.navigation_projection().screen as i32
    }
    pub fn back_screen(&self) -> i32 {
        self.navigation_projection()
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
        let after = self.navigation_projection();
        let guide = after.screen == Screen::Guide;
        self.as_mut().apply_guide_visibility(guide);
        // The QML Loader can request its first day synchronously on this signal.
        // Guide state and data must already be ready to accept that request.
        if before != after {
            self.screen_changed();
        }
    }
    pub(super) fn reconcile_navigation(mut self: Pin<&mut Self>) {
        let before = self.navigation_projection();
        let available = self.screen_availability();
        self.as_mut().rust_mut().navigation.reconcile(available);
        self.publish_navigation(before);
    }
    pub fn open_screen(mut self: Pin<&mut Self>, screen: i32) -> bool {
        let Some(screen) = Screen::from_number(screen) else {
            return false;
        };
        let before = self.navigation_projection();
        let available = self.screen_availability();
        let accepted = self.as_mut().rust_mut().navigation.open(screen, available);
        self.publish_navigation(before);
        accepted
    }
    pub fn go_back(mut self: Pin<&mut Self>) -> bool {
        let before = self.navigation_projection();
        let available = self.screen_availability();
        let accepted = self.as_mut().rust_mut().navigation.back(available);
        self.publish_navigation(before);
        accepted
    }
    /// Shutdown leaves for Home and closes the session in one notification: a separate
    /// move would briefly offer the closing session as the back target.
    pub(super) fn close_viewing_at_home(mut self: Pin<&mut Self>) {
        let before = self.navigation_projection();
        let was_available = self.watching_available();
        self.as_mut().rust_mut().viewing.close();
        let available = self.screen_availability();
        self.as_mut()
            .rust_mut()
            .navigation
            .open(Screen::Home, available);
        self.as_mut().publish_navigation(before);
        if was_available != self.watching_available() {
            self.watching_available_changed();
        }
    }
    pub(super) fn change_viewing(
        mut self: Pin<&mut Self>,
        change: impl FnOnce(&mut crate::playback::viewing::Viewing),
        open: bool,
    ) {
        let before = self.navigation_projection();
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
