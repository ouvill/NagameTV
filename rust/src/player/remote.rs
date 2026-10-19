//! Execute the same player operations as QML, exclusively on the Qt thread.
use super::error_text::PresentError;
use super::{PlayerRust, ffi, stream_state};
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::{
    pin::Pin,
    time::{SystemTime, UNIX_EPOCH},
};
use viewer_remote::model::{self, Command, CommandError};

impl PlayerRust {
    pub(crate) fn start_remote_if_requested(&mut self) -> bool {
        if !self.remote.needs_start() {
            return false;
        }
        let state = self.remote_state();
        let channels = self.remote_channels();
        self.remote.start(self.network.as_ref(), state, channels);
        true
    }

    fn remote_channels(&self) -> Vec<model::Channel> {
        let now = epoch_ms();
        self.catalog
            .channels()
            .iter()
            .map(|channel| model::Channel {
                id: channel.id,
                name: channel.name.clone(),
                label: channel.label.clone(),
                band: crate::remote::band(channel.band),
                remote_key: channel
                    .remote_key
                    .map(u32::from)
                    .filter(|key| (1..=12).contains(key)),
                current_program: now
                    .and_then(|now| self.epg.current(channel.broadcast, now))
                    .map(remote_program),
            })
            .collect()
    }

    fn remote_state(&self) -> model::State {
        use stream_state::{Attempt, State};
        let (playback, broadcast) = match &self.stream_state {
            State::Stopped(_) => (model::Playback::Stopped, None),
            State::Connecting(Attempt::Live(live)) => (
                model::Playback::Connecting(live.service()),
                live.broadcast(),
            ),
            State::Playing(live, phase) => {
                use crate::playback::timeline::{Phase, Resume};
                let id = live.service();
                let playback = match phase {
                    Phase::Playing => model::Playback::Playing(id),
                    Phase::Paused => model::Playback::Paused(id),
                    Phase::Seeking(Resume::Playing) => model::Playback::Seeking(id),
                    Phase::Seeking(Resume::Paused) => model::Playback::SeekingPaused(id),
                    Phase::Ended => model::Playback::Ended(id),
                };
                (playback, live.broadcast())
            }
            State::StopFailed(Attempt::Live(live)) => (
                model::Playback::StopFailed(live.service()),
                live.broadcast(),
            ),
            State::Connecting(Attempt::File(file)) => (
                model::Playback::FileConnecting(file.name().to_owned()),
                None,
            ),
            State::Recording(file, phase) => {
                use crate::playback::timeline::{Phase, Resume};
                let name = file.name().to_owned();
                let playback = match phase {
                    Phase::Playing => model::Playback::FilePlaying(name),
                    Phase::Paused => model::Playback::FilePaused(name),
                    Phase::Seeking(Resume::Playing) => model::Playback::FileSeeking(name),
                    Phase::Seeking(Resume::Paused) => model::Playback::FileSeekingPaused(name),
                    Phase::Ended => model::Playback::FileEnded(name),
                };
                (playback, None)
            }
            State::StopFailed(Attempt::File(file)) => (
                model::Playback::FileStopFailed(file.name().to_owned()),
                None,
            ),
        };
        let now = epoch_ms();
        // The remote API's program IDs come from the server's wall-clock EPG.
        // Do not label retained playback with a program currently on air.
        let broadcast = if self.stream_state.timeshift(self.timeline.range.is_some()) {
            None
        } else {
            broadcast
        };
        let current_program = now
            .and_then(|now| self.epg.current(broadcast, now))
            .map(remote_program);
        let recording = self.stream_state.recording().is_some();
        let timeshift = self.stream_state.timeshift(self.timeline.range.is_some());
        let can_pause = self.stream_state.active() && (recording || self.stream_state.pausable());
        let can_seek = can_pause && (recording || timeshift) && self.timeline.range.is_some();
        let live_ready = self.media.playback().is_some()
            && !self.preferences.preferences().server.trim().is_empty()
            && self.pending_server.is_none();
        let previous = self.stream_state.active_service().and_then(|current| {
            self.viewing_history.previous(current, |id| {
                self.catalog.channels().iter().any(|c| c.id == id)
            })
        });
        model::State {
            selected_channel_id: self.catalog.selected().map(|channel| channel.id),
            playback,
            volume: model::Volume::new(self.audio_output.volume().fraction())
                .expect("validated application volume"),
            muted: self.audio_output.muted(),
            subtitles: if !self.subtitles_enabled {
                model::SubtitleDisplay::Disabled
            } else if self.subtitle_display {
                model::SubtitleDisplay::Visible
            } else {
                model::SubtitleDisplay::Hidden
            },
            playback_error: self.playback_error.render().to_string(),
            settings_error: self.settings_error.render().to_string(),
            current_program,
            transport: model::Transport {
                position_ms: self.timeline.position.map(|value| value.mseconds()),
                duration_ms: self.timeline.duration.map(|value| value.mseconds()),
                seek_range: self
                    .timeline
                    .range
                    .filter(|_| can_seek)
                    .map(|range| (range.start().mseconds(), range.end().mseconds())),
                playback_rate_tenths: self.speed.applied.tenths(),
                can_pause,
                can_seek,
                can_return_to_live: can_pause && timeshift,
                can_set_rate: can_pause
                    && self.speed.availability == crate::playback::speed::Availability::Variable,
                at_live_edge: match self.stream_state {
                    State::Playing(_, phase) => self.speed.playing_at_live_edge(phase),
                    _ => false,
                },
                can_step_channel: live_ready && !self.catalog.channels().is_empty(),
                can_previous_channel: live_ready && previous.is_some(),
            },
        }
    }
}

