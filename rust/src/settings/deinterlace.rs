//! CPU deinterlacing method applied when playback starts.
use serde::{Deserialize, Serialize};

/// GPU paths stay environment-only: they need Qt platform selection before
/// startup and fail without a matching driver, which a saved choice cannot fix.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum Deinterlace {
    Yadif,
    #[default]
    GreedyL,
    Linear,
    Off,
}
impl Deinterlace {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "yadif" => Some(Self::Yadif),
            "greedyl" => Some(Self::GreedyL),
            "linear" => Some(Self::Linear),
            "off" => Some(Self::Off),
            _ => None,
        }
    }
    pub fn code(self) -> &'static str {
        match self {
            Self::Yadif => "yadif",
            Self::GreedyL => "greedyl",
            Self::Linear => "linear",
            Self::Off => "off",
        }
    }
}
impl From<String> for Deinterlace {
    // A method from a newer version still plays with the default filter
    // instead of rejecting the whole settings file.
    fn from(value: String) -> Self {
        Self::parse(&value).unwrap_or_default()
    }
}
impl From<Deinterlace> for String {
    fn from(value: Deinterlace) -> Self {
        value.code().into()
    }
}
