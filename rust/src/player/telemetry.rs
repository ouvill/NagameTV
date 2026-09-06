//! Collect owned counters only; process measurement and file writes run off the GUI thread.
use super::ffi;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::pin::Pin;
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
            .is_some_and(|recorder| recorder.is_finished())
        {
            self.as_mut().stop_diagnostics();
        }
        let mut this = self.as_mut().rust_mut();
        let options = Options {
            playing: this.playing,
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
        let (history, text_bytes) = this.comments.storage();
        let snapshot = Snapshot {
            playing: this.playing,
            subtitles: this.subtitles_enabled,
            comments: this.danmaku_enabled,
            comments_enabled: this.comments_enabled,
            epg_enabled: this.epg_enabled,
            guide_open: this.diagnostic_ui.guide,
            channels_open: this.diagnostic_ui.channels,
            live_comments: this.diagnostic_ui.live_comments,
            channel_count: this.entries.len(),
            epg_programs: programs,
            epg_text_capacity_bytes: this.epg.text_capacity_bytes,
            comment_history_count: history,
            comment_history_text_capacity_bytes: text_bytes,
            subtitle_cells: this.subtitle_cells,
            subtitle_pending: this
                .subtitle_session
                .as_ref()
                .and_then(|session| session.pending_diagnostic()),
            epg_loading: tasks > 0 && !stopping,
        };
        let _ = recorder.record(event, snapshot);
    }
    pub(super) fn stop_diagnostics(mut self: Pin<&mut Self>) {
        let recorder = self.as_mut().rust_mut().diagnostic_recorder.take();
        if let Some(recorder) = recorder
            && let Err(error) = recorder.stop().join()
        {
            eprintln!("{error}");
            self.set_log_error(QString::from(error.to_string()));
        }
    }
}
