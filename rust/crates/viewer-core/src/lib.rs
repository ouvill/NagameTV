//! Pure application state and transitions. Inputs include time and IO results;
//! this crate never opens sockets, reads files, or calls Qt/GStreamer.
#![forbid(unsafe_code)]

pub mod app;

pub mod audio;
pub mod channels;
pub mod comments;
pub mod epg;
pub mod settings;
pub mod subtitles;

mod selection;
