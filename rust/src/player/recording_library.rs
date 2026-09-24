//! Qt commands/notifications for the independently owned recording catalogue.
use super::ffi;
use crate::services::{FetchError, NetworkError};
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::pin::Pin;

impl ffi::Player {
    pub fn recording_files(&self) -> *mut crate::video_file_model::ffi::VideoFileModel {
        self.rust().video_file_model.as_ptr().cast_mut()
    }
    pub fn choose_epgstation(mut self: Pin<&mut Self>, id: QString) -> bool {
        let files = id
            .to_string()
            .parse::<u64>()
            .ok()
            .map(|id| self.rust().recording_library.files(id))
            .unwrap_or_default();
        let available = !files.is_empty();
        self.as_mut()
            .rust_mut()
            .video_file_model
            .pin_mut()
            .replace(files);
        available
    }
    pub fn play_epgstation_file(mut self: Pin<&mut Self>, id: QString, video: QString) -> bool {
        let request = id
            .to_string()
            .parse::<u64>()
            .ok()
            .zip(video.to_string().parse::<u64>().ok())
            .and_then(|(id, video)| {
                self.rust()
                    .recording_library
                    .playback_request(id, Some(video))
            });
        match request {
            Some(request) => {
                self.as_mut().rust_mut().autoplay_pending = false;
                self.as_mut().set_file_error(QString::default());
                self.begin_recording(request);
                true
            }
            None => false,
        }
    }

    pub fn recordings(&self) -> *mut crate::recording_model::ffi::RecordingModel {
        self.rust().recording_model.as_ptr().cast_mut()
    }
    pub fn epgstation_server(&self) -> QString {
        QString::from(
            self.rust()
                .preferences
                .preferences()
                .epgstation_server
                .as_str(),
        )
    }
    pub fn epgstation_busy(&self) -> bool {
        self.rust().recording_library.busy()
    }
    pub fn epgstation_loaded(&self) -> bool {
        self.rust().recording_library.loaded()
    }
    pub fn epgstation_previous(&self) -> bool {
        self.rust().recording_library.has_previous()
    }
    pub fn epgstation_next(&self) -> bool {
        self.rust().recording_library.has_next()
    }
    pub fn epgstation_total(&self) -> QString {
        QString::from(self.rust().recording_library.total().to_string())
    }
    pub fn epgstation_page(&self) -> QString {
        QString::from(
            (self.rust().recording_library.offset() / crate::epgstation::PAGE_SIZE + 1).to_string(),
        )
    }
    pub fn epgstation_error(&self) -> QString {
        if !self.rust().epgstation_input_error.is_empty() {
            return self.rust().epgstation_input_error.clone();
        }
        match self.rust().recording_library.error() {
            None => QString::default(),
            Some(FetchError::Network(NetworkError::Http(error)))
                if error
                    .status()
                    .is_some_and(|status| matches!(status.as_u16(), 401 | 403)) =>
            {
                super::status::tr(
                    "Authentication failed. Check your username and password, then sign in again.",
                )
            }
            Some(error) => QString::from(error.to_string()),
        }
    }
    pub fn browse_epgstation(mut self: Pin<&mut Self>, server: QString, keyword: QString) -> bool {
        self.as_mut()
            .request_recording_library(server, keyword, crate::epgstation::Login::Current)
    }
    pub fn login_epgstation(
        mut self: Pin<&mut Self>,
        server: QString,
        name: QString,
        password: QString,
    ) -> bool {
        let login = match crate::epgstation::Login::password(name.to_string(), password.to_string())
        {
            Ok(login) => login,
            Err(error) => {
                tracing::error!(
                    error = &error as &dyn std::error::Error,
                    "EPGStation login validation failed"
                );
                self.as_mut().rust_mut().epgstation_input_error = QString::from(error.to_string());
                self.epgstation_changed();
                return false;
            }
        };
        self.request_recording_library(server, QString::default(), login)
    }
    fn request_recording_library(
        mut self: Pin<&mut Self>,
        server: QString,
        keyword: QString,
        login: crate::epgstation::Login,
    ) -> bool {
        let state = self.as_mut().rust_mut().get_mut();
        let result = match &state.network {
            Some(network) => state.recording_library.open(
                network,
                &server.to_string(),
                &keyword.to_string(),
                login,
            ),
            None => {
                tracing::error!("Recording catalogue request failed: network is unavailable");
                state.epgstation_input_error = super::status::tr("Network is unavailable.");
                self.epgstation_changed();
                return false;
            }
        };
        if let Err(error) = &result {
            tracing::error!(
                error = error as &dyn std::error::Error,
                "Recording catalogue request failed"
            );
        }
        let accepted = result.is_ok();
        self.as_mut().rust_mut().epgstation_input_error = result
            .err()
            .map(|error| QString::from(error.to_string()))
            .unwrap_or_default();
        self.publish_recording_library();
        accepted
    }
    pub fn epgstation_change_page(mut self: Pin<&mut Self>, forward: bool) {
        let state = self.as_mut().rust_mut().get_mut();
        if let Some(network) = &state.network {
            if forward {
                state.recording_library.next(network);
            } else {
                state.recording_library.previous(network);
            }
        }
        self.publish_recording_library();
    }
    pub fn cancel_epgstation(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().recording_library.cancel();
        self.epgstation_changed();
    }
    pub fn play_epgstation(mut self: Pin<&mut Self>, id: QString) -> bool {
        let request = id
            .to_string()
            .parse::<u64>()
            .ok()
            .and_then(|id| self.rust().recording_library.playback_request(id, None));
        match request {
            Some(request) => {
                self.as_mut().rust_mut().autoplay_pending = false;
                self.as_mut().set_file_error(QString::default());
                self.begin_recording(request);
                true
            }
            None => false,
        }
    }
    pub(super) fn poll_recording_library(mut self: Pin<&mut Self>) {
        let before = (
            self.epgstation_busy(),
            self.rust().recording_library.revision(),
        );
        let state = self.as_mut().rust_mut().get_mut();
        let verified = state
            .network
            .as_ref()
            .and_then(|network| state.recording_library.poll(network));
        if let Some(server) = verified {
            self.as_mut()
                .rust_mut()
                .preferences
                .confirm_epgstation(&server);
            self.as_mut().save_settings();
        }
        if before
            != (
                self.epgstation_busy(),
                self.rust().recording_library.revision(),
            )
        {
            self.publish_recording_library();
        }
    }
    fn publish_recording_library(mut self: Pin<&mut Self>) {
        let rows = self.rust().recording_library.rows();
        self.as_mut()
            .rust_mut()
            .video_file_model
            .pin_mut()
            .replace(Default::default());
        self.as_mut()
            .rust_mut()
            .recording_model
            .pin_mut()
            .replace(rows);
        self.epgstation_changed();
    }
}
