//! Linux desktop media session. The registered native owner is the only state
//! allowed to publish or consume commands; dropping it removes the D-Bus name.
use super::{ffi, stream_state::State};
use crate::playback::timeline::{Phase, Resume};
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use serde::{Deserialize, Serialize};
use std::pin::Pin;

const MICROSECONDS_PER_MILLISECOND: f64 = 1000.0;
const COMMANDS_PER_TICK: usize = 8;

#[derive(Default)]
pub(super) enum Registration {
    #[default]
    Pending,
    Unavailable,
    Registered(cxx::UniquePtr<ffi::DesktopMedia>),
}

#[derive(Serialize)]
enum Status {
    Stopped,
    Playing,
    Paused,
}

fn status(state: &State) -> Status {
    match state {
        State::Stopped(_) | State::Connecting(_) | State::StopFailed(_) => Status::Stopped,
        State::Playing(_, phase) | State::Recording(_, phase) => match phase {
            Phase::Playing | Phase::Seeking(Resume::Playing) => Status::Playing,
            Phase::Paused | Phase::Seeking(Resume::Paused) => Status::Paused,
            Phase::Ended => Status::Stopped,
        },
    }
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Command {
    Play,
    Pause,
    PlayPause,
    Stop,
    Raise,
    Volume { value: f64 },
    Seek { value: f64, track: String },
    SetPosition { value: f64, track: String },
}

impl ffi::Player {
    fn desktop_can_pause(&self) -> bool {
        self.recording()
            || self.timeshift()
            || (!self.media_active()
                && *self.selected() >= 0
                && self.rust().preferences.preferences().timeshift
                    != crate::playback::input::Retention::Off)
    }

    fn desktop_track(&self) -> String {
        if !self.media_active() || self.ended() {
            return String::new();
        }
        self.rust()
            .media
            .source_identity()
            .map_or_else(String::new, |id| {
                format!("/org/mpris/MediaPlayer2/track/{id}")
            })
    }

    pub(super) fn poll_desktop_media(mut self: Pin<&mut Self>) {
        if matches!(self.rust().desktop_media, Registration::Pending) {
            let registration = match ffi::connect_desktop_media() {
                Ok(session) => Registration::Registered(session),
                Err(error) => {
                    tracing::warn!("Desktop media controls unavailable: {error}");
                    Registration::Unavailable
                }
            };
            self.as_mut().rust_mut().desktop_media = registration;
        }
        for _ in 0..COMMANDS_PER_TICK {
            let command = match &mut self.as_mut().rust_mut().desktop_media {
                Registration::Registered(session) => session.pin_mut().take_command().to_string(),
                Registration::Pending | Registration::Unavailable => return,
            };
            if command.is_empty() {
                break;
            }
            match serde_json::from_str::<Command>(&command) {
                Ok(command) => self.as_mut().execute_desktop_command(command),
                Err(error) => tracing::error!("Invalid native media command: {error}"),
            }
        }
        // Use the presentation's TS program at the playhead, including timeshift;
        // do not substitute the wall-clock EPG or publish server/file URLs.
        let track = self.desktop_track();
        let program: serde_json::Value =
            serde_json::from_str(&self.current_program_data().to_string()).unwrap_or_default();
        let channel = usize::try_from(*self.selected())
            .ok()
            .and_then(|i| self.rust().entries.get(i));
        let fallback = if self.recording() {
            self.recording_name().to_string()
        } else {
            channel.map_or_else(|| "NagameTV".into(), |channel| channel.name.clone())
        };
        let title = program
            .get("name")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(&fallback);
        // A moving timeshift window has no stable MPRIS track position. Expose
        // transport controls there; absolute seek/length apply only to recordings.
        let can_seek = self.recording() && self.seekable() && !track.is_empty();
        let snapshot = serde_json::json!({
            "track": track, "title": title,
            "artist": if self.recording() { "" } else { channel.map_or("", |c| c.name.as_str()) },
            "status": status(&self.rust().stream_state),
            "can_play": self.recording() || *self.selected() >= 0,
            "can_pause": self.desktop_can_pause(),
            "can_seek": can_seek,
            "seeking": self.seeking(),
            "position": if can_seek { (self.position_ms().max(0.0) * MICROSECONDS_PER_MILLISECOND) as i64 } else { 0 },
            "length": if can_seek { (self.duration_ms().max(0.0) * MICROSECONDS_PER_MILLISECOND) as i64 } else { 0 },
            "volume": if *self.audio_muted() { 0.0 } else { *self.volume_level() },
        });
        if let Registration::Registered(session) = &mut self.as_mut().rust_mut().desktop_media {
            session
                .pin_mut()
                .publish(&QString::from(snapshot.to_string()));
        }
    }

    fn execute_desktop_command(mut self: Pin<&mut Self>, command: Command) {
        match command {
            Command::Play => {
                if !self.playing() {
                    self.play();
                }
            }
            Command::Pause => {
                if (self.recording() || self.timeshift())
                    && self.media_active()
                    && !self.paused()
                    && !self.ended()
                {
                    self.pause();
                }
            }
            Command::PlayPause => {
                if self.desktop_can_pause() {
                    self.toggle_playback();
                }
            }
            Command::Stop => self.stop(),
            Command::Raise => self.desktop_raise_requested(),
            Command::Volume { value } => {
                self.as_mut().volume(value);
                self.save_settings();
            }
            Command::Seek { value, track } => {
                if self.recording() && self.seekable() && track == self.desktop_track() {
                    self.skip(value / MICROSECONDS_PER_MILLISECOND);
                }
            }
            Command::SetPosition { value, track } => {
                let milliseconds = value / MICROSECONDS_PER_MILLISECOND;
                if self.recording()
                    && self.seekable()
                    && track == self.desktop_track()
                    && milliseconds >= 0.0
                    && milliseconds <= self.duration_ms()
                {
                    self.seek_to(milliseconds);
                }
            }
        }
    }
}
