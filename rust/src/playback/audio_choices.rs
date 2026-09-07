//! UI choices are revalidated against the current program, PMT and native format.
use super::{
    Playback,
    audio_routing::{Format, Mode},
    audio_streams::{Error, Track},
};
use crate::audio::{self, Kind, Program, Role};
use serde::Serialize;
use std::time::{Duration, Instant};

#[derive(Serialize)]
pub struct Choice {
    pub id: String,
    pub number: usize,
    pub language: String,
    pub role: Role,
    pub selected: bool,
    pub enabled: bool,
    #[serde(skip)]
    pub(super) default: bool,
    #[serde(skip)]
    native: String,
    #[serde(skip)]
    mode: Mode,
}

pub(super) struct Intent {
    native: String,
    mode: Mode,
    program: Option<audio::ProgramKey>,
    descriptor: Option<audio::Descriptor>,
    applied: Option<Format>,
    deadline: Instant,
}

impl Intent {
    fn matches(
        &self,
        program: Option<Program<'_>>,
        descriptor: Option<&audio::Descriptor>,
        format: Format,
    ) -> bool {
        self.program == program.map(|program| program.key)
            && self.descriptor.as_ref() == descriptor
            && self
                .applied
                .is_none_or(|applied| applied.generation() == format.generation())
    }
}

pub(super) fn choices(
    tracks: Vec<Track>,
    program: Option<Program<'_>>,
    format: Format,
) -> Result<Vec<Choice>, serde_json::Error> {
    let mut result = Vec::new();
    for (index, track) in tracks.into_iter().enumerate() {
        let descriptor = audio::matching(
            program.map_or(&[], |program| program.descriptors),
            track.component_tag,
        );
        let dual = descriptor.is_some_and(|audio| audio.kind() == Kind::DualMono);
        let modes: &[Mode] = if dual {
            &[Mode::Main, Mode::Sub, Mode::Both]
        } else {
            &[Mode::Both]
        };
        for &mode in modes {
            let role = if dual {
                match mode {
                    Mode::Main => Role::Main,
                    Mode::Sub => Role::Sub,
                    Mode::Both => Role::Both,
                }
            } else {
                descriptor.map_or(Role::Unknown, audio::Descriptor::role)
            };
            let language = descriptor
                .and_then(|audio| audio.langs.get(usize::from(mode == Mode::Sub)))
                .cloned()
                .unwrap_or_else(|| {
                    if dual {
                        String::new()
                    } else {
                        track.language.clone()
                    }
                });
            result.push(Choice {
                number: index + 1,
                id: serde_json::to_string(&(
                    &track.id,
                    program.map(|program| program.key),
                    descriptor,
                    mode,
                ))?,
                language,
                role,
                native: track.id.clone(),
                mode,
                selected: track.selected && (!dual || format.mode.unwrap_or(Mode::Both) == mode),
                enabled: !track.selected || !dual || format.mode.is_some(),
                default: descriptor.is_some_and(audio::Descriptor::is_main)
                    && (!dual || mode == Mode::Main),
            });
        }
    }
    Ok(result)
}

