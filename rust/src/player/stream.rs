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
        let old_program = self.current_program_data().clone();
        let old_progress = *self.program_progress();
        let old_subtitle = self.subtitle_data().clone();
        let old_rate = *self.timeshift_bytes_per_second();
        let old_live_timeline = self.live_timeline().clone();
        let before_live = (
            self.timeshift(),
            self.window_start_ms(),
            self.live_delay_ms(),
            self.window_end_ms(),
            self.duration_estimated(),
        );
        let before = (
            self.connecting(),
            self.playing(),
            self.recording(),
            self.recording_name(),
            self.media_active(),
            self.paused(),
            self.seeking(),
            self.ended(),
            self.seekable(),
            self.position_ms(),
            self.duration_ms(),
        );
        {
            let mut this = self.as_mut().rust_mut();
            this.stream_state = change(std::mem::take(&mut this.stream_state));
            this.timeshift_bytes_per_second =
                this.media.timeshift_bytes_per_second().unwrap_or_default();
            if this.stream_state.active() {
                if let Some((phase, snapshot)) = this.media.timeline() {
                    this.stream_state = std::mem::take(&mut this.stream_state).transport(phase);
                    this.timeline = snapshot;
                }
            } else {
                this.timeline = Default::default();
                this.timeshift_bytes_per_second = 0.0;
            }
            let source_changed = before.2 != this.stream_state.recording().is_some()
                || before.3.to_string()
                    != this.stream_state.recording().map_or("", |file| file.name());
            if source_changed
                || (old_live_timeline.to_string() != "null" && !this.stream_state.active())
                || (!before.6
                    && this.stream_state.seeking()
                    && this.stream_state.recording().is_some())
            {
                this.current_projection = Default::default();
                this.current_program_data = QString::from("null");
                this.program_progress = 0.0;
                this.subtitle_data = QString::default();
                this.subtitle_cells = 0;
            }
            if this.stream_state.active()
                && let Some(snapshot) = this.media.live_timeline()
            {
                let (data, progress) = snapshot.viewing_program();
                this.current_program_data = QString::from(data);
                this.program_progress = progress;
                this.live_timeline = QString::from(snapshot.serialize());
            } else {
                this.live_timeline = QString::from("null");
            }
        }
        if before_live.0 != self.timeshift() {
            self.as_mut().timeshift_changed();
        }
        if before_live.1 != self.window_start_ms() {
            self.as_mut().window_start_ms_changed();
        }
        if before_live.2 != self.live_delay_ms() {
            self.as_mut().live_delay_ms_changed();
        }
        if before_live.3 != self.window_end_ms() {
            self.as_mut().window_end_ms_changed();
        }
        if before_live.4 != self.duration_estimated() {
            self.as_mut().duration_estimated_changed();
        }
        // Commit input and activity together before any Qt observer reads them.
        if old_live_timeline != *self.live_timeline() {
            self.as_mut().live_timeline_changed();
        }
        if old_rate != *self.timeshift_bytes_per_second() {
            self.as_mut().timeshift_bytes_per_second_changed();
        }
        if old_program != *self.current_program_data() {
            self.as_mut().current_program_data_changed();
        }
        if old_progress != *self.program_progress() {
            self.as_mut().program_progress_changed();
        }
        if old_subtitle != *self.subtitle_data() {
            self.as_mut().subtitle_data_changed();
        }
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
        if before.4 != self.media_active() {
            self.as_mut().media_active_changed();
        }
        if before.5 != self.paused() {
            self.as_mut().paused_changed();
        }
        if before.6 != self.seeking() {
            self.as_mut().seeking_changed();
        }
        if before.7 != self.ended() {
            self.as_mut().ended_changed();
        }
        if before.8 != self.seekable() {
            self.as_mut().seekable_changed();
        }
        if before.9 != self.position_ms() {
            self.as_mut().position_ms_changed();
        }
        if before.10 != self.duration_ms() {
            self.as_mut().duration_ms_changed();
        }
        if self.ended() && !before.7 {
            self.as_mut().update_status(PlaybackStatus::Finished);
        } else if before.7 && self.media_active() {
            let name = self.recording_name().to_string();
            self.as_mut().update_status(PlaybackStatus::Playing(name));
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
        let Some(entry) = self.rust().entries.get(*self.selected() as usize) else {
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
        self.as_mut().set_transport_error(QString::default());
        self.as_mut().cancel_recording_open();
        if attempt
            .service()
            .is_some_and(|service| self.rust().stream_state.requested(service))
        {
            return true;
        }
        let server = self.server().to_string();
        let programs_enabled = *self.epg_enabled();
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
        let active = subtitles_enabled && self.rust().media.subtitles().is_some();
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
        let result = self.as_mut().rust_mut().media.poll();
        if let Some(notice) = self.as_mut().rust_mut().media.take_notice() {
            self.as_mut().set_transport_error(QString::from(notice));
        }
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
            }
            Ok(playback::Event::Ended(_)) => match self.as_mut().end_stream() {
                Ok(()) => self.as_mut().playback_failed(playback::Error::EndOfStream),
                Err(error) => self.as_mut().playback_failed(error),
            },
            Err(error) => {
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
                        self.as_mut().update_status(PlaybackStatus::Reconnecting);
                    }
                } else {
                    self.as_mut().playback_failed(error);
                }
            }
            Ok(playback::Event::Idle) => {}
        }
        self.as_mut().change_stream_state(|state| state);
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
