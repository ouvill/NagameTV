//! Coordinate playback requests, native state changes and final shutdown.
//!
//! Keep READY-before-subtitle-release and the single fresh-connection retry in
//! one place. Channel acquisition and feature workers retain their own modules.
use super::{PlaybackStatus, StatusFailure, channel_refresh, ffi, subtitle_status};
use crate::{features::subtitles, playback};
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::pin::Pin;

impl ffi::Player {
    /// READY joins streaming callbacks before dropping their subscriptions/state.
    pub(super) fn end_stream(mut self: Pin<&mut Self>) -> Result<(), playback::Error> {
        // Reveal controls even if the native stop itself fails.
        self.as_mut().set_connecting(false);
        self.as_mut().set_playing(false);
        if let Some(playback) = &self.rust().playback {
            playback.stop()?;
        }
        self.as_mut().rust_mut().subtitle_session = None;
        self.as_mut().rust_mut().active_service = None;
        self.as_mut().set_subtitles_active(false);
        self.as_mut().rust_mut().subtitle_cells = 0;
        self.as_mut().set_subtitle_data(QString::default());
        self.as_mut()
            .update_subtitle_status(subtitle_status::Status::Stopped);
        Ok(())
    }
    pub fn play(mut self: Pin<&mut Self>) {
        self.record_diagnostic(viewer_diagnostics::recorder::Event::PlayRequested);
        self.as_mut().clear_playback_failure();
        self.as_mut().rust_mut().resume_retry_used = false;
        self.start_stream();
    }
    fn start_stream(mut self: Pin<&mut Self>) {
        let Some(entry) = self.rust().entries.get(*self.selected() as usize) else {
            return;
        };
        let (id, name, broadcast) = (entry.id, entry.name.clone(), entry.broadcast);
        let server = self.server().to_string();
        if self.rust().active_service == Some(id) {
            return;
        }
        if let Err(error) = self.as_mut().end_stream() {
            self.playback_failed(error);
            return;
        }
        if self.rust().subtitles_enabled {
            let result = self
                .rust()
                .playback
                .as_ref()
                .ok_or(subtitles::Error::PlaybackUnavailable)
                .and_then(|p| subtitles::Session::start(p.element(), broadcast));
            match result {
                Ok(session) => {
                    self.as_mut().rust_mut().subtitle_session = Some(session);
                    self.as_mut().set_subtitles_active(true);
                    self.as_mut()
                        .update_subtitle_status(subtitle_status::Status::Parsing);
                }
                Err(error) => self
                    .as_mut()
                    .update_subtitle_status(subtitle_status::Status::Failed(error)),
            }
        }
        let result = self
            .rust()
            .playback
            .as_ref()
            .map(|p| p.play(&server, id, broadcast));
        match result {
            Some(Ok(_)) => {
                self.as_mut()
                    .rust_mut()
                    .preferences
                    .preferences_mut()
                    .service_id = id.to_string();
                self.as_mut().rust_mut().active_service = Some(id);
                self.as_mut().set_connecting(true);
                self.as_mut()
                    .update_status(PlaybackStatus::Connecting(name));
            }
            Some(Err(error)) => {
                let failure = match self.as_mut().end_stream() {
                    Ok(()) => error,
                    Err(cleanup) => playback::Error::Cleanup {
                        primary: Box::new(error),
                        cleanup: Box::new(cleanup),
                    },
                };
                self.as_mut().playback_failed(failure);
            }
            None => self.as_mut().playback_failed(playback::Error::Unavailable),
        }
    }
    pub fn stop(mut self: Pin<&mut Self>) {
        // An explicit stop supersedes startup autoplay, including a still-empty catalog.
        self.as_mut().rust_mut().autoplay_pending = false;
        self.record_diagnostic(viewer_diagnostics::recorder::Event::StopRequested);
        match self.as_mut().end_stream() {
            Ok(()) => self.update_status(PlaybackStatus::Stopped),
            Err(error) => self.playback_failed(error),
        }
    }
    pub fn poll(mut self: Pin<&mut Self>) {
        self.as_mut().refresh_channels_if_due();
        if let Err(error) = self.as_mut().poll_channels() {
            self.status_error(StatusFailure::ChannelPresentation, error);
            return;
        }
        self.as_mut().poll_features();
        let result = self.rust().playback.as_ref().map(playback::Playback::poll);
        self.poll_audio_choice();
        match result {
            Some(Ok(true)) => {
                self.as_mut().clear_playback_failure();
                self.as_mut().set_playing(true);
                self.as_mut().set_connecting(false);
                if let Some(entry) = self.rust().entries.get(*self.selected() as usize) {
                    let status = PlaybackStatus::Playing(entry.name.clone());
                    tracing::info!("Pipeline PLAYING service {}", entry.id);
                    self.as_mut().update_status(status);
                }
            }
            Some(Err(error)) => {
                let text = error.to_string();
                tracing::error!("Playback error: {text}");
                let recover = error.is_live_resume_rejected()
                    && self.rust().active_service.is_some()
                    && !self.rust().resume_retry_used;
                if let Err(stop_error) = self.as_mut().end_stream() {
                    self.playback_failed(playback::Error::Cleanup {
                        primary: Box::new(error),
                        cleanup: Box::new(stop_error),
                    });
                    return;
                }
                if recover {
                    self.as_mut().rust_mut().resume_retry_used = true;
                    tracing::warn!("Live resume rejected; opening one fresh stream connection");
                    self.as_mut().start_stream();
                    if self.rust().active_service.is_some() {
                        self.update_status(PlaybackStatus::Reconnecting);
                    }
                } else {
                    self.playback_failed(error);
                }
            }
            _ => {}
        }
    }
    pub fn shutdown(mut self: Pin<&mut Self>) -> bool {
        if let Some(playback) = self.as_mut().rust_mut().playback.as_mut()
            && let Err(error) = playback.shutdown()
        {
            tracing::error!("Playback shutdown failed; keeping the window alive: {error}");
            self.playback_failed(error);
            return false;
        }
        self.as_mut().rust_mut().epg_events.configure(None);
        self.as_mut().rust_mut().channel_refresh = channel_refresh::Refresh::Disabled;
        // Stop UI samples; the application owner retains GC logging through engine teardown.
        self.as_mut().rust_mut().diagnostic_recorder.take();
        self.as_mut().rust_mut().comments.configure(false, None);
        self.as_mut().clear_comment_history();
        self.as_mut().rust_mut().activity.configure(false);
        self.as_mut().set_activity_data(QString::from("[]"));
        self.as_mut().set_comment_program_title(QString::default());
        self.as_mut().browser_open(false);
        self.as_mut().rust_mut().epg.configure(None);
        self.as_mut().guide_open(false);
        if let Err(error) = self.as_mut().end_stream() {
            tracing::error!("Stream cleanup after shutdown failed: {error}");
        }
        self.as_mut().rust_mut().request.cancel();
        self.save_settings();
        true
    }
}