impl Playback {
    pub fn has_audio_intent(&self) -> bool {
        self.audio_intent.borrow().is_some()
    }
    pub fn audio_choices(
        &self,
        program: Option<Program<'_>>,
    ) -> Result<Vec<Choice>, serde_json::Error> {
        choices(self.audio_tracks(), program, self.routing.format())
    }
    pub fn choose_audio(&self, key: &str, program: Option<Program<'_>>) -> Result<(), Error> {
        let result = self.request_audio(key, program);
        self.audio_streams
            .borrow_mut()
            .set_failure(result.as_ref().err().copied());
        if result.is_ok() {
            self.audio_default.borrow_mut().user_choice(key);
        }
        result
    }
    fn request_audio(&self, key: &str, program: Option<Program<'_>>) -> Result<(), Error> {
        let choice = self
            .audio_choices(program)
            .map_err(|error| {
                tracing::error!("Audio choice presentation: {error}");
                Error::Presentation
            })?
            .into_iter()
            .find(|choice| choice.id == key && choice.enabled)
            .ok_or(Error::Unavailable)?;
        let state = self
            .audio_streams
            .borrow()
            .state_for_id(&choice.native)
            .ok_or(Error::Unavailable)?;
        // Clear previous routing before a new stream can deliver decoded samples.
        let _ = self.routing.select(self.routing.format(), Mode::Both);
        *self.audio_intent.borrow_mut() = None;
        if !state.1 {
            self.select_audio(&choice.native)?;
        }
        if choice.mode != Mode::Both {
            let descriptor =
                audio::matching(program.map_or(&[], |program| program.descriptors), state.0)
                    .cloned();
            *self.audio_intent.borrow_mut() = Some(Intent {
                native: choice.native,
                mode: choice.mode,
                program: program.map(|program| program.key),
                descriptor,
                applied: None,
                deadline: Instant::now() + Duration::from_secs(5),
            });
        }
        self.update_audio_choice(program);
        Ok(())
    }
    pub fn update_audio_choice(&self, program: Option<Program<'_>>) {
        let mut intent = self.audio_intent.borrow_mut();
        let Some(request) = intent.as_mut() else {
            return;
        };
        let state = self.audio_streams.borrow().state_for_id(&request.native);
        let descriptor =
            state.and_then(|(tag, _)| audio::matching(program.map_or(&[], |p| p.descriptors), tag));
        let format = self.routing.format();
        let changed = !request.matches(program, descriptor, format);
        if state.is_none() || changed {
            let _ = self.routing.select(format, Mode::Both);
            *intent = None;
            return;
        }
        if request.applied.is_some() && state.is_some_and(|(_, selected)| selected) {
            return;
        }
        if state.is_some_and(|(_, selected)| selected)
            && self
                .routing
                .select_for(format, &request.native, request.mode)
                .is_ok()
        {
            tracing::debug!(
                "AUDIO_ROUTE mode={:?} stream={}",
                request.mode,
                request.native
            );
            request.applied = Some(format);
        } else if request.applied.is_some() || Instant::now() >= request.deadline {
            let _ = self.routing.select(format, Mode::Both);
            *intent = None;
            self.audio_streams
                .borrow_mut()
                .set_failure(Some(Error::Unsupported));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::BroadcastService;
    fn track(selected: bool) -> Track {
        Track {
            id: "source/00000110".into(),
            component_tag: Some(16),
            language: "native".into(),
            title: String::new(),
            selected,
        }
    }
    #[test]
    fn dual_options_and_keys_follow_program_and_descriptor_identity()
    -> Result<(), Box<dyn std::error::Error>> {
        let descriptors: Vec<audio::Descriptor> = serde_json::from_str(
            r#"[
            {"componentTag":16,"componentType":2,"isMain":true,"langs":["jpn","eng"]}
        ]"#,
        )?;
        let mut program = Program {
            key: audio::ProgramKey {
                id: 1,
                start: 100,
                duration: 100,
                service: BroadcastService {
                    network_id: 10,
                    service_id: 1,
                },
            },
            descriptors: &descriptors,
        };
        let format = super::super::audio_routing::Routing::default().format();
        let options = choices(vec![track(false)], Some(program), format)?;
        assert_eq!(
            options.iter().map(|choice| choice.role).collect::<Vec<_>>(),
            [Role::Main, Role::Sub, Role::Both]
        );
        assert_eq!(options[0].language, "jpn");
        assert_eq!(options[1].language, "eng");
        assert!(
            options
                .iter()
                .all(|choice| choice.enabled && !choice.selected)
        );
        assert_ne!(options[0].id, options[1].id);
        program.key.start = 200;
        assert_ne!(
            options[0].id,
            choices(vec![track(false)], Some(program), format)?[0].id
        );
        let unsupported = choices(vec![track(true)], Some(program), format)?;
        assert!(unsupported.iter().all(|choice| !choice.enabled));
        assert!(unsupported[2].selected);
        let missing = choices(vec![track(true)], None, format)?;
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].role, Role::Unknown);
        assert_eq!(missing[0].language, "native");
        assert!(missing[0].selected && missing[0].enabled);
        Ok(())
    }
    #[test]
    fn applied_intent_expires_with_program_descriptor_or_format_changes()
    -> Result<(), Box<dyn std::error::Error>> {
        let descriptors: Vec<audio::Descriptor> = serde_json::from_str(
            r#"[
            {"componentTag":16,"componentType":2,"isMain":true,"langs":["jpn","eng"]}
        ]"#,
        )?;
        let program = Program {
            key: audio::ProgramKey {
                id: 1,
                start: 100,
                duration: 100,
                service: BroadcastService {
                    network_id: 10,
                    service_id: 1,
                },
            },
            descriptors: &descriptors,
        };
        let routing = super::super::audio_routing::Routing::default();
        let format = routing.format();
        let intent = Intent {
            native: "s/00000110".into(),
            mode: Mode::Sub,
            program: Some(program.key),
            descriptor: descriptors.first().cloned(),
            applied: Some(format),
            deadline: Instant::now(),
        };
        assert!(intent.matches(Some(program), descriptors.first(), format));
        assert!(!intent.matches(None, None, format));
        assert!(!intent.matches(Some(program), None, format));
        let mut changed = program;
        changed.key.start += 1;
        assert!(!intent.matches(Some(changed), descriptors.first(), format));
        let replacement: Vec<audio::Descriptor> = serde_json::from_str(
            r#"[
            {"componentTag":16,"componentType":3,"isMain":true,"langs":["jpn","eng"]}
        ]"#,
        )?;
        assert!(!intent.matches(Some(program), replacement.first(), format));
        routing.reset();
        assert!(!intent.matches(Some(program), descriptors.first(), routing.format()));
        Ok(())
    }
}
