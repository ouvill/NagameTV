//! NX-Jikkyo protocol and optional bounded receiver. No Qt or playback devices.
pub mod activity;
#[cfg(feature = "network")]
pub mod connection;
#[cfg(feature = "network")]
pub mod controller;
pub mod danmaku;
#[cfg(feature = "network")]
pub mod posting;
mod protocol;
pub use protocol::{
    Comment, CommentIdentity, Decoder, Error, Event, Origin, Phase, Position, Style, ThreadId,
};
pub use protocol::{MAX_COMMENT_BYTES, MAX_MESSAGE_BYTES, MAX_THREAD_LIST_BYTES};
