use super::{ffi, playback_control::PlayerError};
use crate::playback::Playback;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::pin::Pin;

impl ffi::Player {
    pub fn record_ui_state(
        mut self: Pin<&mut Self>,
        guide: bool,
        channels: bool,
        live_comments: i32,
    ) {
        let flags = PlaybackFlags {
            playing: *self.as_ref().playing(),
            subtitles: *self.as_ref().subtitles_enabled(),
            comments: *self.as_ref().danmaku_enabled(),
        };
        let previous = self.as_ref().rust().diagnostic_ui;
        let event = if (previous.guide_open, previous.channels_open) != (guide, channels) {
            "panels_changed"
        } else if flags != self.as_ref().rust().diagnostic_flags {
            "playback_options_changed"
        } else {
            "sample"
        };
        self.as_mut().rust_mut().diagnostic_ui = UiState {
            guide_open: guide,
            channels_open: channels,
            live_comments: usize::try_from(live_comments).unwrap_or(0),
        };
        self.as_mut().rust_mut().diagnostic_flags = flags;
        self.as_ref().record_diagnostics(event);
    }

    pub(super) fn record_diagnostics(&self, event: &'static str) {
        let rust = self.rust();
        let Some(recorder) = rust.diagnostics.as_ref() else {
            return;
        };
        let epg = rust
            .network
            .as_ref()
            .map(|network| network.epg().snapshot());
        recorder.record(
            event,
            crate::diagnostics::Snapshot {
                playing: rust.playing,
                subtitles: rust.subtitles_enabled,
                comments: rust.danmaku_enabled,
                guide_open: rust.diagnostic_ui.guide_open,
                channels_open: rust.diagnostic_ui.channels_open,
                live_comments: rust.diagnostic_ui.live_comments,
                channel_count: rust.service_ids.len(),
                epg_programs: epg.as_ref().map_or(0, |epg| epg.program_count),
                epg_text_capacity_bytes: epg.as_ref().map_or(0, |epg| epg.text_capacity_bytes),
                comment_history_count: rust.comments.len(),
                comment_history_text_capacity_bytes: rust
                    .comments
                    .iter()
                    .map(|entry| {
                        entry.time.capacity() + entry.text.capacity() + entry.source.capacity()
                    })
                    .sum(),
                subtitle_cells: rust.subtitle_cue.as_ref().map_or(0, |cue| cue.cells.len()),
                subtitle_pending: rust.playback.as_ref().and_then(Playback::pending_subtitles),
                epg_loading: rust.catalog_request.is_loading(),
            },
        );
    }

    pub fn video_stats(&self) -> QString {
        let Some(playback) = self.rust().playback.as_ref() else {
            return QString::from("{}");
        };
        match serde_json::to_string(&playback.video_stats()) {
            Ok(stats) => QString::from(stats),
            Err(error) => {
                tracing::warn!(%error, "Could not serialize video statistics");
                QString::from("{}")
            }
        }
    }

    pub fn open_log_folder(self: Pin<&mut Self>) -> bool {
        match prepare_log_directory() {
            Ok(path) => {
                let opened = ffi::open_playback_log_directory(&QString::from(
                    path.to_string_lossy().as_ref(),
                ));
                if !opened {
                    tracing::warn!(path = %path.display(), "Could not open log folder");
                }
                opened
            }
            Err(error) => {
                tracing::warn!(%error, "Could not prepare log folder");
                false
            }
        }
    }

    pub(super) fn report_playback_error(mut self: Pin<&mut Self>, error: &PlayerError) {
        self.as_mut().set_playing(false);
        if let Some(playback) = self.as_ref().rust().playback.as_ref() {
            if let Err(stop_error) = playback.stop() {
                tracing::warn!(%stop_error, "Could not clean up failed playback");
            }
        }
        self.as_mut().rust_mut().subtitle_cue = None;
        self.as_mut().set_subtitle_text(QString::default());
        self.as_mut().set_subtitle_data(QString::default());
        let summary = match error {
            PlayerError::Playback(error) => error.user_message(),
            PlayerError::PlaybackUnavailable => "Could not initialize the player",
            PlayerError::InvalidServiceId => "Enter a valid Mirakurun service ID",
        };
        self.as_mut().set_playback_error(QString::from(summary));
        let message = error.to_string();
        self.as_mut()
            .set_playback_error_details(QString::from(&message));
        tracing::error!(%error, "Playback failed");
        match prepare_log_directory() {
            Ok(directory) => {
                let path = directory.join("playback-error.log");
                match std::fs::write(&path, format!("{message}\n")) {
                    Ok(()) => tracing::error!(path = %path.display(), "Playback diagnostic saved"),
                    Err(write_error) => {
                        tracing::warn!(%write_error, path = %path.display(), "Could not save playback diagnostic")
                    }
                }
            }
            Err(error) => tracing::warn!(%error, "Could not prepare playback diagnostic directory"),
        }
        self.as_mut().set_status(QString::from(message));
    }
}

pub(super) fn prepare_log_directory() -> std::io::Result<std::path::PathBuf> {
    let path = std::path::PathBuf::from(ffi::playback_log_directory().to_string());
    if !path.is_absolute() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "User log directory unavailable",
        ));
    }
    std::fs::create_dir_all(&path).map_err(|error| {
        std::io::Error::new(error.kind(), format!("{}: {error}", path.display()))
    })?;
    Ok(path)
}

#[derive(Clone, Copy, Default)]
pub(super) struct UiState {
    pub guide_open: bool,
    pub channels_open: bool,
    pub live_comments: usize,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct PlaybackFlags {
    playing: bool,
    subtitles: bool,
    comments: bool,
}
