use super::ffi::Player;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::pin::Pin;

impl Player {
    pub(super) fn clear_playback_failure(mut self: Pin<&mut Self>) {
        self.as_mut().set_playback_error(QString::default());
        self.as_mut().set_playback_message(QString::default());
    }

    pub fn open_log_folder(mut self: Pin<&mut Self>) -> bool {
        let result = match &self.rust().error_log {
            Ok(log) => log
                .prepare()
                .map_err(|error| error.to_string())
                .and_then(|()| {
                    let path = QString::from(log.directory().to_string_lossy().as_ref());
                    if crate::qt::ffi::open_local_directory(&path) {
                        Ok(())
                    } else {
                        // A translation source for the QML boundary, like playback_message.
                        Err("Could not open the log folder.".to_owned())
                    }
                }),
            Err(error) => Err(error.to_string()),
        };
        let opened = result.is_ok();
        self.as_mut()
            .set_log_error(QString::from(result.err().unwrap_or_default()));
        opened
    }

    /// Retain only the latest failure for the UI; ordinary status updates do not erase it.
    /// User retry/server replacement and successful PLAYING clear this projection.
    pub(super) fn playback_failed(mut self: Pin<&mut Self>, error: crate::playback::Error) {
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
            Ok(log) => log.save(&text).map_err(|error| error.to_string()),
            Err(error) => Err(error.to_string()),
        };
        let log_error = result.err().unwrap_or_default();
        if !log_error.is_empty() {
            tracing::error!("{log_error}");
        }
        self.as_mut().set_log_error(QString::from(log_error));
        self.as_mut()
            .set_playback_error(QString::from(text.as_str()));
        self.update_status(super::lifecycle::Status::PlaybackFailed(hint));
    }
}
