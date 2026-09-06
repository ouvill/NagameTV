//! Initial audio policy; user choices survive until their broadcast identity disappears.
use super::{Playback, audio_choices::Choice};
use crate::audio::Program;
use std::time::{Duration, Instant};

#[derive(Default)]
enum Selection {
    #[default]
    Waiting,
    User(String),
    Automatic(String),
}

#[derive(Default)]
pub(super) struct Policy {
    selection: Selection,
    next_check: Option<Instant>,
}

impl Policy {
    pub(super) fn user_choice(&mut self, key: &str) {
        self.selection = Selection::User(key.to_owned());
    }

    fn due(&mut self, now: Instant) -> bool {
        if self.next_check.is_some_and(|next| now < next) {
            return false;
        }
        self.next_check = Some(now + Duration::from_secs(1));
        true
    }

    fn candidate<'a>(&mut self, options: &'a [Choice]) -> Option<&'a Choice> {
        let key = match &self.selection {
            Selection::Waiting => None,
            Selection::User(key) | Selection::Automatic(key) => Some(key),
        };
        // A temporary unsupported format must not override a user's choice.
        if key.is_some_and(|key| options.iter().any(|option| &option.id == key)) {
            return None;
        }
        self.selection = Selection::Waiting;
        options
            .iter()
            .find(|option| option.default && option.enabled)
    }
}

impl Playback {
    pub fn audio_default_due(&self) -> bool {
        self.requested_uri.borrow().is_some() && self.audio_default.borrow_mut().due(Instant::now())
    }

    pub fn apply_audio_default(&self, program: Option<Program<'_>>) {
        let options = match self.audio_choices(program) {
            Ok(options) => options,
            Err(error) => {
                eprintln!("Default audio presentation: {error}");
                self.audio_streams
                    .borrow_mut()
                    .set_failure(Some(super::audio_streams::Error::Presentation));
                return;
            }
        };
        let key = self
            .audio_default
            .borrow_mut()
            .candidate(&options)
            .map(|option| option.id.clone());
        let Some(key) = key else { return };
        // Use the same revalidation and native confirmation path as manual selection.
        let result = self.choose_audio(&key, program);
        // Remember rejected attempts too: do not resend every second. Manual retry is
        // still available, and a new program/descriptor/stream identity permits a retry.
        self.audio_default.borrow_mut().selection = Selection::Automatic(key);
        if let Err(error) = result {
            eprintln!("Default audio selection: {error}");
        } else {
            eprintln!("AUDIO_DEFAULT requested=true");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{audio_choices::choices, audio_routing::Routing, audio_streams::Track};
    use super::*;
    use crate::{audio, channels::BroadcastService};

    #[test]
    fn defaults_wait_for_metadata_and_preserve_manual_choices_until_program_changes()
    -> Result<(), Box<dyn std::error::Error>> {
        let descriptors: Vec<audio::Descriptor> = serde_json::from_str(
            r#"[{"componentTag":16,"componentType":2,"isMain":true,"langs":["jpn","eng"]}]"#,
        )?;
        let mut program = Program {
            key: audio::ProgramKey {
                id: 1,
                start: 100,
                duration: 100,
                service: BroadcastService {
                    network_id: 4,
                    service_id: 101,
                },
            },
            descriptors: &descriptors,
        };
        let tracks = || {
            vec![Track {
                id: "s/00000110".into(),
                component_tag: Some(16),
                language: String::new(),
                title: String::new(),
                selected: false,
            }]
        };
        let format = Routing::default().format();
        let mut policy = Policy::default();
        assert!(
            policy
                .candidate(&choices(tracks(), None, format)?)
                .is_none()
        );
        let options = choices(tracks(), Some(program), format)?;
        assert_eq!(
            policy.candidate(&options).map(|c| c.role),
            Some(audio::Role::Main)
        );
        assert_eq!(options.iter().filter(|c| c.default).count(), 1);
        let mut unavailable = choices(tracks(), Some(program), format)?;
        for option in &mut unavailable {
            option.enabled = false;
        }
        assert!(policy.candidate(&unavailable).is_none());
        // Accepted manual Sub and Both must suppress initial Main selection.
        for index in [1, 2] {
            policy.user_choice(&options[index].id);
            assert!(policy.candidate(&options).is_none());
            assert!(policy.candidate(&unavailable).is_none());
        }
        policy.selection = Selection::Automatic(options[0].id.clone());
        assert!(policy.candidate(&options).is_none());
        program.key.start += 100;
        let next = choices(tracks(), Some(program), format)?;
        assert_eq!(
            policy.candidate(&next).map(|c| c.role),
            Some(audio::Role::Main)
        );
        let now = Instant::now();
        assert!(policy.due(now));
        assert!(!policy.due(now + Duration::from_millis(999)));
        assert!(policy.due(now + Duration::from_secs(1)));
        Ok(())
    }
}
