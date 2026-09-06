//! NX-Jikkyo wire protocol. No Qt, devices, network tasks or retained chat history.
mod protocol;
pub use protocol::{Comment, Decoder, Error, Event, Origin, Phase, ThreadId};
pub use protocol::{MAX_COMMENT_BYTES, MAX_MESSAGE_BYTES, MAX_THREAD_LIST_BYTES};
