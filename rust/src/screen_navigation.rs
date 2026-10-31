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
    pub fn projection(&self) -> Projection {
        Projection {
            screen: *self.history.last().expect("Home is always present"),
            back: self.history.iter().rev().nth(1).copied(),
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
        if self.history.len() > 1 {
            self.history.pop();
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests;
