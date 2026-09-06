//! Validated presentation settings. Units stay distinct at Rust boundaries.
use serde::{Deserialize, Serialize};
macro_rules! bounded_setting {
    ($name:ident, $min:literal, $max:literal, $default:literal) => {
        #[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
        #[serde(from = "f64", into = "f64")]
        pub struct $name(f64);
        impl $name {
            pub fn checked(value: f64) -> Option<Self> {
                value.is_finite().then(|| Self(value.clamp($min, $max)))
            }
        }
        impl Default for $name {
            fn default() -> Self {
                Self($default)
            }
        }
        impl From<f64> for $name {
            fn from(value: f64) -> Self {
                Self::checked(value).unwrap_or_default()
            }
        }
        impl From<$name> for f64 {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}
// Match main's persisted bounds, which are wider than its size/opacity sliders.
bounded_setting!(CommentFontSize, 12.0, 48.0, 21.0);
bounded_setting!(CommentOpacity, 0.1, 1.0, 1.0);
bounded_setting!(CommentSpeed, 0.5, 2.0, 1.0);
