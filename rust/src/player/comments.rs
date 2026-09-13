use super::ffi;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::pin::Pin;

impl ffi::Player {
    pub fn configure_comment_shadow(mut self: Pin<&mut Self>, enabled: bool) {
        self.as_mut()
            .rust_mut()
            .preferences
            .preferences_mut()
            .comment_shadow_enabled = enabled;
        self.as_mut().set_comment_shadow_enabled(enabled);
        self.save_settings();
    }

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

    pub fn comment_model(&self) -> *mut crate::comment_model::ffi::CommentModel {
        // C++ owns the object through UniquePtr for the complete Player lifetime.
        self.rust().comment_model.as_ref().expect("comment model") as *const _ as *mut _
    }

    pub fn comments_open(mut self: Pin<&mut Self>, opened: bool) {
        if opened {
            self.as_mut().poll_comments();
        }
    }

    pub(super) fn poll_comments(mut self: Pin<&mut Self>) {
        self.as_mut().poll_activity();
        let (reset, comments, show_live) = {
            let mut this = self.as_mut().rust_mut();
            let this = &mut *this;
            let channel = usize::try_from(this.selected)
                .ok()
                .and_then(|index| this.entries.get(index));
            let reset = this.comments.configure(this.comments_enabled, channel);
            let comments = match &this.network {
                Some(network) => match this.comments.poll(network) {
                    Ok(comments) => comments,
                    Err(error) => {
                        tracing::error!("{error}");
                        Vec::new()
                    }
                },
                None => Vec::new(),
            };
            (reset, comments, this.playing && this.danmaku_enabled)
        };
        // Build only newly received live signals, before moving history into its model.
        let live = project_live_comments(
            &comments,
            show_live,
            &mut self.as_mut().rust_mut().comments.posting,
        );
        if reset {
            self.as_mut().clear_comment_history();
            self.as_mut().set_comment_draft(QString::default());
        }
        self.as_mut().history_model().append(comments);
        let title = {
            let this = self.rust();
            let channel = usize::try_from(this.selected)
                .ok()
                .and_then(|index| this.entries.get(index));
            QString::from(this.activity.program_title(channel))
        };
        self.as_mut().set_comment_program_title(title);
        self.as_mut().refresh_comment_status();
        self.as_mut().poll_comment_posting();
        for (text, position, color, own) in live {
            self.as_mut().comment_received(text, position, color, own);
        }
    }
    fn history_model(self: Pin<&mut Self>) -> Pin<&mut crate::comment_model::ffi::CommentModel> {
        let model = self.comment_model();
        // UniquePtr keeps the model at a stable address for the Player lifetime.
        // End the PlayerRust borrow before emitting synchronous model signals,
        // whose QML handlers may read Player properties again.
        unsafe { Pin::new_unchecked(&mut *model) }
    }

    pub(super) fn clear_comment_history(self: Pin<&mut Self>) {
        self.history_model().clear();
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

fn project_live_comments(
    comments: &[viewer_comments::Comment],
    enabled: bool,
    posting: &mut viewer_comments::posting::Controller,
) -> Vec<(QString, QString, u32, bool)> {
    comments
        .iter()
        .filter(|c| enabled && c.phase == viewer_comments::Phase::Live)
        .map(|c| {
            (
                QString::from(c.text.as_ref()),
                QString::from(c.style.position.as_str()),
                c.style.color,
                posting.is_own_comment(c, std::time::Instant::now()),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_live_comments_are_sent_to_enabled_danmaku() {
        use viewer_comments::{Comment, Origin, Phase, Position, Style};
        let comments: Vec<_> = [Phase::History, Phase::Live]
            .into_iter()
            .map(|phase| Comment {
                identity: None,
                text: "same text".into(),
                unix_seconds: 0,
                origin: Origin::Nx,
                phase,
                style: Style {
                    position: Position::Top,
                    color: 0xff0000,
                },
            })
            .collect();
        let mut posting = viewer_comments::posting::Controller::default();
        let live = project_live_comments(&comments, true, &mut posting);
        assert_eq!(live.len(), 1);
        assert_eq!(
            live[0],
            (
                QString::from("same text"),
                QString::from("top"),
                0xff0000,
                false
            )
        );
        assert!(project_live_comments(&comments, false, &mut posting).is_empty());
    }
}
