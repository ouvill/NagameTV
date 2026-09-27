//! Posting and draft projection at the Qt boundary; drafts are never saved or logged.
use super::{
    ffi,
    status::{tr, with_detail},
};
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::pin::Pin;
use viewer_comments::posting::{Error, Status};

impl ffi::Player {
    pub fn edit_comment_draft(mut self: Pin<&mut Self>, text: QString) {
        self.as_mut()
            .rust_mut()
            .commentary
            .edit_draft(text.to_string());
        self.refresh_commentary();
    }

    pub fn configure_comment_send_on_enter(mut self: Pin<&mut Self>, enabled: bool) {
        self.as_mut()
            .rust_mut()
            .preferences
            .change(crate::settings::Change::CommentSendOnEnter(enabled));
        self.as_mut().set_comment_send_on_enter(enabled);
        self.save_settings();
    }

    pub fn post_comment(self: Pin<&mut Self>) -> bool {
        self.update_commentary(crate::features::comments::session::Command::Submit)
    }
}

pub(super) fn render(status: &Status) -> QString {
    match status {
        Status::Idle => QString::default(),
        Status::Sending => tr("Sending comment…"),
        Status::Sent => tr("Comment sent to NX-Jikkyo"),
        Status::Failed(Error::Empty) => tr("Enter a comment before sending."),
        Status::Failed(Error::TooLong) => tr("The comment is too long."),
        Status::Failed(error) => with_detail("Could not post comment: %1", error),
        Status::Unknown(_) => {
            tr("Delivery could not be confirmed. Check the comments before sending again.")
        }
    }
}
