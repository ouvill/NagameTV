//! Exclusive top-level destinations; only this owner may mutate the history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum Screen {
    Home = 0,
    Watching = 1,
    Guide = 2,
    Recordings = 3,
}
impl Screen {
    pub fn from_number(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::Home),
            1 => Some(Self::Watching),
            2 => Some(Self::Guide),
            3 => Some(Self::Recordings),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Availability {
    pub watching: bool,
    pub guide: bool,
}
impl Availability {
    fn allows(self, screen: Screen) -> bool {
        match screen {
            Screen::Home | Screen::Recordings => true,
            Screen::Watching => self.watching,
            Screen::Guide => self.guide,
        }
    }
}

#[derive(Default)]
enum State {
    #[default]
    Disabled,
    Enabled(Vec<Screen>),
}
#[derive(Default)]
pub struct Navigation(State);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Projection {
    pub screen: Screen,
    pub back: Option<Screen>,
}
impl Navigation {
    pub fn new(enabled: bool) -> Self {
        Self(if enabled {
            State::Enabled(vec![Screen::Home])
        } else {
            State::Disabled
        })
    }
    pub fn enabled(&self) -> bool {
        matches!(self.0, State::Enabled(_))
    }
    pub fn enable(&mut self, screen: Screen, availability: Availability) -> bool {
        if !availability.allows(screen) {
            return false;
        }
        if !self.enabled() {
            *self = Self::new(true);
            self.open(screen, availability);
        }
        true
    }
    pub fn disable(&mut self) {
        self.0 = State::Disabled;
    }
    pub fn projection(&self) -> Projection {
        match &self.0 {
            State::Disabled => Projection {
                screen: Screen::Home,
                back: None,
            },
            State::Enabled(stack) => Projection {
                screen: *stack.last().expect("Home is always present"),
                back: stack.iter().rev().nth(1).copied(),
            },
        }
    }
    pub fn reconcile(&mut self, availability: Availability) {
        if let State::Enabled(stack) = &mut self.0 {
            stack.retain(|screen| availability.allows(*screen));
        }
    }
    pub fn open(&mut self, screen: Screen, availability: Availability) -> bool {
        self.reconcile(availability);
        let State::Enabled(stack) = &mut self.0 else {
            return false;
        };
        if !availability.allows(screen) {
            return false;
        }
        if screen == Screen::Home {
            stack.truncate(1);
        } else {
            stack.retain(|previous| *previous != screen);
            stack.push(screen);
        }
        true
    }
    pub fn back(&mut self, availability: Availability) -> bool {
        self.reconcile(availability);
        match &mut self.0 {
            State::Enabled(stack) if stack.len() > 1 => {
                stack.pop();
                true
            }
            State::Disabled | State::Enabled(_) => false,
        }
    }
}

#[cfg(test)]
mod tests;
