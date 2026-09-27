use super::ffi;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::pin::Pin;

impl ffi::Player {
    pub fn comment_density(&self) -> QString {
        self.rust()
            .preferences
            .preferences()
            .comment_density
            .as_str()
            .into()
    }
    pub fn configure_comment_density(mut self: Pin<&mut Self>, density: QString) -> bool {
        let Some(value) = viewer_comments::danmaku::DensityMode::parse(&density.to_string()) else {
            return false;
        };
        if value != self.rust().preferences.preferences().comment_density {
            self.as_mut()
                .rust_mut()
                .preferences
                .change(crate::settings::Change::CommentDensity(value));
            self.as_mut().comment_density_changed();
            self.save_settings();
        }
        true
    }
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

    /// # Safety
    /// `controller` must be null or a live DanmakuController on the GUI thread.
    /// The QML call owns its lifetime for this synchronous delivery; no pointer is retained.
    pub unsafe fn sync_comment_timeline(
        &self,
        controller: *mut crate::danmaku::ffi::DanmakuController,
    ) -> bool {
        if self.seeking() || !self.media_active() {
            return false;
        }
        let Some(position) = self
            .rust()
            .media
            .playback()
            .and_then(|playback| playback.position())
        else {
            return false;
        };
        let timeline = self.rust().commentary.timeline().clone();
        // End all borrows of Player state before the controller emits any signals.
        let Some(controller) = (unsafe { controller.as_mut() }) else {
            return false;
        };
        unsafe { Pin::new_unchecked(controller) }.apply_playback_timeline(
            timeline,
            std::time::Duration::from_nanos(position.nseconds()),
        );
        true
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
            self.as_mut().rust_mut().commentary.open_cache();
            self.as_mut().poll_comments();
        }
    }
    pub fn clear_comment_cache(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().commentary.clear_unused();
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
        self.as_mut().rust_mut().commentary.set_cache_limit(limit);
        self.as_mut().comment_cache_limit_mib_changed();
        self.save_settings();
        true
    }
    pub fn refresh_recording_comments(mut self: Pin<&mut Self>) {
        if self.rust().stream_state.recording().is_some() {
            self.as_mut().rust_mut().commentary.refresh_current();
            self.poll_comments();
        }
    }

    pub(super) fn poll_comments(self: Pin<&mut Self>) {
        self.update_commentary(crate::features::comments::session::Command::Poll);
    }

    pub(super) fn update_commentary(
        mut self: Pin<&mut Self>,
        command: crate::features::comments::session::Command,
    ) -> bool {
        use crate::features::comments::session::{Input, Options, Submission};
        let update = {
            let mut this = self.as_mut().rust_mut();
            let this = &mut *this;
            let input = this
                .stream_state
                .commentary_playback(this.catalog.selected(), &this.media);
            let options = Options {
                enabled: this.comments_enabled,
                display: this.danmaku_enabled,
                cache_limit: this.preferences.preferences().comment_cache_limit_mib,
            };
            let wall_ms = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
                Ok(time) => match i64::try_from(time.as_millis()) {
                    Ok(time) => time,
                    Err(error) => {
                        tracing::error!(
                            error = &error as &dyn std::error::Error,
                            "Comment clock exceeds supported range"
                        );
                        return false;
                    }
                },
                Err(error) => {
                    tracing::error!(
                        error = &error as &dyn std::error::Error,
                        "Comment clock precedes Unix epoch"
                    );
                    return false;
                }
            };
            this.commentary.update(
                Input {
                    options,
                    playback: input,
                    channels: this.catalog.channels(),
                    network: this.network.as_ref(),
                    now: std::time::Instant::now(),
                    wall_ms,
                },
                command,
            )
        };
        let accepted = update.submission == Submission::Accepted;
        super::comment_projection::publish(self, Some(update));
        accepted
    }

    pub(super) fn refresh_commentary(self: Pin<&mut Self>) {
        super::comment_projection::publish(self, None);
    }
    pub(super) fn history_model(
        self: Pin<&mut Self>,
    ) -> Pin<&mut crate::comment_model::ffi::CommentModel> {
        let model = self.comment_model();
        // UniquePtr keeps the model at a stable address for the Player lifetime.
        // End the PlayerRust borrow before emitting synchronous model signals,
        // whose QML handlers may read Player properties again.
        unsafe { Pin::new_unchecked(&mut *model) }
    }

    pub(super) fn clear_comment_history(self: Pin<&mut Self>) {
        self.history_model().clear();
    }
}
