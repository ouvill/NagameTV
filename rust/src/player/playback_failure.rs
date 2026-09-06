use super::ffi::Player;
use cxx_qt_lib::QString;
use std::{fmt::Display, pin::Pin};

impl Player {
    /// Retain only the latest failure for the UI; ordinary status updates do not erase it.
    /// User retry/server replacement and successful PLAYING clear this projection.
    pub(super) fn playback_failed(mut self: Pin<&mut Self>, error: impl Display) {
        let text = error.to_string();
        self.as_mut()
            .set_playback_error(QString::from(text.as_str()));
        self.status_text(format!("再生エラー: {text}"));
    }
}
