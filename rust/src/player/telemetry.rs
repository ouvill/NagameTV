//! Collect owned counters only; process measurement and file writes run off the GUI thread.
use super::{ffi, status::tr};
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::{
    pin::Pin,
    time::{Duration, Instant},
};
use viewer_diagnostics::{Snapshot, recorder::Event};
#[derive(Default, PartialEq, Eq)]
struct Options {
    playing: bool,
    subtitles: bool,
    comments: bool,
    epg: bool,
    danmaku: bool,
}
#[derive(Default)]
pub(super) struct UiState {
    guide: bool,
    channels: bool,
    options: Options,
    live_comments: usize,
}
impl ffi::Player {
    pub fn build_info(&self) -> QString {
        QString::from(crate::build_info::json())
    }

    pub fn record_ui_state(
        mut self: Pin<&mut Self>,
        guide: bool,
        channels: bool,
        live_comments: i32,
    ) {
        if self
            .rust()
            .diagnostic_recorder
            .as_ref()
            .is_some_and(|recorder| recorder.is_finished().unwrap_or(true))
        {
            self.as_mut().stop_diagnostics();
        }
        let mut this = self.as_mut().rust_mut();
        let options = Options {
            playing: this.stream_state.playing(),
            subtitles: this.subtitles_enabled,
            comments: this.comments_enabled,
            epg: this.epg_enabled,
            danmaku: this.danmaku_enabled,
        };
        let event = if (guide, channels) != (this.diagnostic_ui.guide, this.diagnostic_ui.channels)
        {
            Event::PanelsChanged
        } else if options != this.diagnostic_ui.options {
            Event::PlaybackOptionsChanged
        } else {
            Event::Sample
        };
        this.diagnostic_ui = UiState {
            guide,
            channels,
            options,
            live_comments: usize::try_from(live_comments).unwrap_or(0).min(64),
        };
        self.record_diagnostic(event);
    }
    pub(super) fn record_diagnostic(&self, event: Event) {
        let this = self.rust();
        let Some(recorder) = &this.diagnostic_recorder else {
            return;
        };
        let (tasks, programs, stopping) = this.epg.counters();
        let (history, text_bytes) = this.comment_model.storage();
        // Only queried while diagnostics are enabled; no frame probes or history.
        let video = this
            .media
            .playback()
            .and_then(|playback| playback.video_frame_counters());
        let warnings = this.media.playback().map(|p| p.warning_counts());
        let snapshot = Snapshot {
            playing: this.stream_state.playing(),
            video_rendered: video.as_ref().map(|counters| counters.rendered),
            video_dropped: video.as_ref().map(|counters| counters.dropped),
            gst_warning_count: warnings.map(|counts| counts.total),
            ts_continuity_warning_count: warnings.map(|counts| counts.continuity),
            last_ts_continuity_pid: warnings.and_then(|counts| counts.last_continuity_pid),
            subtitles: this.subtitles_enabled,
            comments: this.danmaku_enabled,
            comments_enabled: this.comments_enabled,
            epg_enabled: this.epg_enabled,
            guide_open: this.diagnostic_ui.guide,
            channels_open: this.diagnostic_ui.channels,
            live_comments: this.diagnostic_ui.live_comments,
            channel_count: this.catalog.channels().len(),
            epg_programs: programs,
            epg_text_capacity_bytes: this.epg.text_capacity_bytes,
            comment_history_count: history,
            comment_history_text_capacity_bytes: text_bytes,
            subtitle_cells: this.subtitle_cells,
            subtitle_pending: this
                .media
                .subtitles()
                .and_then(|session| session.pending_diagnostic()),
            epg_loading: tasks > 0 && !stopping,
        };
        let _ = recorder.record(event, snapshot);
    }
    pub(super) fn stop_diagnostics(mut self: Pin<&mut Self>) {
        let recorder = self.as_mut().rust_mut().diagnostic_recorder.take();
        if let Some(recorder) = recorder
            && let Err(error) = recorder.finish()
        {
            tracing::error!("{error}");
            self.set_log_error(QString::from(error.to_string()));
        }
    }
}

/// Last sampled counters only; language changes never query feature workers.
#[derive(Default)]
pub(super) struct FeatureMetrics {
    subscriptions: usize,
    pending: usize,
    decoded: u64,
    tasks: usize,
    programs: usize,
    stopping: bool,
}

impl FeatureMetrics {
    fn display(&self) -> QString {
        tr("Subtitles: subscriptions %1, pending %2, received %3 | EPG: tasks %4, programs %5, stopping %6")
            .arg(&QString::from(self.subscriptions.to_string()))
            .arg(&QString::from(self.pending.to_string()))
            .arg(&QString::from(self.decoded.to_string()))
            .arg(&QString::from(self.tasks.to_string()))
            .arg(&QString::from(self.programs.to_string()))
            .arg(&tr(if self.stopping { "Yes" } else { "No" }))
    }
}

impl ffi::Player {
    pub(super) fn refresh_metric_text(mut self: Pin<&mut Self>) {
        let text = self
            .rust()
            .feature_metrics
            .as_ref()
            .map(FeatureMetrics::display)
            .unwrap_or_default();
        self.as_mut().set_diagnostics(text);
    }

    pub(super) fn poll_feature_metrics(mut self: Pin<&mut Self>) {
        if Instant::now() >= self.rust().next_diagnostic {
            let (subscriptions, pending, decoded) = self
                .rust()
                .media
                .subtitles()
                .map(|s| s.counters())
                .unwrap_or_default();
            let (tasks, programs, stopping) = self.rust().epg.counters();
            // Preserve the diagnostic log format independently of UI language.
            tracing::debug!(
                "METRICS 字幕: 購読 {subscriptions}, 待機 {pending}, 受信 {decoded} | EPG: タスク {tasks}, 番組 {programs}, 停止待ち {stopping}"
            );
            self.as_mut().rust_mut().feature_metrics = Some(FeatureMetrics {
                subscriptions,
                pending,
                decoded,
                tasks,
                programs,
                stopping,
            });
            self.as_mut().refresh_metric_text();
            self.as_mut().rust_mut().next_diagnostic = Instant::now() + Duration::from_secs(10);
        }
    }
}
