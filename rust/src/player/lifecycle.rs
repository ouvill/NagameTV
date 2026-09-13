//! Presentation state for connection/playback; language changes perform no I/O.
use super::{
    ffi,
    status::{tr, with_detail},
};
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::pin::Pin;

pub(super) enum Failure {
    Server,
    Network,
    ChannelPresentation,
    ChannelFetch,
}

pub(super) enum Status {
    Connect,
    Loading,
    Select,
    Ready,
    Empty,
    Connecting(String),
    Playing(String),
    Stopped,
    Reconnecting,
    NetworkUnavailable,
    Failure(Failure, String),
    PlaybackFailed(crate::playback::failure::Hint),
}
impl Status {
    pub(super) fn render(&self) -> QString {
        match self {
            Self::Connect => tr("Connect to a server"),
            Self::Loading => tr("Loading channels..."),
            Self::Select => tr("Select a channel"),
            Self::Ready => tr("Ready"),
            Self::Empty => tr("No available channels were found"),
            Self::Connecting(name) => with_detail("Connecting: %1", name),
            Self::Playing(name) => with_detail("Playing: %1", name),
            Self::Stopped => tr("Stopped"),
            Self::Reconnecting => tr("The stream connection was interrupted. Reconnecting…"),
            Self::NetworkUnavailable => tr("Network runtime is unavailable"),
            Self::PlaybackFailed(hint) => tr(hint.source()),
            Self::Failure(kind, detail) => with_detail(
                match kind {
                    Failure::Server => "Invalid server settings: %1",
                    Failure::Network => "Network initialization failed: %1",
                    Failure::ChannelPresentation => "Could not prepare the channel display: %1",
                    Failure::ChannelFetch => "Could not load channels: %1",
                },
                detail,
            ),
        }
    }
}
impl ffi::Player {
    pub(super) fn update_status(mut self: Pin<&mut Self>, status: Status) {
        self.as_mut().rust_mut().lifecycle_status = status;
        self.refresh_status();
    }
    pub(super) fn refresh_status(mut self: Pin<&mut Self>) {
        let text = self.rust().lifecycle_status.render();
        self.as_mut().set_status(text);
    }
    pub(super) fn status_error(self: Pin<&mut Self>, kind: Failure, error: impl std::fmt::Display) {
        self.update_status(Status::Failure(kind, error.to_string()));
    }
}
