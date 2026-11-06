//! Who draws the window title bar and its move/resize/close controls.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum WindowDecoration {
    /// The frameless window draws its own buttons, drag areas and resize edges.
    #[default]
    Custom,
    /// The desktop environment or window manager provides the title bar.
    System,
}
impl WindowDecoration {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "custom" => Some(Self::Custom),
            "system" => Some(Self::System),
            _ => None,
        }
    }
    pub fn code(self) -> &'static str {
        match self {
            Self::Custom => "custom",
            Self::System => "system",
        }
    }
}
impl From<String> for WindowDecoration {
    // A value from a newer version keeps the window usable with the built-in
    // controls instead of rejecting the whole settings file.
    fn from(value: String) -> Self {
        Self::parse(&value).unwrap_or_default()
    }
}
impl From<WindowDecoration> for String {
    fn from(value: WindowDecoration) -> Self {
        value.code().into()
    }
}
