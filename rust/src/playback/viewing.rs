//! A user-visible playback attempt outlives native loading, failures and EOF.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Presence {
    #[default]
    Empty,
    Retained,
}
#[derive(Debug, Default, PartialEq, Eq)]
enum State {
    #[default]
    Empty,
    Retained,
    Inspecting(Presence),
}
#[derive(Debug, Default)]
pub struct Viewing(State);
impl Viewing {
    pub fn available(&self) -> bool {
        !matches!(self.0, State::Empty)
    }
    pub fn inspect(&mut self) {
        self.0 = State::Inspecting(match self.0 {
            State::Empty => Presence::Empty,
            State::Retained => Presence::Retained,
            State::Inspecting(previous) => previous,
        });
    }
    pub fn retain(&mut self) {
        self.0 = State::Retained;
    }
    pub fn cancel_inspection(&mut self) {
        if let State::Inspecting(previous) = self.0 {
            self.0 = match previous {
                Presence::Empty => State::Empty,
                Presence::Retained => State::Retained,
            };
        }
    }
    pub fn close(&mut self) {
        self.0 = State::Empty;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancelled_replacement_restores_the_previous_session() {
        let mut viewing = Viewing::default();
        viewing.inspect();
        viewing.inspect();
        assert!(viewing.available());
        viewing.cancel_inspection();
        assert!(!viewing.available());
        viewing.retain();
        viewing.inspect();
        viewing.inspect();
        viewing.cancel_inspection();
        assert!(viewing.available());
        viewing.close();
        assert!(!viewing.available());
    }
    #[test]
    fn completed_or_failed_inspection_retains_a_session_until_closed() {
        let mut viewing = Viewing::default();
        viewing.inspect();
        viewing.retain();
        viewing.cancel_inspection();
        assert!(viewing.available());
        viewing.close();
        viewing.cancel_inspection();
        assert!(!viewing.available());
    }
}
