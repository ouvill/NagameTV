//! Main-compatible persisted language codes, distinct from the resolved Qt locale.
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum Language {
    #[default]
    System,
    Japanese,
    English,
}
impl Language {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "system" => Some(Self::System),
            "ja" => Some(Self::Japanese),
            "en" => Some(Self::English),
            _ => None,
        }
    }
    pub fn code(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Japanese => "ja",
            Self::English => "en",
        }
    }
}
impl From<String> for Language {
    fn from(value: String) -> Self {
        Self::parse(&value).unwrap_or(Self::English)
    }
}
impl From<Language> for String {
    fn from(value: Language) -> Self {
        value.code().into()
    }
}
