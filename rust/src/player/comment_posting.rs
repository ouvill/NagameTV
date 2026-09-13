//! Posting and draft projection at the Qt boundary; drafts are never saved or logged.
use super::{
    ffi,
    status::{tr, with_detail},
};
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::{pin::Pin, time::Instant};
use viewer_comments::posting::{Error, Status};

impl ffi::Player {
    pub fn edit_comment_draft(mut self: Pin<&mut Self>, text: QString) {
        if !self.rust().comments.posting.busy()
            && text.to_string().len() <= viewer_comments::MAX_COMMENT_BYTES
        {
            self.as_mut().set_comment_draft(text);
        }
    }

    pub fn configure_comment_send_on_enter(mut self: Pin<&mut Self>, enabled: bool) {
        self.as_mut()
            .rust_mut()
            .preferences
            .change(crate::settings::Change::CommentSendOnEnter(enabled));
        self.as_mut().set_comment_send_on_enter(enabled);
        self.save_settings();
    }

    pub fn post_comment(mut self: Pin<&mut Self>) -> bool {
        // Selection may have changed since the last GUI poll. Invalidate old drafts
        // and jobs before reading text, never retarget an old submission.
        self.as_mut().poll_comments();
        let accepted = {
            let mut this = self.as_mut().rust_mut();
            let this = &mut *this;
            this.network.as_ref().is_some_and(|network| {
                network.post_comment(&mut this.comments.posting, &this.comment_draft.to_string())
            })
        };
        self.as_mut().refresh_comment_posting();
        accepted
    }

    pub(super) fn poll_comment_posting(mut self: Pin<&mut Self>) {
        let sent = self
            .as_mut()
            .rust_mut()
            .comments
            .posting
            .poll(Instant::now());
        if sent {
            self.as_mut().set_comment_draft(QString::default());
        }
        self.refresh_comment_posting();
    }

    pub(super) fn refresh_comment_posting(mut self: Pin<&mut Self>) {
        let (busy, available, target, status) = {
            let this = self.rust();
            let posting = &this.comments.posting;
            let text = match posting.status() {
                Status::Idle => QString::default(),
                Status::Sending => tr("Sending comment…"),
                Status::Sent => tr("Comment sent to NX-Jikkyo"),
                Status::Failed(Error::Empty) => tr("Enter a comment before sending."),
                Status::Failed(Error::TooLong) => tr("The comment is too long."),
                Status::Failed(error) => with_detail("Could not post comment: %1", error),
                Status::Unknown(_) => {
                    tr("Delivery could not be confirmed. Check the comments before sending again.")
                }
            };
            (
                posting.busy(),
                this.network.is_some() && posting.available(Instant::now()),
                this.comments
                    .jikkyo_id()
                    .map(|id| format!("NX-Jikkyo · jk{id}"))
                    .unwrap_or_default(),
                text,
            )
        };
        self.as_mut().set_comment_post_busy(busy);
        self.as_mut().set_comment_post_available(available);
        self.as_mut().set_comment_post_target(QString::from(target));
        self.set_comment_post_status(status);
    }
}
