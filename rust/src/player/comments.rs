use super::ffi;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::pin::Pin;

impl ffi::Player {
    pub fn enable_comments(mut self: Pin<&mut Self>, enabled: bool) {
        let enabled = enabled && self.rust().comments_allowed;
        self.as_mut().set_comments_enabled(enabled);
        self.as_mut()
            .rust_mut()
            .preferences
            .preferences_mut()
            .comments_enabled = enabled;
        self.as_mut().poll_comments();
        self.save_settings();
    }

    pub fn comments_open(mut self: Pin<&mut Self>, opened: bool) {
        self.as_mut().rust_mut().comments_visible = opened;
        self.as_mut().rust_mut().comments.dirty = true;
        if opened {
            self.poll_comments();
        } else {
            self.set_comment_data(QString::from("[]"));
        }
    }

    pub(super) fn poll_comments(mut self: Pin<&mut Self>) {
        let (status, data) = {
            let mut this = self.as_mut().rust_mut();
            let this = &mut *this;
            let channel = usize::try_from(this.selected)
                .ok()
                .and_then(|index| this.entries.get(index));
            this.comments.configure(this.comments_enabled, channel);
            if let Some(network) = &this.network {
                this.comments.poll(network);
            }
            let status = this.comments.status(this.comments_enabled);
            let data = if this.comments_visible && this.comments.dirty {
                this.comments.dirty = false;
                Some(this.comments.json())
            } else {
                None
            };
            (status, data)
        };
        self.as_mut().set_comment_status(QString::from(status));
        match data {
            Some(Ok(json)) => self.set_comment_data(QString::from(json)),
            Some(Err(error)) => self.set_comment_status(QString::from(error.to_string())),
            None => {}
        }
    }
}
