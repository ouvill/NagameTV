use super::*;
use crate::sleep_inhibit::{Controller, Status, Viewing};
use std::{collections::BTreeMap, time::Instant};
#[derive(Default)]
struct Data {
    attempts: Vec<Kind>,
    releases: Vec<Kind>,
    failures: BTreeMap<Kind, bool>,
    revision: u64,
    revisions: BTreeMap<Kind, u64>,
}
struct Fake(Rc<RefCell<Data>>);
struct Lease {
    data: Rc<RefCell<Data>>,
    kind: Kind,
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.data.borrow_mut().releases.push(self.kind);
    }
}
impl Request for Lease {
    fn poll(&mut self) -> Result<Progress, Error> {
        if let Some(unavailable) = self.data.borrow().failures.get(&self.kind) {
            Err(Error {
                operation: "test backend",
                unavailable: *unavailable,
                cause: "fixture failure".into(),
            })
        } else {
            Ok(Progress::Held)
        }
    }
    fn release(self) -> Result<(), Error> {
        Ok(())
    }
}
impl Transport for Fake {
    type Lease = Lease;
    fn acquire(&mut self, kind: Kind) -> Result<Lease, Error> {
        self.0.borrow_mut().attempts.push(kind);
        Ok(Lease {
            data: self.0.clone(),
            kind,
        })
    }
    fn revision_for(&mut self, kind: Kind) -> u64 {
        self.0.borrow().revisions.get(&kind).copied().unwrap_or(0)
    }
    fn revision(&mut self) -> u64 {
        self.0.borrow().revision
    }
}
fn setup() -> (Controller<Routes<Fake>>, Rc<RefCell<Data>>) {
    let data = Rc::new(RefCell::new(Data::default()));
    (Controller::new(Routes::new(Fake(data.clone())), true), data)
}
fn settle(c: &mut Controller<Routes<Fake>>) {
    for _ in 0..8 {
        c.update(true, Viewing::Playing, Instant::now());
    }
}
#[test]
fn portal_success_never_starts_fallbacks() {
    let (mut c, data) = setup();
    settle(&mut c);
    assert_eq!(c.status(), Status::Inhibited);
    assert_eq!(data.borrow().attempts, [Kind::Portal]);
    drop(c);
    assert_eq!(data.borrow().releases, [Kind::Portal]);
}
#[test]
fn fallback_requires_sleep_and_screen_and_releases_both_on_stop() {
    let (mut c, data) = setup();
    data.borrow_mut().failures.insert(Kind::Portal, true);
    for _ in 0..3 {
        c.update(true, Viewing::Playing, Instant::now());
        assert_ne!(c.status(), Status::Inhibited);
    }
    settle(&mut c);
    assert_eq!(c.status(), Status::Inhibited);
    assert_eq!(
        data.borrow().attempts,
        [Kind::Portal, Kind::Logind, Kind::ScreenSaver]
    );
    c.stop(true);
    assert!(data.borrow().releases.contains(&Kind::Logind));
    assert!(data.borrow().releases.contains(&Kind::ScreenSaver));
    settle(&mut c);
    assert_eq!(
        data.borrow()
            .attempts
            .iter()
            .filter(|k| **k == Kind::Portal)
            .count(),
        1
    );
}
#[test]
fn screensaver_unavailability_uses_x11_with_a_sleep_lock_and_partial_failure_releases_it() {
    for x11_missing in [false, true] {
        let (mut c, data) = setup();
        for kind in [Kind::Portal, Kind::ScreenSaver] {
            data.borrow_mut().failures.insert(kind, true);
        }
        if x11_missing {
            data.borrow_mut().failures.insert(Kind::X11, true);
        }
        settle(&mut c);
        assert_eq!(
            c.status(),
            if x11_missing {
                Status::Unavailable
            } else {
                Status::Inhibited
            }
        );
        assert_eq!(
            data.borrow().attempts,
            [Kind::Portal, Kind::Logind, Kind::ScreenSaver, Kind::X11]
        );
        drop(c);
        assert!(data.borrow().releases.contains(&Kind::Logind));
    }
}
#[test]
fn unavailable_environment_survives_stop_pause_and_play_until_toggle_or_service_appearance() {
    let (mut c, data) = setup();
    for kind in [Kind::Portal, Kind::Logind] {
        data.borrow_mut().failures.insert(kind, true);
    }
    settle(&mut c);
    assert_eq!(c.status(), Status::Unavailable);
    let count = data.borrow().attempts.len();
    for _ in 0..5 {
        c.stop(true);
        c.update(true, Viewing::Inactive, Instant::now());
        settle(&mut c);
    }
    assert_eq!(c.status(), Status::Unavailable);
    assert_eq!(data.borrow().attempts.len(), count);
    c.reset_environment();
    settle(&mut c);
    assert_eq!(data.borrow().attempts.len(), count * 2);
    data.borrow_mut().failures.clear();
    data.borrow_mut().revision += 1;
    data.borrow_mut().revisions.insert(Kind::Portal, 1);
    settle(&mut c);
    assert_eq!(c.status(), Status::Inhibited);
    assert_eq!(data.borrow().attempts.last(), Some(&Kind::Portal));
}
#[test]
fn denials_remain_failures_and_do_not_bypass_portal_or_retain_partial_grants() {
    for failed in [Kind::Portal, Kind::Logind, Kind::ScreenSaver] {
        let (mut c, data) = setup();
        if failed != Kind::Portal {
            data.borrow_mut().failures.insert(Kind::Portal, true);
        }
        data.borrow_mut().failures.insert(failed, false);
        settle(&mut c);
        assert_eq!(c.status(), Status::Failed);
        assert!(!data.borrow().attempts.contains(&Kind::X11));
        let count = data.borrow().attempts.len();
        settle(&mut c);
        assert_eq!(data.borrow().attempts.len(), count);
        c.update(true, Viewing::Inactive, Instant::now());
        settle(&mut c);
        assert!(data.borrow().attempts.len() > count);
        if failed == Kind::ScreenSaver {
            assert!(data.borrow().releases.contains(&Kind::Logind));
        }
    }
}

