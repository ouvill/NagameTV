//! Execution intent belongs to the viewed source, independently of its connection.
use crate::{
    features::data_broadcast::{Demand, Entry},
    player::stream_state::{Attempt, State},
};

#[derive(Default, Debug)]
pub(in crate::player) struct Mode {
    source: Option<Source>,
    engine: Engine,
    entry: Entry,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Source {
    Live(u64),
    Recording,
}
impl Source {
    fn viewed(state: &State) -> Option<Self> {
        match state {
            State::Playing(live, _) | State::Connecting(Attempt::Live(live)) => {
                Some(Self::Live(live.service()))
            }
            State::Recording(_, _) | State::Connecting(Attempt::File(_)) => Some(Self::Recording),
            State::Stopped(_) | State::StopFailed(_) => None,
        }
    }
}
#[derive(Default, Debug)]
enum Engine {
    #[default]
    Stopped,
    Running(Activation),
    // After a receiver/browser failure, periodic projections must not retry it.
    Suppressed,
}
#[derive(Debug)]
enum Activation {
    Automatic,
    User,
}

impl Mode {
    pub(in crate::player) fn demand(&self, prefetch: bool, state: &State) -> Demand {
        if self.requested() {
            Demand::Display
        } else if prefetch && matches!(Source::viewed(state), Some(Source::Live(_))) {
            Demand::Prefetch
        } else if Source::viewed(state).is_some() {
            Demand::Monitor
        } else {
            Demand::Stopped
        }
    }
    pub(in crate::player) fn requested(&self) -> bool {
        matches!(self.engine, Engine::Running(_))
    }
    pub(in crate::player) fn activate(&self) -> bool {
        matches!(self.engine, Engine::Running(Activation::User))
    }
    pub(in crate::player) fn open(&mut self, state: &State) {
        self.source = Source::viewed(state);
        self.engine = if self.source.is_some() {
            Engine::Running(Activation::User)
        } else {
            Engine::Stopped
        };
    }
    pub(in crate::player) fn close(&mut self) {
        *self = Self::default();
    }
    pub(in crate::player) fn dismiss(&mut self) {
        self.engine = Engine::Suppressed;
    }
    pub(in crate::player) fn observe(&mut self, state: &State, entry: Entry) {
        let source = Source::viewed(state);
        // A temporary stopped projection during reconnect must preserve intent.
        if source.is_some() && self.source != source {
            self.close();
            self.source = source;
        }
        if source.is_none() {
            return;
        }
        if entry == Entry::Absent
            && matches!(self.entry, Entry::Manual | Entry::Automatic)
            && !matches!(self.engine, Engine::Suppressed)
        {
            self.engine = Engine::Stopped;
        }
        if entry == Entry::Automatic && matches!(self.engine, Engine::Stopped) {
            self.engine = Engine::Running(Activation::Automatic);
        }
        // Unknown during a reset/reconnect is not evidence that the entry disappeared.
        if entry != Entry::Unknown {
            self.entry = entry;
        }
    }
    /// Same-service reconnects retain both execution and an explicit dismissal.
    pub(in crate::player) fn continues(&self, attempt: &Attempt) -> bool {
        matches!((&self.source, attempt), (Some(Source::Live(service)), Attempt::Live(live)) if *service == live.service())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playback::{input::Retention, recording::Recording, timeline::Phase};

    fn attempt(service: u64) -> Attempt {
        let channels = crate::channels::parse(
            format!(r#"[{{"id":{service},"name":"TV","type":1}}]"#).as_bytes(),
        )
        .unwrap();
        Attempt::new(&channels[0], Retention::Memory.into())
    }

    #[test]
    fn broadcast_startup_and_receiver_dismissal_have_distinct_lifetimes() {
        let mut mode = Mode::default();
        let first = State::Connecting(attempt(1)).started();
        mode.observe(&first, Entry::Manual);
        assert!(!mode.requested());
        mode.observe(&first, Entry::Automatic);
        assert!(mode.requested());
        assert!(!mode.activate(), "automatic startup must not synthesize d");
        mode.observe(&first, Entry::Manual);
        assert!(
            mode.requested(),
            "1 -> 0 does not terminate the running engine"
        );
        mode.dismiss();
        mode.observe(&first, Entry::Automatic);
        assert!(
            !mode.requested(),
            "failed startup must not immediately retry"
        );
        mode.observe(&first, Entry::Absent);
        mode.observe(&first, Entry::Automatic);
        assert!(!mode.requested(), "PMT updates must preserve dismissal");
        assert!(mode.continues(&attempt(1)));
        mode.open(&first);
        assert!(mode.activate());
        mode.observe(&first, Entry::Absent);
        assert!(!mode.requested(), "entry removal terminates the engine");
        mode.observe(&first, Entry::Automatic);
        assert!(mode.requested());
        mode.dismiss();
        let second = State::Connecting(attempt(2)).started();
        mode.observe(&second, Entry::Automatic);
        assert!(mode.requested(), "dismissal belongs to the old source");
        assert!(!mode.activate());
    }

    #[test]
    fn prefetch_is_live_only_and_never_disables_an_open_view() {
        use crate::features::data_broadcast::Demand;
        let mut mode = Mode::default();
        assert_eq!(mode.demand(true, &State::default()), Demand::Stopped);
        let live = State::Connecting(attempt(1));
        assert_eq!(mode.demand(false, &live), Demand::Monitor);
        assert_eq!(mode.demand(true, &live), Demand::Prefetch);
        mode.open(&live);
        assert_eq!(mode.demand(false, &live), Demand::Display);
        assert_eq!(mode.demand(true, &live), Demand::Display);
        mode.close();
        assert_eq!(mode.demand(true, &live), Demand::Prefetch);
        let file = Recording::open(std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../tests/fixtures/subtitle-clock.ts"
        )))
        .unwrap();
        let recording = State::Connecting(Attempt::File(file)).started();
        assert_eq!(mode.demand(true, &recording), Demand::Monitor);
        mode.open(&recording);
        assert_eq!(mode.demand(false, &recording), Demand::Display);
    }

    #[test]
    fn reconnect_preserves_only_the_open_service_across_stream_teardown() {
        let mut mode = Mode::default();
        let mut state = State::Connecting(attempt(1)).started();
        mode.open(&state);
        state = state.transport(Phase::Paused);
        let retry = state.take_retry().unwrap();
        state = state.stopped();
        assert!(!state.active());
        assert!(mode.requested());
        assert!(mode.continues(&retry));
        assert!(!mode.continues(&attempt(2)));
        mode.close();
        assert!(!mode.requested());
        assert!(
            !mode.continues(&retry),
            "stop must also cancel a pending retry's intent"
        );
    }

    #[test]
    fn recording_mode_ends_when_another_input_is_opened() {
        let file = || {
            Recording::open(std::path::Path::new(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../tests/fixtures/subtitle-clock.ts"
            )))
            .unwrap()
        };
        let mut mode = Mode::default();
        let state = State::Connecting(Attempt::File(file())).started();
        mode.open(&state);
        assert!(mode.requested());
        assert!(!mode.continues(&Attempt::File(file())));
        assert!(!mode.continues(&attempt(1)));
        mode.close();
        mode.open(&state.stopped());
        assert!(
            !mode.requested(),
            "stopped file selection is not active viewing"
        );
    }

    #[test]
    fn inactive_playback_cannot_arm_a_future_channel() {
        let mut mode = Mode::default();
        for state in [
            State::default(),
            State::Connecting(attempt(1)).stop_failed(),
        ] {
            mode.open(&state);
            assert!(!mode.requested());
        }
    }
}
