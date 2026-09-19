use super::ffi;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::pin::Pin;

impl ffi::Player {
    pub fn comment_display(&self) -> QString {
        self.rust()
            .preferences
            .preferences()
            .comment_presentation
            .display()
            .as_str()
            .into()
    }
    pub fn comment_placement(&self) -> QString {
        self.rust()
            .preferences
            .preferences()
            .comment_presentation
            .placement()
            .as_str()
            .into()
    }
    // Normal builds omit the separate received-comment list, even with danmaku
    // off: JP7080382 / JP7277651 / JP7153786 / JP7852687. Estimated expiry
    // 2027-03-02; registry status unverified, no date-based reactivation.
    // This restores the display only for local evaluation; build.rs also gates
    // its QML resource. Internal history storage alone is not a list display.
    pub fn evaluation_comment_list(&self) -> bool {
        cfg!(feature = "evaluation-comment-list")
    }
    // Keep comments inside the actual picture: JP4734471, estimated expiry
    // 2026-12-11 (registry status unverified), including letter/pillarboxing.
    pub fn evaluation_wide_comments(&self) -> bool {
        cfg!(feature = "evaluation-wide-comments")
    }
    // Pairwise collision/catch-up: JP4695583 (2026-12-11), JP6526304 / JP7178462
    // (2027-03-02), all estimated expiry dates, not verified legal status.
    pub fn evaluation_collision_layout(&self) -> bool {
        cfg!(feature = "evaluation-collision-layout")
    }
    pub fn configure_comment_presentation(
        mut self: Pin<&mut Self>,
        display: QString,
        placement: QString,
    ) -> bool {
        use viewer_comments::danmaku::{DisplayMode, PlacementMode, Presentation};
        let (Some(display), Some(placement)) = (
            DisplayMode::parse(&display.to_string()),
            PlacementMode::parse(&placement.to_string()),
        ) else {
            return false;
        };
        let Some(value) = Presentation::new(display, placement) else {
            return false;
        };
        if value == self.rust().preferences.preferences().comment_presentation {
            return true;
        }
        self.as_mut()
            .rust_mut()
            .preferences
            .change(crate::settings::Change::CommentPresentation(value));
        self.as_mut().comment_display_changed();
        self.as_mut().comment_placement_changed();
        self.save_settings();
        true
    }

    pub fn commentary_position(&self) -> f64 {
        if self.seeking() || !self.media_active() {
            return -1.;
        }
        self.rust()
            .media
            .playback()
            .and_then(|playback| playback.position())
            .map_or(-1., |position| position.nseconds() as f64 / 1_000_000_000.)
    }

    pub fn configure_comment_shadow(mut self: Pin<&mut Self>, enabled: bool) {
        self.as_mut()
            .rust_mut()
            .preferences
            .change(crate::settings::Change::CommentShadow(enabled));
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
            this.preferences.change(crate::settings::Change::Danmaku {
                enabled,
                size,
                opacity,
                speed,
            });
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
            .change(crate::settings::Change::Comments(enabled));
        self.as_mut().poll_comments();
        self.save_settings();
    }

    pub fn comment_model(&self) -> *mut crate::comment_model::ffi::CommentModel {
        // C++ owns the object through UniquePtr for the complete Player lifetime.
        self.rust().comment_model.as_ref().expect("comment model") as *const _ as *mut _
    }

    pub fn comments_open(mut self: Pin<&mut Self>, opened: bool) {
        if opened {
            self.as_mut().rust_mut().comment_replay.open_cache();
            self.as_mut().poll_comments();
        }
    }
    pub fn clear_comment_cache(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().comment_replay.clear_unused();
        self.poll_comments();
    }
    pub fn comment_cache_limit_mib(&self) -> i32 {
        self.rust()
            .preferences
            .preferences()
            .comment_cache_limit_mib
            .mib()
    }
    pub fn configure_comment_cache_limit(mut self: Pin<&mut Self>, mib: i32) -> bool {
        let Some(limit) = crate::settings::CommentCacheLimit::checked(mib) else {
            return false;
        };
        self.as_mut()
            .rust_mut()
            .preferences
            .change(crate::settings::Change::CommentCacheLimit(limit));
        self.as_mut()
            .rust_mut()
            .comment_replay
            .set_budget(limit.bytes());
        self.as_mut().comment_cache_limit_mib_changed();
        self.save_settings();
        true
    }
    pub fn refresh_recording_comments(mut self: Pin<&mut Self>) {
        if self.rust().stream_state.recording().is_some() {
            self.as_mut().rust_mut().comment_replay.refresh_current();
            self.poll_comments();
        }
    }

