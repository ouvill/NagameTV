use serde::{Deserialize, Serialize};

const MIB: i64 = 1024 * 1024;
const MIN_MIB: u32 = 64;
const MAX_MIB: u32 = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "u32", into = "u32")]
pub struct CommentCacheLimit(u32);
impl CommentCacheLimit {
    pub fn checked(mib: i32) -> Option<Self> {
        let mib = u32::try_from(mib).ok()?;
        (MIN_MIB..=MAX_MIB).contains(&mib).then_some(Self(mib))
    }
    pub fn mib(self) -> i32 {
        self.0 as i32
    }
    pub fn bytes(self) -> i64 {
        i64::from(self.0) * MIB
    }
}
impl Default for CommentCacheLimit {
    fn default() -> Self {
        Self((viewer_comments::cache::DEFAULT_CACHE_BYTES / MIB) as u32)
    }
}
impl From<u32> for CommentCacheLimit {
    fn from(mib: u32) -> Self {
        Self(mib.clamp(MIN_MIB, MAX_MIB))
    }
}
impl From<CommentCacheLimit> for u32 {
    fn from(value: CommentCacheLimit) -> Self {
        value.0
    }
}
