use super::error_text::{PresentError, Text};
use super::ffi::Player;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::pin::Pin;

impl Player {
    pub(super) fn clear_playback_failure(mut self: Pin<&mut Self>) {
        self.as_mut().set_playback_error(Text::default());
        self.as_mut().set_playback_message(QString::default());
    }

    pub fn open_log_folder(mut self: Pin<&mut Self>) -> bool {
        let result = match &self.rust().error_log {
            Ok(log) => log
                .prepare()
                .map_err(|error| {
                    tracing::error!(
                        error = &error as &dyn std::error::Error,
                        "Log file operation failed"
                    );
                    error.present()
                })
                .and_then(|()| {
                    let path = QString::from(log.directory().to_string_lossy().as_ref());
                    if crate::qt::ffi::open_local_directory(&path) {
                        Ok(())
                    } else {
                        tracing::error!("Could not open the log folder");
                        Err(Text::source("Could not open the log folder."))
                    }
                }),
            Err(error) => {
                tracing::error!(
                    error = error as &dyn std::error::Error,
                    "Log directory is unavailable"
                );
                Err(error.present())
            }
        };

        let opened = result.is_ok();
        self.as_mut()
            .set_log_error(result.err().unwrap_or_default());
        opened
    }

    /// Retain only the latest failure for the UI; ordinary status updates do not erase it.
    /// User retry/server replacement and successful PLAYING clear this projection.
    pub(super) fn playback_failed(mut self: Pin<&mut Self>, error: crate::playback::Error) {
        tracing::error!(error = &error as &dyn std::error::Error, "Playback failed");
        self.as_mut()
            .change_stream_state(super::stream_state::State::stop_failed);
        // Store only a static translation source plus the existing latest diagnostics.
        let hint = if error.hint() == crate::playback::failure::Hint::MissingDecoder {
            error.hint()
        } else if self.recording() {
            crate::playback::failure::Hint::Recording
        } else {
            error.hint()
        };
        self.as_mut()
            .set_playback_message(QString::from(hint.source()));
        let text = error.to_string();
        let result = match &self.rust().error_log {
            Ok(log) => log.save(&text).map_err(|error| {
                tracing::error!(
                    error = &error as &dyn std::error::Error,
                    "Log file operation failed"
                );
                error.present()
            }),
            Err(error) => {
                tracing::error!(
                    error = error as &dyn std::error::Error,
                    "Log directory is unavailable"
                );
                Err(error.present())
            }
        };
        let log_error = result.err().unwrap_or_default();
        self.as_mut().set_log_error(log_error);
        self.as_mut().set_playback_error(error.present());
        self.update_status(super::lifecycle::Status::PlaybackFailed(hint));
    }
}
