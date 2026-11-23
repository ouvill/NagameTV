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

pub struct Navigation {
    history: Vec<Screen>,
}
impl Default for Navigation {
    fn default() -> Self {
        Self {
            history: vec![Screen::Home],
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Projection {
    pub screen: Screen,
    pub back: Option<Screen>,
}
impl Navigation {
    fn current(&self) -> Screen {
        *self.history.last().expect("Home is always present")
    }
    /// While a session exists, every other screen goes back to Watching.
    fn resumes_watching(&self, availability: Availability) -> bool {
        availability.watching && self.current() != Screen::Watching
    }
    pub fn projection(&self, availability: Availability) -> Projection {
        let back = if self.resumes_watching(availability) {
            Some(Screen::Watching)
        } else {
            self.history.iter().rev().nth(1).copied()
        };
        Projection {
            screen: self.current(),
            back,
        }
    }
    pub fn reconcile(&mut self, availability: Availability) {
        self.history.retain(|screen| availability.allows(*screen));
    }
    pub fn open(&mut self, screen: Screen, availability: Availability) -> bool {
        self.reconcile(availability);
        if !availability.allows(screen) {
            return false;
        }
        if screen == Screen::Home {
            self.history.truncate(1);
        } else {
            self.history.retain(|previous| *previous != screen);
            self.history.push(screen);
        }
        true
    }
    pub fn back(&mut self, availability: Availability) -> bool {
        self.reconcile(availability);
        let below = self.history.iter().rev().nth(1).copied();
        if self.resumes_watching(availability) && below != Some(Screen::Watching) {
            // Reopening keeps the screen left from Watching as its next back target.
            self.open(Screen::Watching, availability)
        } else if self.history.len() > 1 {
            // A screen opened from Watching is a detour: dropping it keeps the screen
            // that chose the current content below Watching.
            self.history.pop();
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests;