impl ffi::Player {
    pub fn remote_watchers(&self) -> i32 {
        self.rust()
            .remote
            .session()
            .map_or(0, |session| session.watchers() as i32)
    }
    pub fn remote_enabled(&self) -> bool {
        self.rust().remote.settings().enabled()
    }
    pub fn remote_address(&self) -> QString {
        QString::from(self.rust().remote.settings().endpoint().ip().to_string())
    }
    pub fn remote_port(&self) -> i32 {
        i32::from(self.rust().remote.settings().endpoint().port())
    }
    pub fn remote_status(&self) -> QString {
        QString::from(self.rust().remote.status())
    }
    pub fn remote_error(&self) -> QString {
        self.rust()
            .remote
            .error()
            .map(|error| error.present().render())
            .unwrap_or_default()
    }
    pub fn remote_save_error(&self) -> QString {
        self.rust()
            .remote
            .save_error()
            .map(|error| error.present().render())
            .unwrap_or_default()
    }
    pub fn remote_endpoints(&self) -> QString {
        QString::from(self.rust().remote.endpoints())
    }
    pub fn remote_session_only(&self) -> bool {
        self.rust().remote.session_only()
    }

    fn notify_remote(mut self: Pin<&mut Self>) {
        // Every getter already reflects the complete update before any signal fires.
        self.as_mut().remote_watchers_changed();
        self.as_mut().remote_enabled_changed();
        self.as_mut().remote_address_changed();
        self.as_mut().remote_port_changed();
        self.as_mut().remote_status_changed();
        self.as_mut().remote_error_changed();
        self.as_mut().remote_save_error_changed();
        self.as_mut().remote_endpoints_changed();
        self.as_mut().remote_session_only_changed();
    }
    pub fn refresh_remote_addresses(self: Pin<&mut Self>) {
        self.remote_endpoints_changed();
    }
    pub fn configure_remote(
        mut self: Pin<&mut Self>,
        enabled: bool,
        address: QString,
        port: i32,
    ) -> bool {
        let Ok(settings) =
            crate::remote::settings::Settings::parse(enabled, &address.to_string(), port)
        else {
            return false;
        };
        self.as_mut().rust_mut().remote.configure(settings);
        self.as_mut().rust_mut().start_remote_if_requested();
        self.notify_remote();
        true
    }
    pub(super) fn poll_remote_commands(mut self: Pin<&mut Self>) {
        let changed = self.as_mut().rust_mut().remote.poll();
        let started = self.as_mut().rust_mut().start_remote_if_requested();
        if changed || started {
            self.as_mut().notify_remote();
        }
        // Bound work per UI tick, including cancelled requests.
        for _ in 0..8 {
            let pending = self
                .as_mut()
                .rust_mut()
                .remote
                .session_mut()
                .and_then(|session| session.next_request());
            let Some(pending) = pending else {
                break;
            };
            let Some(request) = pending.claim() else {
                continue;
            };
            let command = request.command();
            let result = self.as_mut().execute_remote(command);
            if result.is_ok() {
                self.as_mut()
                    .remote_command_executed(viewer_remote::command_kind(command) as i32);
            } else if let Err(error) = &result {
                tracing::warn!(
                    ?command,
                    error = error as &dyn std::error::Error,
                    "Remote command failed"
                );
            }
            // Publish before replying so a following GetState includes this operation.
            self.as_mut().publish_remote();
            request.complete(result);
        }
    }

