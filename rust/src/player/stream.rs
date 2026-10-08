//! Coordinate playback requests, native state changes and final shutdown.
//!
//! Delegate resource ordering to playback::Session and retain the single
//! fresh-connection retry here. Feature workers own their own lifecycles.
use super::error_text::PresentError;
use super::stream_state::{Attempt, State};
use super::{PlaybackStatus, channel_refresh, ffi, subtitle_status};
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
    pub(super) fn change_stream_state(self: Pin<&mut Self>, change: impl FnOnce(State) -> State) {
        super::stream_projection::apply(self, change);
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
        self.as_mut().reset_media_subtitles();
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
        if self.paused() {
            self.resume_transport();
            return;
        }
        if self.ended() {
            self.seek_to(0.0);
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
        let Some(entry) = self.rust().catalog.selected() else {
            return;
        };
        if self.rust().stream_state.requested(entry.id) {
            return;
        }
        let attempt = Attempt::new(
            entry,
            self.rust().preferences.preferences().timeshift_policy(),
        );
        self.start_stream(attempt);
    }
    pub(super) fn start_stream(mut self: Pin<&mut Self>, attempt: Attempt) -> bool {
        self.as_mut()
            .set_transport_message(super::transport::Message::None);
        self.as_mut().cancel_recording_open();
        if attempt
            .service()
            .is_some_and(|service| self.rust().stream_state.requested(service))
        {
            return true;
        }
        // Decide against the source that owned the open request. Automatic
        // reconnect may already have torn down the previous stream state.
        if !self.rust().data_broadcast_mode.continues(&attempt) {
            self.as_mut().data_broadcast_open(false);
            self.as_mut().rust_mut().data_broadcast_mode.close();
        }
        let server = self.server().to_string();
        // Audio selection always needs position-scoped EIT, independently of
        // Mirakurun EPG and comment settings. Their UI/network gates stay separate.
        let programs_enabled = true;
        let subtitles_enabled = self.rust().subtitles_enabled;
        // Keep stop failure distinct: the previous generation is still owned.
        // A successful stop grants an exclusive capability for the next start.
        let result = {
            let mut this = self.as_mut().rust_mut();
            this.media.stop().map(|stopped| match &attempt {
                Attempt::Live(live) => stopped.start(
                    &server,
                    live.service(),
                    live.broadcast(),
                    subtitles_enabled,
                    programs_enabled,
                    live.retention(),
                ),
                Attempt::File(file) => {
                    stopped.start_file(file, subtitles_enabled, programs_enabled)
                }
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
        self.as_mut().reset_media_subtitles();
        let active = subtitles_enabled
            && (self.rust().media.subtitles().is_some() || self.media_subtitle_available());
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
        self.as_mut().data_broadcast_open(false);
        self.as_mut().rust_mut().data_broadcast_mode.close();
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
        self.as_mut().poll_seek_preview();
        self.as_mut()
            .expire_transport_notice(std::time::Instant::now());
        let aspect = self
            .rust()
            .media
            .playback()
            .and_then(|playback| playback.video_aspect_ratio())
            .unwrap_or(0.0);
        self.as_mut().set_video_aspect_ratio(aspect);
        self.as_mut().publish_remote();
        #[cfg(target_os = "linux")]
        self.poll_desktop_media();
    }
    fn poll_player(mut self: Pin<&mut Self>) {
        self.as_mut().refresh_channels_if_due();
        self.as_mut().poll_channels();
        self.as_mut().poll_recording();
        self.as_mut().poll_recording_library();
        self.as_mut().poll_updates();
        let result = self.as_mut().rust_mut().media.poll();
        let notice = self.as_mut().rust_mut().media.take_notice();
        self.poll_audio_choice();
        match result {
            Ok(playback::Event::Playing) => {
                if let State::Connecting(attempt) = &self.rust().stream_state {
                    let status = PlaybackStatus::Playing(attempt.name().to_owned());
                    tracing::info!(service = ?self.rust().stream_state.active_service(), "Pipeline PLAYING");
                    self.as_mut().clear_playback_failure();
                    self.as_mut().change_stream_state(State::started);
                    self.as_mut().update_status(status);
                }
            }
            Ok(playback::Event::Ended(_)) if self.recording() => {
                self.as_mut().update_status(PlaybackStatus::Finished);
                self.as_mut().rust_mut().subtitle_cells = 0;
                self.as_mut().set_subtitle_data(QString::default());
                self.as_mut().reset_media_subtitles();
            }
            Ok(playback::Event::Ended(_)) => match self.as_mut().end_stream() {
                Ok(()) => self.as_mut().playback_failed(playback::Error::EndOfStream),
                Err(error) => self.as_mut().playback_failed(error),
            },
            Err(playback::Error::Transport(error @ playback::timeline::Error::RateTimedOut)) => {
                tracing::error!(
                    error = &error as &dyn std::error::Error,
                    "Playback rate change timed out"
                );
                // The controller has paused and requires a confirmed normal-rate
                // seek before resuming. Keep this source and the paused frame.
                self.as_mut().change_stream_state(|state| state);
                self.as_mut()
                    .set_transport_message(super::transport::Message::Failure(error.present()));
            }
            Err(error) => {
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
                    tracing::warn!(
                        error = &error as &dyn std::error::Error,
                        "Live resume rejected; opening one fresh stream connection"
                    );
                    self.as_mut().start_stream(attempt);
                    if self.connecting() {
                        self.as_mut().update_status(PlaybackStatus::Reconnecting);
                    }
                } else {
                    self.as_mut().playback_failed(error);
                }
            }
            Ok(playback::Event::Idle) => {}
        }
        self.as_mut().change_stream_state(|state| state);
        if let Some(notice) = notice {
            self.as_mut()
                .set_transport_message(super::transport::Message::notice(
                    notice,
                    std::time::Instant::now(),
                ));
        }
        // Commentary uses the output-confirmed position and phase from this
        // tick, so a completed seek publishes its restored window before draw.
        self.as_mut().poll_features();
    }
    pub fn shutdown(mut self: Pin<&mut Self>) -> bool {
        self.as_mut().data_broadcast_open(false);
        self.as_mut().rust_mut().data_broadcast_mode.close();
        self.as_mut().cancel_epgstation();
        self.as_mut().cancel_recording_open();
        // Retained native screenshot buffers may still need the GL context for
        // readback. Complete accepted work before stopping that context.
        self.as_mut().rust_mut().screenshot_saves.finish();
        let result = self.as_mut().rust_mut().media.shutdown();
        if let Err(error) = result {
            // playback_failed records the cause; keep the window alive for a retry.
            self.playback_failed(error);
            return false;
        }
        #[cfg(target_os = "linux")]
        {
            self.as_mut().rust_mut().desktop_media =
                super::desktop_media::Registration::Unavailable;
        }
        self.as_mut().rust_mut().updates.stop();
        self.as_mut().updates_changed();
        self.as_mut().rust_mut().remote.stop();
        self.as_mut().rust_mut().epg_events.configure(None);
        self.as_mut().rust_mut().channel_refresh = channel_refresh::Refresh::Disabled;
        // Stop UI samples; the application owner retains GC logging through engine teardown.
        self.as_mut().rust_mut().diagnostic_recorder.take();
        let update = self.as_mut().rust_mut().commentary.shutdown();
        super::comment_projection::publish(self.as_mut(), Some(update));
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
