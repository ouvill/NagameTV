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
                    if super::ffi::open_playback_log_directory(&path) {
                        Ok(())
                    } else {
                        Err("ログフォルダーを開けませんでした".to_owned())
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
        // Store only a static translation source plus the existing latest diagnostics.
        self.as_mut()
            .set_playback_message(QString::from(error.hint().source()));
        let text = error.to_string();
        let result = match &self.rust().error_log {
            Ok(log) => log.save(&text).map_err(|error| error.to_string()),
            Err(error) => Err(error.to_string()),
        };
        let log_error = result.err().unwrap_or_default();
        if !log_error.is_empty() {
            eprintln!("{log_error}");
        }
        self.as_mut().set_log_error(QString::from(log_error));
        self.as_mut()
            .set_playback_error(QString::from(text.as_str()));
        self.status_text(format!("再生エラー: {text}"));
    }
}
