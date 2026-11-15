//! HDMI-CEC user control codes (Linux cec.h / HDMI CEC 1.4) to existing inputs.
use std::time::{Duration, Instant};
use viewer_remote::model::{Command, RemoteKey, Step};

// A release can be lost on the bus. A new press after this interval starts a new gesture.
const RELEASE_TIMEOUT: Duration = Duration::from_millis(550);
const INITIATOR_COUNT: usize = 16;
const SELECT: u8 = 0x00;
const UP: u8 = 0x01;
const DOWN: u8 = 0x02;
const LEFT: u8 = 0x03;
const RIGHT: u8 = 0x04;
const BACK: u8 = 0x0d;
const EXIT: u8 = 0x0d;
const DIGIT_0: u8 = 0x20;
const DIGIT_9: u8 = 0x29;
const ENTER: u8 = 0x2b;
const CHANNEL_UP: u8 = 0x30;
const CHANNEL_DOWN: u8 = 0x31;
const PREVIOUS_CHANNEL: u8 = 0x32;
const PLAY: u8 = 0x44;
const STOP: u8 = 0x45;
const PAUSE: u8 = 0x46;
const REWIND: u8 = 0x48;
const FAST_FORWARD: u8 = 0x49;
const FORWARD: u8 = 0x4b;
const BACKWARD: u8 = 0x4c;
const PLAY_FUNCTION: u8 = 0x60;
const PAUSE_FUNCTION: u8 = 0x61;
const STOP_FUNCTION: u8 = 0x64;
const BLUE: u8 = 0x71;
const RED: u8 = 0x72;
const GREEN: u8 = 0x73;
const YELLOW: u8 = 0x74;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputKey {
    Digit(u8),
    SeekBackward,
    SeekForward,
    PreviousChapter,
    NextChapter,
    Blue,
    Red,
    Green,
    Yellow,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Input {
    Command(Command),
    Key { key: InputKey, repeat: bool },
}
impl Input {
    fn repeatable(self) -> bool {
        matches!(
            self,
            Self::Key {
                key: InputKey::SeekBackward | InputKey::SeekForward,
                ..
            }
        )
    }
}
fn map(code: u8) -> Option<Input> {
    let key = match code {
        SELECT | ENTER => return Some(Input::Command(Command::SendKey(RemoteKey::Ok))),
        UP => return Some(Input::Command(Command::SendKey(RemoteKey::Up))),
        DOWN => return Some(Input::Command(Command::SendKey(RemoteKey::Down))),
        LEFT => return Some(Input::Command(Command::SendKey(RemoteKey::Left))),
        RIGHT => return Some(Input::Command(Command::SendKey(RemoteKey::Right))),
        // Linux headers use BACK for the CEC Exit/Back control, both 0x0d.
        BACK => {
            debug_assert_eq!(BACK, EXIT);
            return Some(Input::Command(Command::SendKey(RemoteKey::Back)));
        }
        CHANNEL_UP => return Some(Input::Command(Command::StepChannel(Step::Next))),
        CHANNEL_DOWN => return Some(Input::Command(Command::StepChannel(Step::Previous))),
        PREVIOUS_CHANNEL => return Some(Input::Command(Command::PreviousChannel)),
        PLAY | PLAY_FUNCTION => return Some(Input::Command(Command::Play)),
        PAUSE | PAUSE_FUNCTION => return Some(Input::Command(Command::SetPaused(true))),
        STOP | STOP_FUNCTION => return Some(Input::Command(Command::Stop)),
        DIGIT_0..=DIGIT_9 => InputKey::Digit(code - DIGIT_0),
        REWIND => InputKey::SeekBackward,
        FAST_FORWARD => InputKey::SeekForward,
        FORWARD => InputKey::NextChapter,
        BACKWARD => InputKey::PreviousChapter,
        BLUE => InputKey::Blue,
        RED => InputKey::Red,
        GREEN => InputKey::Green,
        YELLOW => InputKey::Yellow,
        _ => {
            tracing::debug!(code, "Unknown HDMI-CEC user control code ignored");
            return None;
        }
    };
    Some(Input::Key { key, repeat: false })
}
#[derive(Clone, Copy)]
struct Held {
    code: u8,
    last: Instant,
}
pub(super) struct HeldKey {
    held: [Option<Held>; INITIATOR_COUNT],
}
impl Default for HeldKey {
    fn default() -> Self {
        Self {
            held: [None; INITIATOR_COUNT],
        }
    }
}
impl HeldKey {
    pub fn clear(&mut self) {
        self.held.fill(None);
    }
    pub fn release(&mut self, initiator: u8) {
        if let Some(held) = self.held.get_mut(usize::from(initiator)) {
            *held = None;
        }
    }
    pub fn press(&mut self, initiator: u8, code: u8, now: Instant) -> Option<Input> {
        let held = self.held.get_mut(usize::from(initiator))?;
        let repeat = held.is_some_and(|held| {
            held.code == code && now.saturating_duration_since(held.last) < RELEASE_TIMEOUT
        });
        *held = Some(Held { code, last: now });
        let input = map(code)?;
        if repeat {
            if !input.repeatable() {
                return None;
            }
            if let Input::Key { key, .. } = input {
                return Some(Input::Key { key, repeat: true });
            }
        }
        Some(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn controls_use_the_existing_remote_and_keyboard_paths() {
        for (code, expected) in [
            (SELECT, Input::Command(Command::SendKey(RemoteKey::Ok))),
            (UP, Input::Command(Command::SendKey(RemoteKey::Up))),
            (DOWN, Input::Command(Command::SendKey(RemoteKey::Down))),
            (LEFT, Input::Command(Command::SendKey(RemoteKey::Left))),
            (RIGHT, Input::Command(Command::SendKey(RemoteKey::Right))),
            (BACK, Input::Command(Command::SendKey(RemoteKey::Back))),
            (
                CHANNEL_UP,
                Input::Command(Command::StepChannel(Step::Next)),
            ),
            (
                CHANNEL_DOWN,
                Input::Command(Command::StepChannel(Step::Previous)),
            ),
            (PREVIOUS_CHANNEL, Input::Command(Command::PreviousChannel)),
            (PLAY, Input::Command(Command::Play)),
            (PAUSE, Input::Command(Command::SetPaused(true))),
            (STOP, Input::Command(Command::Stop)),
        ] {
            assert_eq!(map(code), Some(expected));
        }
        for (code, key) in [
            (REWIND, InputKey::SeekBackward),
            (FAST_FORWARD, InputKey::SeekForward),
            (FORWARD, InputKey::NextChapter),
            (BACKWARD, InputKey::PreviousChapter),
            (BLUE, InputKey::Blue),
            (RED, InputKey::Red),
            (GREEN, InputKey::Green),
            (YELLOW, InputKey::Yellow),
        ] {
            assert_eq!(map(code), Some(Input::Key { key, repeat: false }));
        }
        for digit in 0..=9 {
            assert_eq!(
                map(DIGIT_0 + digit),
                Some(Input::Key {
                    key: InputKey::Digit(digit),
                    repeat: false
                })
            );
        }
        assert_eq!(map(0xff), None);
    }
    #[test]
    fn only_seek_repeats_and_release_or_timeout_starts_a_new_gesture() {
        let now = Instant::now();
        for code in 0..=u8::MAX {
            let mut held = HeldKey::default();
            let first = held.press(0, code, now);
            let repeat = held.press(0, code, now + Duration::from_millis(100));
            assert_eq!(
                repeat.is_some(),
                first.is_some_and(Input::repeatable),
                "{code:x}"
            );
            if let Some(Input::Key { repeat, .. }) = repeat {
                assert!(repeat);
            }
            held.release(0);
            assert_eq!(held.press(0, code, now), first);
            assert_eq!(held.press(0, code, now + RELEASE_TIMEOUT), first);
        }
    }
    #[test]
    fn other_initiators_and_unknown_keys_do_not_repeat_the_previous_action() {
        let now = Instant::now();
        let mut held = HeldKey::default();
        assert!(held.press(0, SELECT, now).is_some());
        assert!(held.press(1, SELECT, now).is_some());
        held.release(1);
        assert!(held.press(0, SELECT, now).is_none());
        assert!(held.press(0, 0xff, now).is_none());
        assert!(held.press(0, SELECT, now).is_some());
    }
    proptest::proptest! {
        #[test]
        fn arbitrary_gestures_only_repeat_seek_and_releases_restore_first_presses(
            actions in proptest::collection::vec((0u8..16, proptest::prelude::any::<u8>(), 0u16..1000, proptest::prelude::any::<bool>()), 0..300)
        ) {
            let mut held = HeldKey::default();
            let mut previous = [None; INITIATOR_COUNT];
            let mut elapsed_ms = 0u64;
            let start = Instant::now();
            for (initiator, code, delay, release) in actions {
                elapsed_ms += u64::from(delay);
                let slot = &mut previous[usize::from(initiator)];
                if release { held.release(initiator); *slot = None; }
                let same_gesture = slot.is_some_and(|(previous_code, time)| previous_code == code && elapsed_ms - time < RELEASE_TIMEOUT.as_millis() as u64);
                let input = held.press(initiator, code, start + Duration::from_millis(elapsed_ms));
                if let Some(input) = input {
                    if same_gesture { proptest::prop_assert!(input.repeatable()); }
                    if let Input::Key { repeat, .. } = input { proptest::prop_assert_eq!(repeat, same_gesture); }
                }
                if !same_gesture { proptest::prop_assert_eq!(input.is_some(), map(code).is_some()); }
                *slot = Some((code, elapsed_ms));
            }
        }
    }
}
