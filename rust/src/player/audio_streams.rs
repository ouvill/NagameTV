//! Qt boundary for native audio identity. Selection errors do not stop video.
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;

impl super::ffi::Player {
    pub fn audio_error(&self) -> QString {
        self.rust()
            .playback
            .as_ref()
            .and_then(|player| player.audio_failure())
            .map(|error| QString::from(error.to_string()))
            .unwrap_or_default()
    }
    pub fn audio_tracks(&self) -> QString {
        let tracks = self
            .rust()
            .playback
            .as_ref()
            .map(|player| player.audio_tracks())
            .unwrap_or_default();
        match serde_json::to_string(&tracks) {
            Ok(json) => QString::from(json),
            Err(error) => {
                eprintln!("Audio presentation: {error}");
                QString::from("[]")
            }
        }
    }
    pub fn select_audio(&self, id: QString) -> QString {
        let result = self
            .rust()
            .playback
            .as_ref()
            .ok_or(crate::playback::audio_streams::Error::Unavailable)
            .and_then(|player| player.select_audio(&id.to_string()));
        match result {
            Ok(()) => QString::default(),
            Err(error) => QString::from(error.to_string()),
        }
    }
}
