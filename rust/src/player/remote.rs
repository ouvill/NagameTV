//! Execute the same player operations as QML, exclusively on the Qt thread.
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
        self.entries
            .iter()
            .map(|channel| model::Channel {
                id: channel.id,
                name: channel.name.clone(),
                label: channel.label.clone(),
                band: crate::remote::band(channel.band),
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
            State::Playing(Attempt::Live(live)) => {
                (model::Playback::Playing(live.service()), live.broadcast())
            }
            State::StopFailed(Attempt::Live(live)) => (
                model::Playback::StopFailed(live.service()),
                live.broadcast(),
            ),
            State::Connecting(Attempt::File(file)) => (
                model::Playback::FileConnecting(file.name().to_owned()),
                None,
            ),
            State::Playing(Attempt::File(file)) => {
                (model::Playback::FilePlaying(file.name().to_owned()), None)
            }
            State::StopFailed(Attempt::File(file)) => (
                model::Playback::FileStopFailed(file.name().to_owned()),
                None,
            ),
        };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|duration| u64::try_from(duration.as_millis()).ok());
        let current_program = now
            .and_then(|now| self.epg.current(broadcast, now))
            .map(|program| model::Program {
                id: program.id,
                name: program.name.clone().unwrap_or_default(),
                description: program.description.clone().unwrap_or_default(),
                start_at_ms: program.start_at,
                duration_ms: program.duration,
            });
        model::State {
            selected_channel_id: usize::try_from(self.selected)
                .ok()
                .and_then(|index| self.entries.get(index))
                .map(|channel| channel.id),
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
            playback_error: self.playback_error.to_string(),
            settings_error: self.settings_error.to_string(),
            current_program,
        }
    }
}

impl ffi::Player {
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
        QString::from(self.rust().remote.error())
    }
    pub fn remote_save_error(&self) -> QString {
        QString::from(self.rust().remote.save_error())
    }
    pub fn remote_endpoints(&self) -> QString {
        QString::from(self.rust().remote.endpoints())
    }
    pub fn remote_session_only(&self) -> bool {
        self.rust().remote.session_only()
    }

    fn notify_remote(mut self: Pin<&mut Self>) {
        // Every getter already reflects the complete update before any signal fires.
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
            let result = self.as_mut().execute_remote(request.command());
            // Publish before replying so a following GetState includes this operation.
            self.as_mut().publish_remote();
            request.complete(result);
        }
    }

    pub(super) fn publish_remote(mut self: Pin<&mut Self>) {
        let Some(session) = self.rust().remote.session() else {
            return;
        };
        let channels_changed = session.channels().len() != self.rust().entries.len()
            || session
                .channels()
                .iter()
                .zip(&self.rust().entries)
                .any(|(a, b)| {
                    a.id != b.id
                        || a.name != b.name
                        || a.label != b.label
                        || a.band != crate::remote::band(b.band)
                });
        let channels = channels_changed.then(|| self.rust().remote_channels());
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
                    .entries
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
                if !self.recording() && self.rust().entries.get(*self.selected() as usize).is_none()
                {
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
            stream_state::State::Connecting(_) | stream_state::State::Playing(_) => Ok(()),
            stream_state::State::Stopped(_) | stream_state::State::StopFailed(_) => {
                Err(CommandError::Playback(self.playback_error().to_string()))
            }
        }
    }
}
