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
        self.as_mut().publish_recording_history();
        let server = self.server().to_string();
        self.as_mut().publish_catalog(&server);
    }
    pub(super) fn poll_viewing_history(mut self: Pin<&mut Self>) {
        if self.as_mut().rust_mut().viewing_history.poll() {
            self.publish_viewing_history();
        }
    }
}

impl ffi::Player {
    pub fn continue_recordings(
        &self,
    ) -> *mut crate::continue_watching_model::ffi::ContinueWatchingModel {
        self.rust().continue_watching_model.as_ptr().cast_mut()
    }
    pub fn resume_recording(mut self: Pin<&mut Self>, recording_key: QString) -> bool {
        let key = recording_key.to_string();
        let identity = self
            .rust()
            .viewing_history
            .continue_watching()
            .into_iter()
            .find(|entry| entry.key == key)
            .map(|entry| entry.identity);
        let Some(identity) = identity else {
            return false;
        };
        match self.rust().recording_library.resume_request(identity) {
            Ok(request) => {
                self.as_mut().rust_mut().autoplay_pending = false;
                self.as_mut()
                    .set_file_error(super::error_text::Text::default());
                self.begin_recording(request);
                true
            }
            Err(error) => {
                use super::error_text::{PresentError, Text};
                tracing::error!(
                    error = &error as &dyn std::error::Error,
                    "Prepare recording resume failed"
                );
                self.set_file_error(Text::message(
                    "Could not open the video file: %1",
                    [error.present()],
                ));
                false
            }
        }
    }
    pub(super) fn publish_recording_history(mut self: Pin<&mut Self>) {
        let continued = self
            .rust()
            .viewing_history
            .continue_watching()
            .into_iter()
            .map(|entry| {
                let thumbnail = match &entry.identity.source {
                    crate::viewing_history::recordings::Source::Epgstation { server, .. } => self
                        .rust()
                        .recording_library
                        .thumbnail(server, entry.identity.thumbnail_id)
                        .to_owned(),
                    crate::viewing_history::recordings::Source::Local { .. }
                    | crate::viewing_history::recordings::Source::Http { .. } => String::new(),
                };
                (entry, thumbnail)
            })
            .collect();
        let rows = self.rust().recording_library.rows();
        let server = self.rust().recording_library.server();
        let thumbnails = rows
            .iter()
            .filter_map(|row| {
                let thumbnail = self
                    .rust()
                    .recording_library
                    .thumbnail(server?, row.thumbnail_id);
                (!thumbnail.is_empty()).then(|| (row.id, thumbnail.to_owned()))
            })
            .collect();
        let mut progress = std::collections::HashMap::new();
        for entry in self.rust().viewing_history.recordings() {
            if let crate::viewing_history::recordings::Source::Epgstation {
                server: entry_server,
                recorded_id,
                ..
            } = &entry.identity.source
                && server == Some(entry_server.as_str())
            {
                progress.entry(*recorded_id).or_insert(entry.position);
            }
        }
        self.as_mut()
            .rust_mut()
            .recording_model
            .pin_mut()
            .replace_with_progress(rows, progress, thumbnails);
        self.as_mut()
            .rust_mut()
            .continue_watching_model
            .pin_mut()
            .replace(continued);
    }
    pub(super) fn poll_recording_history(mut self: Pin<&mut Self>) {
        if !self.media_active() || !self.recording() {
            return;
        }
        if self.seekable() && !self.seeking() {
            let duration = self.duration_ms();
            let target = {
                let this = self.as_mut().rust_mut();
                let this = this.get_mut();
                this.recording_history_session
                    .as_mut()
                    .and_then(|session| session.resume(&this.viewing_history, duration))
            };
            if let Some(target) = target
                && !self.as_mut().seek_to(target)
                && let Some(session) = &mut self.as_mut().rust_mut().recording_history_session
            {
                session.seek_rejected();
            }
        }
        let ended = self.ended();
        self.save_recording_history(ended);
    }
    pub(super) fn save_recording_history(mut self: Pin<&mut Self>, force: bool) {
        use crate::viewing_history::recordings::Position;
        let sample = if self.media_active() && self.recording() {
            let position = if self.ended() {
                self.duration_ms()
            } else {
                self.position_ms()
            };
            Position::new(position, self.duration_ms())
        } else {
            None
        };
        let seeking = self.seeking();
        let changed = {
            let this = self.as_mut().rust_mut();
            let this = this.get_mut();
            match &mut this.recording_history_session {
                Some(session) => {
                    session.sample(sample, seeking);
                    session.save(&mut this.viewing_history, std::time::Instant::now(), force)
                }
                None => false,
            }
        };
        if changed {
            self.publish_recording_history();
        }
    }
}

impl ffi::Player {
    pub(super) fn poll_recording_thumbnails(mut self: Pin<&mut Self>) {
        let wanted = self
            .rust()
            .viewing_history
            .recordings()
            .iter()
            .filter(|entry| entry.position.resumable())
            .take(crate::viewing_history::recordings::MAX_CONTINUE)
            .filter_map(|entry| match &entry.identity.source {
                crate::viewing_history::recordings::Source::Epgstation { server, .. } => {
                    Some(crate::epgstation::thumbnails::Key {
                        server: server.clone(),
                        id: entry.identity.thumbnail_id?,
                    })
                }
                crate::viewing_history::recordings::Source::Local { .. }
                | crate::viewing_history::recordings::Source::Http { .. } => None,
            })
            .collect();
        let changed = {
            let this = self.as_mut().rust_mut();
            let this = this.get_mut();
            this.network
                .as_ref()
                .is_some_and(|network| this.recording_library.poll_thumbnails(network, wanted))
        };
        if changed {
            self.publish_recording_history();
        }
    }
}
