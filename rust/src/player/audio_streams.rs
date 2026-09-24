//! Qt boundary for audio choices; current metadata is also checked while the menu is closed.
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
// Stable source strings cross the Qt boundary; QML retranslates even a pending error.
fn error_source(error: crate::playback::audio_streams::Error) -> &'static str {
    use crate::playback::audio_streams::Error;
    match error {
        Error::Unavailable => "This audio track is no longer available. Choose a track again.",
        Error::Rejected => "Could not switch audio tracks. Try again.",
        Error::Unsupported => "Could not determine the audio format. Choose a track again.",
        Error::Presentation => "Could not prepare the audio choices.",
    }
}

impl super::ffi::Player {
    pub(super) fn poll_audio_choice(&self) {
        if let Some(playback) = self.rust().media.playback() {
            let automatic_due = playback.audio_default_due();
            if !playback.has_audio_intent() && !automatic_due {
                return;
            }
            let metadata = self.rust().media.audio_metadata();
            let program = metadata.as_ref().map(crate::audio::Metadata::program);
            if playback.has_audio_intent() {
                playback.update_audio_choice(program);
            }
            if automatic_due {
                playback.apply_audio_default(program);
            }
        }
    }
    pub fn audio_error(&self) -> QString {
        self.rust()
            .media
            .playback()
            .and_then(|player| player.audio_failure())
            .map(|error| QString::from(error_source(error)))
            .unwrap_or_default()
    }
    pub fn audio_tracks(&self) -> QString {
        let metadata = self.rust().media.audio_metadata();
        let result = self
            .rust()
            .media
            .playback()
            .map(|player| {
                player.audio_choices(metadata.as_ref().map(crate::audio::Metadata::program))
            })
            .transpose()
            .and_then(|choices| serde_json::to_string(&choices.unwrap_or_default()));
        match result {
            Ok(json) => QString::from(json),
            Err(error) => {
                tracing::error!("Audio presentation: {error}");
                QString::from("[]")
            }
        }
    }
    pub fn select_audio(&self, key: QString) -> QString {
        let metadata = self.rust().media.audio_metadata();
        let result = self
            .rust()
            .media
            .playback()
            .ok_or(crate::playback::audio_streams::Error::Unavailable)
            .and_then(|player| {
                player.choose_audio(
                    &key.to_string(),
                    metadata.as_ref().map(crate::audio::Metadata::program),
                )
            });
        match result {
            Ok(()) => QString::default(),
            Err(error) => {
                tracing::error!(
                    error = &error as &dyn std::error::Error,
                    "Audio track selection failed"
                );
                QString::from(error_source(error))
            }
        }
    }
}