    pub(super) fn poll_comments(mut self: Pin<&mut Self>) {
        self.as_mut().poll_activity();
        let seeking = self.seeking();
        let (reset, comments, timeline) = {
            let mut this = self.as_mut().rust_mut();
            let this = &mut *this;
            this.comment_replay.set_budget(
                this.preferences
                    .preferences()
                    .comment_cache_limit_mib
                    .bytes(),
            );
            let channel = usize::try_from(this.selected)
                .ok()
                .and_then(|index| this.entries.get(index))
                .filter(|_| this.stream_state.recording().is_none());
            let reset = this.comments.configure(this.comments_enabled, channel);
            let accepting = this.comment_replay.can_receive();
            let comments = match this.network.as_ref().filter(|_| accepting) {
                Some(network) => match this.comments.poll(network) {
                    Ok(comments) => comments,
                    Err(error) => {
                        tracing::error!("{error}");
                        Vec::new()
                    }
                },
                None => Vec::new(),
            };
            let reception = if accepting
                && comments.len() < viewer_comments::controller::MAX_POLL_COMMENTS
                && this.comments_enabled
            {
                this.comments
                    .reception_epoch()
                    .map(viewer_comments::cache::Reception::Receiving)
                    .unwrap_or(viewer_comments::cache::Reception::Interrupted)
            } else {
                viewer_comments::cache::Reception::Interrupted
            };
            let context = if this.stream_state.active() || this.stream_state.connecting() {
                this.media.source_identity().map(|source| {
                    let position = this
                        .media
                        .playback()
                        .and_then(|playback| playback.position());
                    let view = position
                        .map(|position| this.media.metadata(position))
                        .unwrap_or_default();
                    let fallback = channel.and_then(|channel| channel.broadcast);
                    let source_range = this
                        .media
                        .comment_source(fallback, crate::features::comments::channel_for, reception)
                        .unwrap_or(viewer_comments::cache::Source::Pending);
                    crate::features::comments::replay::Context {
                        source,
                        service: view
                            .service
                            .or_else(|| channel.and_then(|channel| channel.broadcast)),
                        position: position.map(|position| position.nseconds()),
                        clock: view.clock,
                        source_range,
                        enabled: this.comments_enabled,
                        display: this.danmaku_enabled,
                    }
                })
            } else {
                None
            };
            let now = std::time::Instant::now();
            let received = comments
                .iter()
                .map(|comment| {
                    (
                        comment.clone(),
                        this.comments.posting.is_own_comment(comment, now),
                    )
                })
                .collect();
            let wall_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .and_then(|time| i64::try_from(time.as_millis()).ok())
                .unwrap_or(0);
            this.comment_replay.update(
                context,
                seeking && this.comments_enabled,
                received,
                wall_ms,
            );
            let timeline = format!(
                "{{\"generation\":{},\"comments\":{}}}",
                this.comment_replay.generation,
                if this.comment_replay.data.is_empty() {
                    "[]"
                } else {
                    &this.comment_replay.data
                }
            );
            (reset, comments, QString::from(timeline))
        };
        if *self.comment_timeline() != timeline {
            self.as_mut().rust_mut().comment_timeline = timeline;
            self.as_mut().comment_timeline_changed();
        }
        let cache_bytes = self.rust().comment_replay.disk_bytes() as f64;
        if self.rust().comment_cache_bytes != cache_bytes {
            self.as_mut().rust_mut().comment_cache_bytes = cache_bytes;
            self.as_mut().comment_cache_bytes_changed();
        }
        if reset {
            self.as_mut().clear_comment_history();
            self.as_mut().set_comment_draft(QString::default());
        }
        self.as_mut().history_model().append(comments);
        let title = {
            let this = self.rust();
            let channel = usize::try_from(this.selected)
                .ok()
                .and_then(|index| this.entries.get(index))
                .filter(|_| this.stream_state.recording().is_none());
            QString::from(this.activity.program_title(channel))
        };
        self.as_mut().set_comment_program_title(title);
        self.as_mut().refresh_comment_status();
        self.as_mut().poll_comment_posting();
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
