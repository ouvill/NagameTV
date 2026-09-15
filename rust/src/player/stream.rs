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
    pub fn recording(&self) -> bool {
        self.rust().stream_state.recording().is_some()
    }
    pub fn recording_name(&self) -> QString {
        self.rust()
            .stream_state
            .recording()
            .map(|file| QString::from(file.name()))
            .unwrap_or_default()
    }
    pub fn connecting(&self) -> bool {
        self.rust().stream_state.connecting()
    }
    pub fn playing(&self) -> bool {
        self.rust().stream_state.playing()
    }
    pub(super) fn update_stream_state(self: Pin<&mut Self>, state: State) {
        self.change_stream_state(|_| state);
    }
    pub(super) fn change_stream_state(
        mut self: Pin<&mut Self>,
        change: impl FnOnce(State) -> State,
    ) {
        let before = (
            self.connecting(),
            self.playing(),
            self.recording(),
            self.recording_name(),
        );
        {
            let mut this = self.as_mut().rust_mut();
            this.stream_state = change(std::mem::take(&mut this.stream_state));
        }
        // Commit input and activity together before any Qt observer reads them.
        if before.0 != self.connecting() {
            self.as_mut().connecting_changed();
        }
        if before.1 != self.playing() {
            self.as_mut().playing_changed();
        }
        if before.2 != self.recording() {
            self.as_mut().recording_changed();
        }
        if before.3 != self.recording_name() {
            self.as_mut().recording_name_changed();
        }
    }
    /// READY joins streaming callbacks before dropping their subscriptions/state.
    pub(super) fn end_stream(mut self: Pin<&mut Self>) -> Result<(), playback::Error> {
        let result = self.as_mut().rust_mut().media.stop().map(|_| ());
        if let Err(error) = result {
            self.as_mut().change_stream_state(State::stop_failed);
            return Err(error);
        }
        self.as_mut().change_stream_state(State::stopped);
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
        if self.recording_loading() {
            return;
        }
        if let Some(file) = self.rust().stream_state.recording() {
            if self.playing() || self.connecting() {
                return;
            }
            let request = file.replay();
            self.begin_recording(request);
            return;
        }
        let Some(entry) = self.rust().entries.get(*self.selected() as usize) else {
            return;
        };
        if self.rust().stream_state.requested(entry.id) {
            return;
        }
        let attempt = Attempt::new(entry);
        self.start_stream(attempt);
    }
    pub(super) fn start_stream(mut self: Pin<&mut Self>, attempt: Attempt) -> bool {
        self.as_mut().cancel_recording_open();
        if attempt
            .service()
            .is_some_and(|service| self.rust().stream_state.requested(service))
        {
            return true;
        }
        let server = self.server().to_string();
        let subtitles_enabled = self.rust().subtitles_enabled;
        // Keep stop failure distinct: the previous generation is still owned.
        // A successful stop grants an exclusive capability for the next start.
        let result = {
            let mut this = self.as_mut().rust_mut();
            this.media.stop().map(|stopped| match &attempt {
                Attempt::Live(live) => {
                    stopped.start(&server, live.service(), live.broadcast(), subtitles_enabled)
                }
                Attempt::File(file) => stopped.start_file(file, subtitles_enabled),
            })
        };
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                self.playback_failed(error);
                return false;
            }
        };
        let service = attempt.service();
        let name = attempt.name().to_owned();
        let state = match &result {
            Ok(_) => State::Connecting(attempt),
            Err(playback::Error::Cleanup { .. }) => State::StopFailed(attempt),
            Err(_) => attempt.stopped(),
        };
        self.as_mut().update_stream_state(state);
        self.as_mut().poll_current_program(None);
        self.as_mut().poll_comments();
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

                if let Some(id) = service {
                    self.as_mut()
                        .rust_mut()
                        .preferences
                        .change(crate::settings::Change::Service(id.to_string()));
                }
                self.as_mut()
                    .update_status(PlaybackStatus::Connecting(name));
                true
            }
            Err(error) => {
                self.as_mut().playback_failed(error);
                false
            }
        }
    }
    pub fn stop(mut self: Pin<&mut Self>) {
        self.as_mut().cancel_recording_open();
        // An explicit stop supersedes startup autoplay, including a still-empty catalog.
        self.as_mut().rust_mut().autoplay_pending = false;
        self.record_diagnostic(viewer_diagnostics::recorder::Event::StopRequested);
        match self.as_mut().end_stream() {
            Ok(()) => self.update_status(PlaybackStatus::Stopped),
            Err(error) => self.playback_failed(error),
        }
    }
    pub fn poll(mut self: Pin<&mut Self>) {
        self.as_mut().poll_remote_commands();
        self.as_mut().poll_player();
        self.publish_remote();
    }
    fn poll_player(mut self: Pin<&mut Self>) {
        self.as_mut().refresh_channels_if_due();
        if let Err(error) = self.as_mut().poll_channels() {
            self.as_mut()
                .status_error(StatusFailure::ChannelPresentation, error);
            self.finish_connection(false);
            return;
        }
        self.as_mut().poll_recording();
        self.as_mut().poll_features();
        let result = self.rust().media.playback().map(playback::Playback::poll);
        self.poll_audio_choice();
        match result {
            Some(Ok(playback::Event::Playing)) => {
                if let State::Connecting(attempt) = &self.rust().stream_state {
                    let status = PlaybackStatus::Playing(attempt.name().to_owned());
                    tracing::info!(service = ?self.rust().stream_state.active_service(), "Pipeline PLAYING");
                    self.as_mut().clear_playback_failure();
                    self.as_mut().change_stream_state(State::started);
                    self.as_mut().update_status(status);
                }
            }
            Some(Ok(playback::Event::Ended)) => match self.as_mut().end_stream() {
                Ok(()) if self.recording() => self.as_mut().update_status(PlaybackStatus::Finished),
                Ok(()) => self.as_mut().playback_failed(playback::Error::EndOfStream),
                Err(error) => self.as_mut().playback_failed(error),
            },
            Some(Err(error)) => {
                let text = error.to_string();
                tracing::error!("Playback error: {text}");
                let retry = error
                    .is_live_resume_rejected()
                    .then(|| self.as_mut().rust_mut().stream_state.take_retry())
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
            Some(Ok(playback::Event::Idle)) | None => {}
        }
    }
    pub fn shutdown(mut self: Pin<&mut Self>) -> bool {
        self.as_mut().cancel_recording_open();
        let result = self.as_mut().rust_mut().media.shutdown();
        if let Err(error) = result {
            tracing::error!("Playback shutdown failed; keeping the window alive: {error}");
            self.playback_failed(error);
            return false;
        }
        self.as_mut().rust_mut().remote.stop();
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