#[test]
fn unrelated_service_changes_do_not_reprobe_an_unchanged_unavailable_portal_or_sleep_provider() {
    let (mut c, data) = setup();
    for kind in [Kind::Portal, Kind::Logind] {
        data.borrow_mut().failures.insert(kind, true);
    }
    settle(&mut c);
    let count = data.borrow().attempts.len();
    data.borrow_mut().revision += 1;
    data.borrow_mut().revisions.insert(Kind::ScreenSaver, 1);
    settle(&mut c);
    assert_eq!(data.borrow().attempts.len(), count);
    assert_eq!(c.status(), Status::Unavailable);
}

#[test]
fn cancelling_any_fallback_phase_releases_every_acquired_resource_exactly_once() {
    for updates in 0..6 {
        let (mut c, data) = setup();
        data.borrow_mut().failures.insert(Kind::Portal, true);
        for _ in 0..updates {
            c.update(true, Viewing::Playing, Instant::now());
        }
        c.update(false, Viewing::Playing, Instant::now());
        drop(c);
        assert_eq!(data.borrow().attempts.len(), data.borrow().releases.len());
    }
}

#[test]
fn losing_sleep_inhibition_during_display_acquisition_never_publishes_a_complete_grant() {
    let (mut c, data) = setup();
    data.borrow_mut().failures.insert(Kind::Portal, true);
    for _ in 0..3 {
        c.update(true, Viewing::Playing, Instant::now());
    }
    assert_eq!(data.borrow().attempts.last(), Some(&Kind::ScreenSaver));
    data.borrow_mut().failures.insert(Kind::Logind, false);
    c.update(true, Viewing::Playing, Instant::now());
    assert_eq!(c.status(), Status::Failed);
    assert_eq!(data.borrow().attempts.len(), data.borrow().releases.len());
}

#[test]
fn an_unavailable_held_provider_recovers_when_that_provider_reappears() {
    for provider in [Kind::Portal, Kind::Logind, Kind::ScreenSaver] {
        let (mut c, data) = setup();
        if provider != Kind::Portal {
            data.borrow_mut().failures.insert(Kind::Portal, true);
        }
        settle(&mut c);
        assert_eq!(c.status(), Status::Inhibited);
        data.borrow_mut().failures.insert(provider, true);
        c.update(true, Viewing::Playing, Instant::now());
        assert_eq!(c.status(), Status::Unavailable);
        data.borrow_mut().failures.remove(&provider);
        data.borrow_mut().revision += 1;
        data.borrow_mut().revisions.insert(provider, 1);
        settle(&mut c);
        assert_eq!(c.status(), Status::Inhibited);
    }
}
