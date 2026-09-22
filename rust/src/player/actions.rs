//! Shared UI operations. Choose from the owned state again at execution time.
use super::{
    ffi,
    stream_state::{Attempt, Selection, State},
};
use crate::playback::{
    input::Retention,
    timeline::{Phase, Resume},
};
use cxx_qt::CxxQtType;
use std::pin::Pin;

// Keep an exhaustive Rust enum internally: Qt's FFI enum can hold unknown integers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PlaybackAction {
    Unavailable,
    Play,
    Pause,
    Stop,
}

impl State {
    pub(super) fn playback_action(&self, has_live_selection: bool) -> PlaybackAction {
        use PlaybackAction::{Pause, Play, Stop, Unavailable};
        let live_play = if has_live_selection {
            Play
        } else {
            Unavailable
        };
        match self {
            Self::Stopped(Selection::Live)
            | Self::Connecting(Attempt::Live(_))
            | Self::StopFailed(Attempt::Live(_)) => live_play,
            Self::Stopped(Selection::File(_))
            | Self::Connecting(Attempt::File(_))
            | Self::StopFailed(Attempt::File(_)) => Play,
            Self::Playing(live, phase) => match phase {
                Phase::Playing | Phase::Seeking(Resume::Playing) => {
                    if live.retention().storage() == Retention::Off {
                        Stop
                    } else {
                        Pause
                    }
                }
                Phase::Paused | Phase::Seeking(Resume::Paused) | Phase::Ended => live_play,
            },
            Self::Recording(_, phase) => match phase {
                Phase::Playing | Phase::Seeking(Resume::Playing) => Pause,
                Phase::Paused | Phase::Seeking(Resume::Paused) | Phase::Ended => Play,
            },
        }
    }
}

impl From<PlaybackAction> for ffi::PlaybackAction {
    fn from(action: PlaybackAction) -> Self {
        match action {
            PlaybackAction::Unavailable => Self::Unavailable,
            PlaybackAction::Play => Self::Play,
            PlaybackAction::Pause => Self::Pause,
            PlaybackAction::Stop => Self::Stop,
        }
    }
}

impl ffi::Player {
    pub fn playback_action(&self) -> ffi::PlaybackAction {
        self.rust()
            .stream_state
            .playback_action(self.selected() >= 0)
            .into()
    }

    pub fn toggle_playback(self: Pin<&mut Self>) {
        match self
            .rust()
            .stream_state
            .playback_action(self.selected() >= 0)
        {
            PlaybackAction::Unavailable => {}
            PlaybackAction::Play => self.play(),
            PlaybackAction::Pause => {
                self.pause();
            }
            PlaybackAction::Stop => self.stop(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_tracks_input_pause_seek_end_and_cleanup() {
        let channel =
            crate::channels::parse(br#"[{"id":1,"name":"TV","type":1,"channel":{"type":"GR"}}]"#)
                .unwrap()
                .remove(0);
        assert_eq!(
            State::default().playback_action(false),
            PlaybackAction::Unavailable
        );
        assert_eq!(State::default().playback_action(true), PlaybackAction::Play);
        for retention in [Retention::Off, Retention::Memory, Retention::Filesystem] {
            let mut state = State::Connecting(Attempt::new(&channel, retention.into()));
            assert_eq!(state.playback_action(true), PlaybackAction::Play);
            state = state.started();
            let playing = if retention == Retention::Off {
                PlaybackAction::Stop
            } else {
                PlaybackAction::Pause
            };
            assert_eq!(state.playback_action(true), playing);
            state = state.transport(Phase::Seeking(Resume::Playing));
            assert_eq!(state.playback_action(true), playing);
            state = state.transport(Phase::Seeking(Resume::Paused));
            assert_eq!(state.playback_action(true), PlaybackAction::Play);
            state = state.stop_failed();
            assert_eq!(state.playback_action(true), PlaybackAction::Play);
            assert_eq!(
                state.stopped().playback_action(false),
                PlaybackAction::Unavailable
            );
        }
        let file = crate::playback::recording::Recording::open(std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../tests/fixtures/subtitle-clock.ts"
        )))
        .unwrap();
        let mut state = State::Connecting(Attempt::File(file)).started();
        assert_eq!(state.playback_action(false), PlaybackAction::Pause);
        state = state.transport(Phase::Paused);
        assert_eq!(state.playback_action(false), PlaybackAction::Play);
        state = state.transport(Phase::Ended);
        assert_eq!(state.playback_action(false), PlaybackAction::Play);
        state = state.stop_failed();
        assert_eq!(state.playback_action(false), PlaybackAction::Play);
        assert_eq!(state.stopped().playback_action(false), PlaybackAction::Play);
    }
}
