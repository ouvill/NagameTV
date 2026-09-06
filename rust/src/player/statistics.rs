//! Serialize the one requested statistics snapshot at the Qt boundary.
use super::ffi;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;

impl ffi::Player {
    pub fn video_stats(&self) -> QString {
        let Some(playback) = &self.rust().playback else {
            return QString::from("{}");
        };
        match serde_json::to_string(&playback.video_stats()) {
            Ok(json) => QString::from(json),
            Err(error) => {
                eprintln!("Video statistics serialization failed: {error}");
                QString::from("{}")
            }
        }
    }
}
