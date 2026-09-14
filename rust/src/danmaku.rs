//! Typed Qt adapter. The engine owns placement, playback cursor and lifetime.
// CXX-Qt adds receiver/closure parameters to these scalar, typed Qt signals.
// Keep the boundary typed rather than packing rendering commands into JSON.
#![allow(clippy::too_many_arguments)]
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::{pin::Pin, time::Instant};
use viewer_comments::Position;
use viewer_comments::danmaku::{self as danmaku_core, ConfigurationChange, Engine, Viewport};

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }
    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(i32, active_count, READ, NOTIFY)]
        #[qproperty(i32, lane_count, READ, NOTIFY)]
        #[qproperty(i32, timeline_count, READ, NOTIFY)]
        #[qproperty(f64, flow_origin, READ, NOTIFY)]
        #[qproperty(f64, top_origin, READ, NOTIFY)]
        #[qproperty(f64, bottom_origin, READ, NOTIFY)]
        #[qproperty(bool, paused, READ, NOTIFY)]
        #[qproperty(QString, error, READ, NOTIFY)]
        type DanmakuController = super::Controller;
        #[qinvokable]
        fn configure(
            self: Pin<&mut DanmakuController>,
            width: f64,
            height: f64,
            text_height: f64,
            font_size: f64,
            title_overlaps: bool,
            title_bottom: f64,
            controls_overlaps: bool,
            controls_top: f64,
            full_screen: bool,
            speed: f64,
        ) -> bool;
        #[qinvokable]
        fn receive(
            self: Pin<&mut DanmakuController>,
            text: QString,
            position: QString,
            color: u32,
            own: bool,
        ) -> bool;
        #[qinvokable]
        fn measured(self: Pin<&mut DanmakuController>, token: u32, width: f64);
        #[qinvokable]
        fn remeasured(self: Pin<&mut DanmakuController>, round: u32, token: u32, width: f64);
        #[qinvokable]
        fn tick(self: Pin<&mut DanmakuController>);
        #[qinvokable]
        fn clear(self: Pin<&mut DanmakuController>);
        #[qinvokable]
        fn set_visible(self: Pin<&mut DanmakuController>, visible: bool);
        #[qinvokable]
        fn reset(self: Pin<&mut DanmakuController>);
        #[qinvokable]
        fn set_paused(self: Pin<&mut DanmakuController>, paused: bool);
        #[qinvokable]
        fn load_timeline(self: Pin<&mut DanmakuController>, json: QString) -> bool;
        #[qinvokable]
        fn advance(self: Pin<&mut DanmakuController>, seconds: f64) -> bool;
        #[qinvokable]
        fn seek(self: Pin<&mut DanmakuController>, seconds: f64) -> bool;
        #[qsignal]
        fn measure_requested(
            self: Pin<&mut DanmakuController>,
            token: u32,
            text: QString,
            own: bool,
        );
        #[qsignal]
        fn spawned(
            self: Pin<&mut DanmakuController>,
            token: u32,
            kind: i32,
            text: QString,
            color: QString,
            width: f64,
            from_x: f64,
            to_x: f64,
            y: f64,
            duration: i32,
            own: bool,
        );
        #[qsignal]
        fn removed(self: Pin<&mut DanmakuController>, token: u32);
        #[qsignal]
        fn remeasure_requested(self: Pin<&mut DanmakuController>, round: u32, token: u32);
        #[qsignal]
        fn relaid_out(
            self: Pin<&mut DanmakuController>,
            token: u32,
            width: f64,
            from_x: f64,
            to_x: f64,
            y: f64,
            duration: i32,
        );
        #[qsignal]
        fn cleared(self: Pin<&mut DanmakuController>);
    }
}

