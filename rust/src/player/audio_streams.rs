//! Qt boundary for audio choices; current metadata is also checked while the menu is closed.
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
impl super::ffi::Player {
    fn audio_program(&self) -> Option<crate::audio::Program<'_>> {
        let state = self.rust();
        if !state.epg_enabled {
            return None;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .and_then(|time| u64::try_from(time.as_millis()).ok())?;
        let entry = usize::try_from(state.selected)
            .ok()
            .and_then(|index| state.entries.get(index))
            .filter(|entry| state.active_service == Some(entry.id))?;
        state.epg.audio_program(entry.broadcast, now)
    }
    pub(super) fn poll_audio_choice(&self) {
        if let Some(playback) = self.rust().playback.as_ref()
            && playback.has_audio_intent()
        {
            playback.update_audio_choice(self.audio_program());
        }
    }
    pub fn audio_error(&self) -> QString {
        self.rust()
            .playback
            .as_ref()
            .and_then(|player| player.audio_failure())
            .map(|error| QString::from(error.to_string()))
            .unwrap_or_default()
    }
    pub fn audio_tracks(&self) -> QString {
        let result = self
            .rust()
            .playback
            .as_ref()
            .map(|player| player.audio_choices(self.audio_program()))
            .transpose()
            .and_then(|choices| serde_json::to_string(&choices.unwrap_or_default()));
        match result {
            Ok(json) => QString::from(json),
            Err(error) => {
                eprintln!("Audio presentation: {error}");
                QString::from("[]")
            }
        }
    }
    pub fn select_audio(&self, key: QString) -> QString {
        let result = self
            .rust()
            .playback
            .as_ref()
            .ok_or(crate::playback::audio_streams::Error::Unavailable)
            .and_then(|player| player.choose_audio(&key.to_string(), self.audio_program()));
        match result {
            Ok(()) => QString::default(),
            Err(error) => QString::from(error.to_string()),
        }
    }
}
