//! The high-DPI scale chosen for the next launch.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum UiScale {
    #[default]
    Standard,
    Percent125,
    Percent150,
    Percent200,
}

impl UiScale {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "100" => Some(Self::Standard),
            "125" => Some(Self::Percent125),
            "150" => Some(Self::Percent150),
            "200" => Some(Self::Percent200),
            _ => None,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::Standard => "100",
            Self::Percent125 => "125",
            Self::Percent150 => "150",
            Self::Percent200 => "200",
        }
    }

    pub fn factor(self) -> f64 {
        match self {
            Self::Standard => 1.0,
            Self::Percent125 => 1.25,
            Self::Percent150 => 1.5,
            Self::Percent200 => 2.0,
        }
    }
}

impl From<String> for UiScale {
    // Keep the rest of the settings when a newer version has saved a scale we do not recognize.
    fn from(value: String) -> Self {
        Self::parse(&value).unwrap_or_default()
    }
}

impl From<UiScale> for String {
    fn from(value: UiScale) -> Self {
        value.code().into()
    }
}
