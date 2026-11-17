//! How the main window appears when NagameTV starts.
use serde::{Deserialize, Serialize};

/// The `--fullscreen` or `--kiosk` choice for this process. Set once before
/// QML loads; the Player reads it like the feature plan.
pub static OVERRIDE: std::sync::OnceLock<StartupDisplay> = std::sync::OnceLock::new();

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum StartupDisplay {
    #[default]
    Window,
    Fullscreen,
    /// Fullscreen that cannot be left, without window controls, for a
    /// dedicated TV device. Settings provide the only way to quit.
    Kiosk,
}
impl StartupDisplay {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "window" => Some(Self::Window),
            "fullscreen" => Some(Self::Fullscreen),
            "kiosk" => Some(Self::Kiosk),
            _ => None,
        }
    }
    pub fn code(self) -> &'static str {
        match self {
            Self::Window => "window",
            Self::Fullscreen => "fullscreen",
            Self::Kiosk => "kiosk",
        }
    }
}
impl From<String> for StartupDisplay {
    // A value from a newer version opens an ordinary window, which can
    // always be closed, instead of rejecting the whole settings file.
    fn from(value: String) -> Self {
        Self::parse(&value).unwrap_or_default()
    }
}
impl From<StartupDisplay> for String {
    fn from(value: StartupDisplay) -> Self {
        value.code().into()
    }
}
