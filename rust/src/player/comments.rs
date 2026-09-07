use super::ffi;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::pin::Pin;

impl ffi::Player {
    pub fn configure_danmaku(
        mut self: Pin<&mut Self>,
        enabled: bool,
        size: f64,
        opacity: f64,
        speed: f64,
    ) -> bool {
        use crate::settings::{CommentFontSize, CommentOpacity, CommentSpeed};
        let (Some(size), Some(opacity), Some(speed)) = (
            CommentFontSize::checked(size),
            CommentOpacity::checked(opacity),
            CommentSpeed::checked(speed),
        ) else {
            return false;
        };
        {
            let mut this = self.as_mut().rust_mut();
            let prefs = this.preferences.preferences_mut();
            prefs.danmaku_enabled = enabled;
            prefs.comment_font_size = size;
            prefs.comment_opacity = opacity;
            prefs.comment_speed = speed;
        }
        self.as_mut().set_danmaku_enabled(enabled);
        self.as_mut().set_comment_font_size(size.into());
        self.as_mut().set_comment_opacity(opacity.into());
        self.as_mut().set_comment_speed(speed.into());
        true
    }

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
        self.as_mut().poll_activity();
        let (data, live) = {
            let mut this = self.as_mut().rust_mut();
            let this = &mut *this;
            let channel = usize::try_from(this.selected)
                .ok()
                .and_then(|index| this.entries.get(index));
            this.comments.configure(this.comments_enabled, channel);
            let mut live = Vec::new();
            if let Some(network) = &this.network
                && let Err(error) = this.comments.poll(network, |comment| {
                    if this.playing && this.danmaku_enabled {
                        live.push((
                            QString::from(comment.text.as_ref()),
                            QString::from(comment.style.position.as_str()),
                            comment.style.color,
                        ));
                    }
                })
            {
                tracing::error!("{error}");
            }
            let data = if this.comments_visible && this.comments.dirty {
                this.comments.dirty = false;
                Some(this.comments.json())
            } else {
                None
            };
            (data, live)
        };
        self.as_mut().refresh_comment_status();
        match data {
            Some(Ok(json)) => self.as_mut().set_comment_data(QString::from(json)),
            Some(Err(error)) => self
                .as_mut()
                .set_comment_status(QString::from(error.to_string())),
            None => {}
        }
        for (text, position, color) in live {
            self.as_mut().comment_received(text, position, color);
        }
    }
    fn poll_activity(mut self: Pin<&mut Self>) {
        let data = {
            let mut this = self.as_mut().rust_mut();
            let this = &mut *this;
            this.activity
                .configure(this.comments_enabled && !this.entries.is_empty());
            if let Some(network) = &this.network {
                this.activity.poll(network);
            }
            if this.activity.dirty {
                this.activity.dirty = false;
                Some(this.activity.json(&this.entries))
            } else {
                None
            }
        };
        match data {
            Some(Ok(json)) => self.set_activity_data(QString::from(json)),
            Some(Err(error)) => {
                tracing::error!("Comment activity projection failed: {error}");
                self.set_activity_data(QString::from("[]"));
            }
            None => {}
        }
    }
}