    pub(super) fn publish_remote(mut self: Pin<&mut Self>) {
        let Some(session) = self.rust().remote.session() else {
            return;
        };
        let candidate = self.rust().remote_channels();
        let channels = (session.channels() != candidate).then_some(candidate);
        let state = self.rust().remote_state();
        if let Some(session) = self.as_mut().rust_mut().remote.session_mut() {
            session.publish(state, channels);
        }
    }

    fn execute_remote(mut self: Pin<&mut Self>, command: Command) -> Result<(), CommandError> {
        match command {
            Command::SelectChannel(id) => {
                let index = self
                    .rust()
                    .catalog
                    .channels()
                    .iter()
                    .position(|channel| channel.id == id)
                    .ok_or(CommandError::ChannelNotFound)?;
                self.remote_live_ready()?;
                self.as_mut().select(index as i32);
                self.remote_play_result()
            }
            Command::Play => {
                self.remote_output_ready()?;
                if !self.recording() {
                    self.remote_live_ready()?;
                }
                if !self.recording() && self.rust().catalog.selected().is_none() {
                    return Err(CommandError::NotReady("no channel selected"));
                }
                self.as_mut().play();
                self.remote_play_result()
            }
            Command::Stop => {
                self.as_mut().stop();
                if matches!(self.rust().stream_state, stream_state::State::StopFailed(_)) {
                    Err(CommandError::Playback(self.playback_error().to_string()))
                } else {
                    Ok(())
                }
            }
            Command::SetPaused(paused) => {
                if !self.pausable() {
                    return Err(CommandError::NotReady("pause is unavailable"));
                }
                if self.paused() == paused {
                    return Ok(());
                }
                self.remote_transport(|media| {
                    media.transport_control()?.resume(if paused {
                        crate::playback::timeline::Resume::Paused
                    } else {
                        crate::playback::timeline::Resume::Playing
                    })
                })
            }
            Command::SeekRelative(delta) => {
                if !self.seekable() {
                    return Err(CommandError::NotReady("seeking is unavailable"));
                }
                self.remote_transport(|media| media.transport_control()?.skip(delta.milliseconds()))
            }
            Command::SeekTo(position) => {
                if !self.seekable() {
                    return Err(CommandError::NotReady("seeking is unavailable"));
                }
                self.remote_transport(|media| {
                    media.transport_control()?.seek(position.milliseconds())
                })
            }
            Command::ReturnToLive => {
                if !self.timeshift() {
                    return Err(CommandError::NotReady(
                        "retained live playback is unavailable",
                    ));
                }
                self.remote_transport(crate::playback::Session::return_to_live)
            }
            Command::SetPlaybackRate(rate) => {
                if !self.speed_available() {
                    return Err(CommandError::NotReady("speed control is unavailable"));
                }
                self.remote_transport(|media| {
                    media.transport_control()?.set_rate(
                        crate::playback::speed::Rate::checked(rate.tenths())
                            .expect("validated remote rate"),
                    )
                })
            }
            Command::StepChannel(step) => {
                self.remote_live_ready()?;
                if self.rust().catalog.channels().is_empty() {
                    return Err(CommandError::ChannelNotFound);
                }
                self.as_mut().step_channel(step.offset());
                self.remote_play_result()
            }
            Command::PreviousChannel => {
                self.remote_live_ready()?;
                if !self.previous_channel_available() {
                    return Err(CommandError::NotReady("no previous channel"));
                }
                self.as_mut().previous_channel();
                self.remote_play_result()
            }
            Command::SelectRemoteKey(number, band) => {
                self.remote_live_ready()?;
                let band = match band {
                    None => "",
                    Some(model::Band::Terrestrial) => "GR",
                    Some(model::Band::Bs) => "BS",
                    Some(model::Band::Cs) => "CS",
                    Some(model::Band::Sky) => "SKY",
                    Some(model::Band::Other) => "OTHER",
                };
                if !self
                    .as_mut()
                    .select_remote_key(number.number(), QString::from(band))
                {
                    return Err(CommandError::ChannelNotFound);
                }
                self.remote_play_result()
            }
            Command::SendKey(key) => {
                self.as_mut().remote_key_requested(key as i32);
                Ok(())
            }
            Command::SetVolume(volume) => {
                self.volume(volume.fraction());
                Ok(())
            }
            Command::SetMuted(muted) => {
                self.mute(muted);
                Ok(())
            }
            Command::SetSubtitles(visible) => {
                if !self.rust().subtitles_enabled {
                    return Err(CommandError::NotReady("subtitle feature is disabled"));
                }
                // Repeating the desired state must not erase the current cue.
                if self.rust().subtitle_display != visible {
                    self.display_subtitles(visible);
                }
                Ok(())
            }
        }
    }

