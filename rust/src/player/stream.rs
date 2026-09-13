//! Coordinate playback requests, native state changes and final shutdown.
//!
//! Delegate resource ordering to playback::Session and retain the single
//! fresh-connection retry here. Feature workers own their own lifecycles.
use super::stream_state::{Attempt, State};
use super::{PlaybackStatus, StatusFailure, channel_refresh, ffi, subtitle_status};
use crate::playback;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::pin::Pin;

impl ffi::Player {
    pub fn connecting(&self) -> bool {
        self.rust().stream_state.connecting()
    }
    pub fn playing(&self) -> bool {
        self.rust().stream_state.playing()
    }
    pub(super) fn update_stream_state(mut self: Pin<&mut Self>, state: State) {
        let was_connecting = self.connecting();
        let was_playing = self.playing();
        // Commit the whole state before notifying Qt: either signal's observers
        // see coherent values for both properties and the active channel.
        self.as_mut().rust_mut().stream_state = state;
        if was_connecting != self.connecting() {
            self.as_mut().connecting_changed();
        }
        if was_playing != self.playing() {
            self.as_mut().playing_changed();
        }
    }
    /// READY joins streaming callbacks before dropping their subscriptions/state.
    pub(super) fn end_stream(mut self: Pin<&mut Self>) -> Result<(), playback::Error> {
        let result = self.as_mut().rust_mut().media.stop().map(|_| ());
        if let Err(error) = result {
            let state = self.rust().stream_state.stop_failed();
            self.as_mut().update_stream_state(state);
            return Err(error);
        }
        self.as_mut().update_stream_state(State::Stopped);
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
        let Some(entry) = self.rust().entries.get(*self.selected() as usize) else {
            return;
        };
        if self.rust().stream_state.requested(entry.id) {
            return;
        }
        let attempt = Attempt::new(entry);
        self.start_stream(attempt);
    }
    fn start_stream(mut self: Pin<&mut Self>, attempt: Attempt) {
        let (id, broadcast) = (attempt.service, attempt.broadcast);
        let server = self.server().to_string();
        let subtitles_enabled = self.rust().subtitles_enabled;
        // Keep stop failure distinct: the previous generation is still owned.
        // A successful stop grants an exclusive capability for the next start.
        let result = {
            let mut this = self.as_mut().rust_mut();
            this.media
                .stop()
                .map(|stopped| stopped.start(&server, id, broadcast, subtitles_enabled))
        };
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                self.playback_failed(error);
                return;
            }
        };
        self.as_mut().update_stream_state(State::Stopped);
        self.as_mut().rust_mut().subtitle_cells = 0;
        self.as_mut().set_subtitle_data(QString::default());
        let active = self.rust().media.subtitles().is_some();
        self.as_mut().set_subtitles_active(active);
        self.as_mut().update_subtitle_status(if active {
            subtitle_status::Status::Parsing
        } else {
            subtitle_status::Status::Stopped
        });
        match result {
            Ok(subtitles) => {
                match subtitles {
                    playback::SubtitleStart::Disabled | playback::SubtitleStart::Parsing => {}
                    playback::SubtitleStart::Failed(error) => self
                        .as_mut()
                        .update_subtitle_status(subtitle_status::Status::Failed(error)),
                }

                self.as_mut()
                    .rust_mut()
                    .preferences
                    .change(crate::settings::Change::Service(id.to_string()));
                let name = attempt.name.clone();
                self.as_mut()
                    .update_stream_state(State::Connecting(attempt));
                self.as_mut()
                    .update_status(PlaybackStatus::Connecting(name));
            }
            Err(error) => self.as_mut().playback_failed(error),
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
            self.as_mut()
                .status_error(StatusFailure::ChannelPresentation, error);
            self.finish_connection(false);
            return;
        }
        self.as_mut().poll_features();
        let result = self.rust().media.playback().map(playback::Playback::poll);
        self.poll_audio_choice();
        match result {
            Some(Ok(true)) => {
                if let Some(attempt) = self.rust().stream_state.started() {
                    let status = PlaybackStatus::Playing(attempt.name.clone());
                    tracing::info!("Pipeline PLAYING service {}", attempt.service);
                    self.as_mut().clear_playback_failure();
                    self.as_mut().update_stream_state(State::Playing(attempt));
                    self.as_mut().update_status(status);
                }
            }
            Some(Err(error)) => {
                let text = error.to_string();
                tracing::error!("Playback error: {text}");
                let retry = error
                    .is_live_resume_rejected()
                    .then(|| self.rust().stream_state.resume_retry())
                    .flatten();
                if let Err(stop_error) = self.as_mut().end_stream() {
                    self.playback_failed(playback::Error::Cleanup {
                        primary: Box::new(error),
                        cleanup: Box::new(stop_error),
                    });
                    return;
                }
                if let Some(attempt) = retry {
                    tracing::warn!("Live resume rejected; opening one fresh stream connection");
                    self.as_mut().start_stream(attempt);
                    if self.connecting() {
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
        let result = self.as_mut().rust_mut().media.shutdown();
        if let Err(error) = result {
            tracing::error!("Playback shutdown failed; keeping the window alive: {error}");
            self.playback_failed(error);
            return false;
        }
        self.as_mut().rust_mut().epg_events.configure(None);
        self.as_mut().rust_mut().channel_refresh = channel_refresh::Refresh::Disabled;
        // Stop UI samples; the application owner retains GC logging through engine teardown.
        self.as_mut().rust_mut().diagnostic_recorder.take();
        self.as_mut().rust_mut().comments.configure(false, None);
        self.as_mut().set_comment_draft(QString::default());
        self.as_mut().refresh_comment_posting();
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
        self.as_mut().cancel_connection();
        self.save_settings();
        true
    }
}
