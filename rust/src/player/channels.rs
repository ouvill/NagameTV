//! Qt adapter for the domain channel catalog and navigation commands.
impl super::ffi::Player {
    pub(super) fn publish_catalog(mut self: std::pin::Pin<&mut Self>, server: &str) {
        use cxx_qt::CxxQtType;
        let snapshot = self.rust().catalog.snapshot();
        self.as_mut()
            .rust_mut()
            .channel_model
            .pin_mut()
            .replace(snapshot, server);
    }

    pub(super) fn replace_catalog(
        mut self: std::pin::Pin<&mut Self>,
        catalog: crate::channels::catalog::Catalog,
        server: &str,
    ) {
        use cxx_qt::CxxQtType;
        let selected = self.selected();
        let viewing = self.viewing_channel();
        let action = self.playback_action();
        self.as_mut().rust_mut().catalog = catalog;
        self.as_mut().publish_catalog(server);
        if selected != self.selected() {
            self.as_mut().selected_changed();
        }
        if viewing != self.viewing_channel() {
            self.as_mut().viewing_channel_changed();
        }
        if action != self.playback_action() {
            self.as_mut().playback_action_changed();
        }
    }

    /// The viewed live input, independent of the saved selection and browser cursor.
    pub fn viewing_channel(&self) -> i32 {
        use super::stream_state::State;
        use cxx_qt::CxxQtType;
        let this = self.rust();
        let service = match &this.stream_state {
            State::Playing(live, _) => live.service(),
            State::Stopped(_)
            | State::Connecting(_)
            | State::Recording(_, _)
            | State::StopFailed(_) => return -1,
        };
        this.catalog
            .channels()
            .iter()
            .position(|channel| channel.id == service)
            .and_then(|index| i32::try_from(index).ok())
            .unwrap_or(-1)
    }

    pub(super) fn refresh_channels_if_due(self: std::pin::Pin<&mut Self>) {
        use cxx_qt::CxxQtType;
        let now = std::time::Instant::now();
        if !self
            .rust()
            .channel_refresh
            .due(now, self.rust().request.is_busy())
        {
            return;
        }
        self.request_channel_refresh(now);
    }

    pub fn refresh_channels(self: std::pin::Pin<&mut Self>, force: bool) {
        use cxx_qt::CxxQtType;
        let now = std::time::Instant::now();
        if self
            .rust()
            .channel_refresh
            .requested_by_user(now, self.rust().request.is_busy(), force)
        {
            self.request_channel_refresh(now);
        }
    }

    fn request_channel_refresh(mut self: std::pin::Pin<&mut Self>, now: std::time::Instant) {
        use cxx_qt::CxxQtType;
        if self.rust().network.is_none() {
            return;
        }
        let Ok(server) = crate::services::ServerUrl::parse(&self.rust().server.to_string()) else {
            return;
        };
        self.as_mut().rust_mut().request.request(server);
        self.as_mut().rust_mut().channel_refresh.requested(now);
    }

    pub fn step_channel(self: std::pin::Pin<&mut Self>, offset: i32) {
        use crate::channels::Step;
        use cxx_qt::CxxQtType;
        use std::time::{SystemTime, UNIX_EPOCH};
        let step = match offset {
            -1 => Step::Previous,
            1 => Step::Next,
            _ => return,
        };
        let this = self.rust();
        let now = if this.epg_enabled {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .ok()
                .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        } else {
            None
        };
        let selected = this.catalog.selected_index();
        let target = this
            .epg
            .adjacent_channel(this.catalog.channels(), selected, step, now)
            .and_then(|index| i32::try_from(index).ok());
        if let Some(index) = target {
            self.select(index);
        }
    }
}
