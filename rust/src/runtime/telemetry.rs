use super::Runtime;

#[derive(Default)]
pub(super) struct UiState {
    guide: bool,
    channels: bool,
    live_comments: usize,
    flags: (bool, bool, bool),
}

impl Runtime {
    pub fn record_ui(&mut self, guide: bool, channels: bool, live_comments: i32) {
        let prefs = self.state.preferences();
        let flags = (
            self.state.playback().active(),
            prefs.subtitles_enabled,
            prefs.danmaku_enabled,
        );
        let event = if (guide, channels) != (self.ui.guide, self.ui.channels) {
            "panels_changed"
        } else if flags != self.ui.flags {
            "playback_options_changed"
        } else {
            "sample"
        };
        self.ui = UiState {
            guide,
            channels,
            live_comments: usize::try_from(live_comments).unwrap_or(0),
            flags,
        };
        self.record(event);
    }
    pub(super) fn record(&self, event: &'static str) {
        let Some(recorder) = &self.recorder else {
            return;
        };
        let state = &self.state;
        let prefs = state.preferences();
        recorder.record(
            event,
            crate::diagnostics::Snapshot {
                playing: state.playback().active(),
                subtitles: prefs.subtitles_enabled,
                comments: prefs.danmaku_enabled,
                guide_open: self.ui.guide,
                channels_open: self.ui.channels,
                live_comments: self.ui.live_comments,
                channel_count: state.catalog().channels.len(),
                epg_programs: state.epg().program_count,
                epg_text_capacity_bytes: state.epg().text_capacity_bytes,
                comment_history_count: state.comments().len(),
                comment_history_text_capacity_bytes: state
                    .comments()
                    .iter()
                    .map(|c| c.time.capacity() + c.text.capacity() + c.source.capacity())
                    .sum(),
                subtitle_cells: state.subtitle().map_or(0, |c| c.cells.len()),
                subtitle_pending: self
                    .playback
                    .as_ref()
                    .and_then(crate::playback::Playback::pending_subtitles),
                epg_loading: state.loading(),
            },
        );
    }
}