pub struct Controller {
    engine: Engine,
    last_tick: Instant,
    active_count: i32,
    lane_count: i32,
    timeline_count: i32,
    flow_origin: f64,
    top_origin: f64,
    bottom_origin: f64,
    paused: bool,
    error: QString,
}
impl Default for Controller {
    fn default() -> Self {
        Self {
            engine: Engine::default(),
            last_tick: Instant::now(),
            active_count: 0,
            lane_count: 0,
            timeline_count: 0,
            flow_origin: 0.,
            top_origin: 0.,
            bottom_origin: 0.,
            paused: false,
            error: QString::default(),
        }
    }
}
impl ffi::DanmakuController {
    fn publish(mut self: Pin<&mut Self>) {
        let active = i32::try_from(self.rust().engine.active_count()).unwrap_or(i32::MAX);
        let lanes = i32::try_from(self.rust().engine.lane_count()).unwrap_or(i32::MAX);
        let timeline = i32::try_from(self.rust().engine.timeline_count()).unwrap_or(i32::MAX);
        let flow = self.rust().engine.origin(Position::Right);
        let top = self.rust().engine.origin(Position::Top);
        let bottom = self.rust().engine.origin(Position::Bottom);
        let changes = {
            let mut this = self.as_mut().rust_mut();
            let changes = [
                this.active_count != active,
                this.lane_count != lanes,
                this.timeline_count != timeline,
                this.flow_origin != flow,
                this.top_origin != top,
                this.bottom_origin != bottom,
            ];
            this.active_count = active;
            this.lane_count = lanes;
            this.timeline_count = timeline;
            this.flow_origin = flow;
            this.top_origin = top;
            this.bottom_origin = bottom;
            changes
        };
        // Synchronous QML readers see the complete new layout at every notify.
        if changes[0] {
            self.as_mut().active_count_changed();
        }
        if changes[1] {
            self.as_mut().lane_count_changed();
        }
        if changes[2] {
            self.as_mut().timeline_count_changed();
        }
        if changes[3] {
            self.as_mut().flow_origin_changed();
        }
        if changes[4] {
            self.as_mut().top_origin_changed();
        }
        if changes[5] {
            self.as_mut().bottom_origin_changed();
        }
    }

