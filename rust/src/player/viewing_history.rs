//! Publish history snapshots only after the domain mutation is complete.
use super::ffi;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::pin::Pin;

impl ffi::Player {
    pub fn recent_channels(&self) -> *mut crate::channel_model::ffi::ChannelModel {
        self.rust().recent_channel_model.as_ptr().cast_mut()
    }
    fn previous_service(&self) -> Option<u64> {
        let current = self.rust().stream_state.active_service()?;
        self.rust().viewing_history.previous(current, |service| {
            self.rust()
                .catalog
                .channels()
                .iter()
                .any(|channel| channel.id == service)
        })
    }
    pub fn previous_channel_available(&self) -> bool {
        self.previous_service().is_some()
    }
    pub fn previous_channel(mut self: Pin<&mut Self>) -> bool {
        let Some(service) = self.previous_service() else {
            return false;
        };
        let Some(index) = self
            .rust()
            .catalog
            .channels()
            .iter()
            .position(|channel| channel.id == service)
        else {
            return false;
        };
        self.as_mut()
            .select(i32::try_from(index).expect("bounded channel catalog"));
        true
    }
    pub(super) fn publish_recent_channels(mut self: Pin<&mut Self>, server: &str) {
        let catalog = self.rust().catalog.snapshot();
        let favorites = self.rust().viewing_history.favorites().to_vec();
        let recent = self.rust().viewing_history.recent().to_vec();
        self.as_mut()
            .rust_mut()
            .recent_channel_model
            .pin_mut()
            .replace_recent(catalog, server, &favorites, &recent);
    }
    pub fn viewing_history_ready(&self) -> bool {
        self.rust().viewing_history.ready()
    }
    pub fn viewing_history_error(&self) -> QString {
        QString::from(self.rust().viewing_history.error())
    }
    pub fn toggle_favorite(mut self: Pin<&mut Self>, service_id: QString) -> bool {
        let Ok(service) = service_id.to_string().parse::<u64>() else {
            return false;
        };
        if !self
            .rust()
            .catalog
            .channels()
            .iter()
            .any(|channel| channel.id == service)
        {
            return false;
        }
        if !self
            .as_mut()
            .rust_mut()
            .viewing_history
            .toggle_favorite(service)
        {
            return false;
        }
        self.publish_viewing_history();
        true
    }
    pub fn move_favorite(mut self: Pin<&mut Self>, service_id: QString, position: i32) -> bool {
        let (Ok(service), Ok(position)) = (
            service_id.to_string().parse::<u64>(),
            usize::try_from(position),
        ) else {
            return false;
        };
        if !self
            .as_mut()
            .rust_mut()
            .viewing_history
            .move_favorite(service, position)
        {
            return false;
        }
        self.publish_viewing_history();
        true
    }
    pub(super) fn publish_viewing_history(mut self: Pin<&mut Self>) {
        let server = self.server().to_string();
        self.as_mut().publish_catalog(&server);
    }
    pub(super) fn poll_viewing_history(mut self: Pin<&mut Self>) {
        if self.as_mut().rust_mut().viewing_history.poll() {
            self.publish_viewing_history();
        }
    }
}
