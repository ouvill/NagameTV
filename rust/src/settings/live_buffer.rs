//! Validated receive-to-playhead reserve, stored as integer milliseconds.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "i32", into = "i32")]
pub struct LiveBuffer(i32);

impl LiveBuffer {
    // Seeking needs a target strictly before the receive edge.
    pub const MIN_MS: i32 = 1;
    pub const MAX_MS: i32 = 1000;
    pub const DEFAULT_MS: i32 = 250;

    pub fn checked(milliseconds: i32) -> Option<Self> {
        (Self::MIN_MS..=Self::MAX_MS)
            .contains(&milliseconds)
            .then_some(Self(milliseconds))
    }
    pub fn milliseconds(self) -> i32 {
        self.0
    }
}
impl Default for LiveBuffer {
    fn default() -> Self {
        Self(Self::DEFAULT_MS)
    }
}
impl TryFrom<i32> for LiveBuffer {
    type Error = &'static str;
    fn try_from(value: i32) -> Result<Self, Self::Error> {
        Self::checked(value).ok_or("live_buffer_ms is out of range")
    }
}
impl From<LiveBuffer> for i32 {
    fn from(value: LiveBuffer) -> Self {
        value.0
    }
}