    pub fn tick(mut self: Pin<&mut Self>) {
        let expired = {
            let mut this = self.as_mut().rust_mut();
            let now = Instant::now();
            let elapsed = now.saturating_duration_since(this.last_tick);
            this.last_tick = now;
            this.engine.advance_wall(elapsed)
        };
        // No Rust borrow survives a signal: QML may synchronously re-enter us.
        for token in expired {
            self.as_mut().removed(token.value());
        }
        self.publish();
    }
    pub fn configure(
        mut self: Pin<&mut Self>,
        width: f64,
        height: f64,
        text_height: f64,
        font_size: f64,
        title_overlaps: bool,
        title_bottom: f64,
        controls_overlaps: bool,
        controls_top: f64,
        full_screen: bool,
        speed: f64,
    ) -> bool {
        self.as_mut().tick();
        let changed = self.as_mut().rust_mut().engine.configure(
            Viewport {
                width,
                height,
                text_height,
                font_size,
                title_overlaps,
                title_bottom,
                controls_overlaps,
                controls_top,
                full_screen,
            },
            speed,
        );
        let accepted = changed.is_some();
        match changed {
            Some(ConfigurationChange::Remeasure { round, comments }) => {
                for token in comments {
                    self.as_mut()
                        .remeasure_requested(round.value(), token.value());
                }
            }
            Some(ConfigurationChange::Preserved) | None => {}
        }
        self.publish();
        accepted
    }
    pub fn remeasured(mut self: Pin<&mut Self>, round: u32, token: u32, width: f64) {
        let updates = self
            .as_mut()
            .rust_mut()
            .engine
            .remeasured(round, token, width);
        self.as_mut().publish();
        for update in updates {
            self.as_mut().relaid_out(
                update.id.value(),
                update.width,
                update.from_x,
                update.to_x,
                update.y,
                update.remaining.as_millis() as i32,
            );
        }
    }
    pub fn receive(
        mut self: Pin<&mut Self>,
        text: QString,
        position: QString,
        color: u32,
        own: bool,
    ) -> bool {
        self.as_mut().tick();
        let Some(position) = danmaku_core::position(&position.to_string()) else {
            return false;
        };
        let Some(mut comment) = danmaku_core::Comment::new(&text.to_string(), position, color)
        else {
            return false;
        };
        comment.own = own;
        self.request(comment)
    }
    fn request(mut self: Pin<&mut Self>, comment: danmaku_core::Comment) -> bool {
        let own = comment.own;
        let request = self.as_mut().rust_mut().engine.prepare(comment);
        if let Some(request) = request {
            self.as_mut().measure_requested(
                request.id.value(),
                QString::from(request.text.as_ref()),
                own,
            );
            true
        } else {
            false
        }
    }
    pub fn measured(mut self: Pin<&mut Self>, token: u32, width: f64) {
        self.as_mut().tick();
        let spawn = self.as_mut().rust_mut().engine.measured(token, width);
        if let Some(spawn) = spawn {
            self.as_mut().spawned(
                spawn.id.value(),
                match spawn.comment.position {
                    Position::Right => 0,
                    Position::Top => 1,
                    Position::Bottom => 2,
                },
                QString::from(spawn.comment.text.as_ref()),
                QString::from(format!("#{:06x}", spawn.comment.color.rgb())),
                spawn.width,
                spawn.from_x,
                spawn.to_x,
                spawn.y,
                spawn.lifetime.as_millis() as i32,
                spawn.comment.own,
            );
        }
        self.publish();
    }
    pub fn set_visible(mut self: Pin<&mut Self>, visible: bool) {
        self.as_mut().rust_mut().engine.set_visible(visible);
        if !visible {
            self.as_mut().cleared();
        }
        self.publish();
    }
    pub fn clear(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().engine.clear();
        self.as_mut().cleared();
        self.publish();
    }
    pub fn reset(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().engine.reset();
        self.as_mut().cleared();
        self.publish();
    }
    pub fn set_paused(mut self: Pin<&mut Self>, paused: bool) {
        self.as_mut().tick();
        if self.rust().paused == paused {
            return;
        }
        {
            let mut this = self.as_mut().rust_mut();
            this.engine.set_paused(paused);
            this.paused = paused;
        }
        self.as_mut().paused_changed();
        if !paused {
            self.drain_due();
        }
    }
    pub fn load_timeline(mut self: Pin<&mut Self>, json: QString) -> bool {
        match danmaku_core::parse_timeline(&json.to_string()) {
            Ok(records) => {
                self.as_mut().rust_mut().engine.load(records);
                self.as_mut().rust_mut().error = QString::default();
                self.as_mut().error_changed();
                self.as_mut().cleared();
                self.publish();
                true
            }
            Err(error) => {
                self.as_mut().rust_mut().error = QString::from(error.to_string());
                self.as_mut().error_changed();
                false
            }
        }
    }
    pub fn advance(mut self: Pin<&mut Self>, seconds: f64) -> bool {
        let Some(position) = danmaku_core::seconds(seconds) else {
            return false;
        };
        self.as_mut().tick();
        let backwards = self.as_mut().rust_mut().engine.set_position(position);
        if backwards {
            self.as_mut().cleared();
        }
        self.as_mut().drain_due();
        self.publish();
        true
    }
    fn drain_due(mut self: Pin<&mut Self>) {
        loop {
            let comment = self.as_mut().rust_mut().engine.next_due();
            let Some(comment) = comment else {
                break;
            };
            self.as_mut().request(comment);
        }
    }
    pub fn seek(mut self: Pin<&mut Self>, seconds: f64) -> bool {
        let Some(position) = danmaku_core::seconds(seconds) else {
            return false;
        };
        self.as_mut().rust_mut().engine.seek(position);
        self.as_mut().cleared();
        self.publish();
        true
    }
}
