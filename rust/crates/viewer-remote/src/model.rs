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
    SetPaused(bool),
    SeekRelative(SeekDelta),
    SeekTo(SeekPosition),
    ReturnToLive,
    SetPlaybackRate(PlaybackRate),
    StepChannel(Step),
    PreviousChannel,
    SelectRemoteKey(RemoteNumber, Option<Band>),
    SendKey(RemoteKey),
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
    pub current_program: Option<Program>,
    pub remote_key: Option<u32>,
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
    pub transport: Transport,
}

/// Integers exactly representable by the desktop millisecond API.
pub const MAX_SEEK_MS: i64 = (1_i64 << 53) - 1;

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct InvalidCommand(pub &'static str);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeekDelta(i64);
impl SeekDelta {
    pub fn new(value: i64) -> Result<Self, InvalidCommand> {
        (-MAX_SEEK_MS..=MAX_SEEK_MS)
            .contains(&value)
            .then_some(Self(value))
            .ok_or(InvalidCommand("delta_ms exceeds the supported range"))
    }
    pub fn milliseconds(self) -> f64 {
        self.0 as f64
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeekPosition(u64);
impl SeekPosition {
    pub fn new(value: u64) -> Result<Self, InvalidCommand> {
        (value <= MAX_SEEK_MS as u64)
            .then_some(Self(value))
            .ok_or(InvalidCommand("position_ms exceeds the supported range"))
    }
    pub fn milliseconds(self) -> f64 {
        self.0 as f64
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlaybackRate(i32);
impl PlaybackRate {
    pub fn new(value: i32) -> Result<Self, InvalidCommand> {
        (5..=20)
            .contains(&value)
            .then_some(Self(value))
            .ok_or(InvalidCommand("tenths must be between 5 and 20"))
    }
    pub fn tenths(self) -> i32 {
        self.0
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Previous,
    Next,
}
impl Step {
    pub fn new(value: i32) -> Result<Self, InvalidCommand> {
        match value {
            -1 => Ok(Self::Previous),
            1 => Ok(Self::Next),
            _ => Err(InvalidCommand("offset must be -1 or 1")),
        }
    }
    pub fn offset(self) -> i32 {
        match self {
            Self::Previous => -1,
            Self::Next => 1,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RemoteNumber(u8);
impl RemoteNumber {
    pub fn new(value: u32) -> Result<Self, InvalidCommand> {
        (1..=12)
            .contains(&value)
            .then_some(Self(value as u8))
            .ok_or(InvalidCommand("number must be between 1 and 12"))
    }
    pub fn number(self) -> i32 {
        self.0.into()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum RemoteKey {
    Up = 1,
    Down,
    Left,
    Right,
    Ok,
    Back,
    Guide,
    Channels,
    Fullscreen,
}
impl RemoteKey {
    pub fn new(value: i32) -> Result<Self, InvalidCommand> {
        match value {
            1 => Ok(Self::Up),
            2 => Ok(Self::Down),
            3 => Ok(Self::Left),
            4 => Ok(Self::Right),
            5 => Ok(Self::Ok),
            6 => Ok(Self::Back),
            7 => Ok(Self::Guide),
            8 => Ok(Self::Channels),
            9 => Ok(Self::Fullscreen),
            _ => Err(InvalidCommand("key is unspecified or unknown")),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transport {
    pub position_ms: Option<u64>,
    pub duration_ms: Option<u64>,
    pub seek_range: Option<(u64, u64)>,
    pub playback_rate_tenths: i32,
    pub can_pause: bool,
    pub can_seek: bool,
    pub can_return_to_live: bool,
    pub can_set_rate: bool,
    pub at_live_edge: bool,
    pub can_step_channel: bool,
    pub can_previous_channel: bool,
}
impl Default for Transport {
    fn default() -> Self {
        Self {
            position_ms: None,
            duration_ms: None,
            seek_range: None,
            playback_rate_tenths: 10,
            can_pause: false,
            can_seek: false,
            can_return_to_live: false,
            can_set_rate: false,
            at_live_edge: false,
            can_step_channel: false,
            can_previous_channel: false,
        }
    }
}
