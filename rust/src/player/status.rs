//! Translate feature state at the Qt boundary, without performing feature work.
use super::ffi;
use crate::features::{comments::PresentationStatus, program_info::Status as ProgramStatus};
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::pin::Pin;

pub(super) fn tr(source: &'static str) -> QString {
    ffi::translate_backend(&QString::from(source))
}

pub(super) fn with_detail(source: &'static str, detail: impl std::fmt::Display) -> QString {
    // Translate the template first; diagnostics are data, never translation keys.
    tr(source).arg(&QString::from(detail.to_string()))
}

impl ffi::Player {
    pub(super) fn refresh_comment_status(mut self: Pin<&mut Self>) {
        if self.media_active() && *self.comments_enabled() {
            use crate::features::comments::replay::Status;
            let text = match self.rust().comment_replay.status {
                Status::Disabled => tr("Disabled"),
                Status::WaitingService => tr("Identifying the broadcast service…"),
                Status::WaitingClock => tr("Waiting for broadcast time…"),
                Status::Unsupported => tr("Comments are unavailable for this channel"),
                Status::Seeking => tr("Seeking comments…"),
                Status::Loading | Status::Pending => tr("Fetching past comments…"),
                Status::Ready => tr("Comments synchronized to playback"),
                Status::Empty => tr("No archived comments in this interval"),
                Status::Failed => tr("Could not fetch past comments. Retrying…"),
            };
            self.set_comment_status(text);
            return;
        }
        let text = match self.rust().comments.status(self.rust().comments_enabled) {
            PresentationStatus::Disabled => tr("Disabled"),
            PresentationStatus::WaitingForChannel => {
                tr("Select a channel to receive live comments.")
            }
            PresentationStatus::Unavailable => tr("Comments are unavailable for this channel"),
            PresentationStatus::Connecting => tr("Connecting to comments…"),
            PresentationStatus::Receiving => tr("Receiving comments"),
            PresentationStatus::Failed(error) => with_detail("Comment reception failed: %1", error),
            PresentationStatus::Retrying(Some(error)) => {
                with_detail("Waiting to reconnect: %1", error)
            }
            PresentationStatus::Retrying(None) => tr("Waiting to reconnect"),
        };
        self.as_mut().set_comment_status(text);
    }

    pub(super) fn refresh_epg_status(mut self: Pin<&mut Self>) {
        if let Some(error) = &self.rust().guide_error {
            let text = with_detail("Could not display the program guide: %1", error);
            self.set_epg_status(text);
            return;
        }
        let text = match self.rust().epg.status() {
            ProgramStatus::Disabled => tr("Disabled"),
            ProgramStatus::Waiting => tr("Waiting to fetch"),
            ProgramStatus::Fetching => tr("Fetching"),
            ProgramStatus::Cancelling => tr("Stopping"),
            ProgramStatus::Ready(count) => with_detail("Programs: %1", count),
            ProgramStatus::Failed(error) => with_detail("Fetch failed: %1", error),
        };
        self.as_mut().set_epg_status(text);
    }
}
