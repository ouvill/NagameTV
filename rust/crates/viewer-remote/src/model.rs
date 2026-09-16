//! Validated commands and authoritative state, independent of protobuf and Qt.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Volume(f64);

impl Volume {
    pub fn new(fraction: f64) -> Result<Self, InvalidVolume> {
        if fraction.is_finite() && (0.0..=1.0).contains(&fraction) {
            Ok(Self(fraction))
        } else {
            Err(InvalidVolume)
        }
    }

    pub fn fraction(self) -> f64 {
        self.0
    }
}

#[derive(Debug, thiserror::Error)]
#[error("volume fraction must be finite and between 0 and 1")]
pub struct InvalidVolume;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    SelectChannel(u64),
    Play,
    Stop,
    SetVolume(Volume),
    SetMuted(bool),
    SetSubtitles(bool),
}

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("command expired before execution")]
    Expired,
    #[error("channel is not in the current catalog")]
    ChannelNotFound,
    #[error("{0}")]
    NotReady(&'static str),
    #[error("{0}")]
    Playback(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Playback {
    Stopped,
    Connecting(u64),
    Playing(u64),
    Paused(u64),
    Seeking(u64),
    SeekingPaused(u64),
    Ended(u64),
    StopFailed(u64),
    FileConnecting(String),
    FilePlaying(String),
    FilePaused(String),
    FileSeeking(String),
    FileSeekingPaused(String),
    FileEnded(String),
    FileStopFailed(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubtitleDisplay {
    Disabled,
    Hidden,
    Visible,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Band {
    Terrestrial,
    Bs,
    Cs,
    Sky,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Channel {
    pub id: u64,
    pub name: String,
    pub label: String,
    pub band: Band,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    pub id: u64,
    pub name: String,
    pub description: String,
    pub start_at_ms: u64,
    pub duration_ms: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub selected_channel_id: Option<u64>,
    pub playback: Playback,
    pub volume: Volume,
    pub muted: bool,
    pub subtitles: SubtitleDisplay,
    pub playback_error: String,
    pub settings_error: String,
    pub current_program: Option<Program>,
}
