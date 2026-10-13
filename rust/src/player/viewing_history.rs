//! Publish history snapshots only after the domain mutation is complete.
use super::ffi;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::pin::Pin;

impl ffi::Player {
    pub fn viewing_history_ready(&self) -> bool { self.rust().viewing_history.ready() }
    pub fn viewing_history_error(&self) -> QString { QString::from(self.rust().viewing_history.error()) }
    pub fn toggle_favorite(mut self: Pin<&mut Self>, service_id: QString) -> bool {
        let Ok(service) = service_id.to_string().parse::<u64>() else { return false };
        if !self.rust().catalog.channels().iter().any(|channel| channel.id == service) { return false; }
        if !self.as_mut().rust_mut().viewing_history.toggle_favorite(service) { return false; }
        self.publish_viewing_history();
        true
    }
    pub fn move_favorite(mut self: Pin<&mut Self>, service_id: QString, position: i32) -> bool {
        let (Ok(service), Ok(position)) = (service_id.to_string().parse::<u64>(), usize::try_from(position)) else { return false };
        if !self.as_mut().rust_mut().viewing_history.move_favorite(service, position) { return false; }
        self.publish_viewing_history();
        true
    }
    pub(super) fn publish_viewing_history(mut self: Pin<&mut Self>) {
        let server = self.server().to_string();
        self.as_mut().publish_catalog(&server);
        self.viewing_history_changed();
    }
    pub(super) fn poll_viewing_history(mut self: Pin<&mut Self>) {
        if self.as_mut().rust_mut().viewing_history.poll() {
            self.publish_viewing_history();
        }
    }
}