    fn remote_transport(
        self: Pin<&mut Self>,
        operation: impl FnOnce(
            &mut crate::playback::Session,
        ) -> Result<(), crate::playback::timeline::Error>,
    ) -> Result<(), CommandError> {
        use crate::playback::timeline::Error;
        self.control_transport_result(operation)
            .map_err(|error| match error {
                Error::Unavailable | Error::RateUnavailable => {
                    CommandError::NotReady("transport is not ready")
                }
                error => CommandError::Playback(error.to_string()),
            })
    }

    fn remote_output_ready(&self) -> Result<(), CommandError> {
        if self.rust().media.playback().is_none() {
            return Err(CommandError::NotReady("playback output is unavailable"));
        }
        Ok(())
    }

    fn remote_live_ready(&self) -> Result<(), CommandError> {
        self.remote_output_ready()?;
        if !self.server_configured() || self.loading() {
            return Err(CommandError::NotReady("server connection is not ready"));
        }
        Ok(())
    }

    fn remote_play_result(&self) -> Result<(), CommandError> {
        if self.recording_loading() {
            return Ok(());
        }
        match self.rust().stream_state {
            stream_state::State::Connecting(_)
            | stream_state::State::Playing(_, _)
            | stream_state::State::Recording(_, _) => Ok(()),
            stream_state::State::Stopped(_) | stream_state::State::StopFailed(_) => {
                Err(CommandError::Playback(self.playback_error().to_string()))
            }
        }
    }
}

fn epoch_ms() -> Option<u64> {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => Some(
            duration
                .as_millis()
                .try_into()
                .expect("epoch milliseconds fit u64"),
        ),
        Err(error) => {
            tracing::warn!(%error, "Remote program publication cannot read wall clock");
            None
        }
    }
}
fn remote_program(program: &crate::features::program_info::model::Program) -> model::Program {
    model::Program {
        id: program.id,
        name: program.name.clone().unwrap_or_default(),
        description: program.description.clone().unwrap_or_default(),
        start_at_ms: program.start_at,
        duration_ms: program.duration,
    }
}
