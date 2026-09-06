//! Qt boundary for native audio identity. Selection errors do not stop video.
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use serde::Serialize;

#[derive(Serialize)]
struct PresentedTrack {
    #[serde(flatten)]
    track: crate::playback::audio_streams::Track,
    role: crate::audio::Role,
}

fn describe(
    mut track: crate::playback::audio_streams::Track,
    descriptors: &[crate::audio::Descriptor],
) -> PresentedTrack {
    let descriptor = crate::audio::matching(descriptors, track.component_tag);
    let role = descriptor.map_or(crate::audio::Role::Unknown, crate::audio::Descriptor::role);
    if let Some(descriptor) = descriptor {
        if let Some(language) = descriptor.langs.first() {
            track.language.clone_from(language);
        } else if descriptor.kind() == crate::audio::Kind::DualMono {
            track.language.clear();
        }
    }
    PresentedTrack { track, role }
}

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
        let state = self.rust();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .and_then(|time| u64::try_from(time.as_millis()).ok());
        let service = usize::try_from(state.selected)
            .ok()
            .and_then(|index| state.entries.get(index))
            .filter(|entry| state.active_service == Some(entry.id))
            .and_then(|entry| entry.broadcast);
        let descriptors = now
            .filter(|_| state.epg_enabled)
            .map_or(&[][..], |now| state.epg.audio_descriptors(service, now));
        let tracks: Vec<_> = tracks
            .into_iter()
            .map(|track| describe(track, descriptors))
            .collect();
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

#[cfg(test)]
mod tests {
    use super::*;
    fn track(tag: Option<u8>) -> crate::playback::audio_streams::Track {
        crate::playback::audio_streams::Track {
            id: "source/00000110".into(),
            component_tag: tag,
            language: "native".into(),
            title: String::new(),
            selected: true,
        }
    }
    #[test]
    fn program_metadata_overrides_native_tags_only_on_an_unambiguous_match()
    -> Result<(), Box<dyn std::error::Error>> {
        let descriptors: Vec<crate::audio::Descriptor> = serde_json::from_str(
            r#"[
            {"componentTag":17,"componentType":3,"isMain":false,"langs":["eng"]},
            {"componentTag":16,"componentType":3,"isMain":true,"langs":["jpn"]}
        ]"#,
        )?;
        let main = describe(track(Some(16)), &descriptors);
        assert_eq!(main.track.language, "jpn");
        assert_eq!(main.role, crate::audio::Role::Main);
        assert!(main.track.selected);
        let missing = describe(track(None), &descriptors);
        assert_eq!(missing.track.language, "native");
        assert_eq!(missing.role, crate::audio::Role::Unknown);
        let expired = describe(track(Some(16)), &[]);
        assert_eq!(expired.role, crate::audio::Role::Unknown);
        assert_eq!(expired.track.language, "native");
        Ok(())
    }
}
