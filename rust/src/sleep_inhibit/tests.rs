use super::*;
use std::{cell::RefCell, rc::Rc};

#[derive(Default)]
struct Counts {
    acquired: usize,
    released: usize,
    fail: bool,
}
struct Fake(Rc<RefCell<Counts>>);
struct Grant(Rc<RefCell<Counts>>);
impl Drop for Grant {
    fn drop(&mut self) {
        self.0.borrow_mut().released += 1;
    }
}
impl Request for Grant {
    fn poll(&mut self) -> Result<Progress, Error> {
        if self.0.borrow().fail {
            Err(Error {
                operation: "test acquire",
                unavailable: false,
                cause: "rejected".into(),
            })
        } else {
            Ok(Progress::Held)
        }
    }
    fn release(self) -> Result<(), Error> {
        Ok(())
    }
}
impl Inhibitor for Fake {
    type Request = Grant;
    fn acquire(&mut self) -> Result<Grant, Error> {
        self.0.borrow_mut().acquired += 1;
        Ok(Grant(self.0.clone()))
    }
}
fn controller() -> (Controller<Fake>, Rc<RefCell<Counts>>, Instant) {
    let counts = Rc::new(RefCell::new(Counts::default()));
    (
        Controller::new(Fake(counts.clone()), true),
        counts,
        Instant::now(),
    )
}
fn playing(c: &mut Controller<Fake>, now: Instant) {
    c.update(true, Viewing::Playing, now);
    c.update(true, Viewing::Playing, now);
    assert_eq!(c.status(), Status::Inhibited);
}
#[test]
fn only_confirmed_playback_acquires_and_repeated_updates_do_not_duplicate_grants() {
    let (mut c, counts, now) = controller();
    for v in [Viewing::Inactive, Viewing::Interrupted, Viewing::Inactive] {
        c.update(true, v, now);
    }
    assert_eq!(counts.borrow().acquired, 0);
    playing(&mut c, now);
    for _ in 0..20 {
        c.update(true, Viewing::Playing, now);
    }
    assert_eq!(counts.borrow().acquired, 1);
    c.update(true, Viewing::Inactive, now);
    c.update(true, Viewing::Inactive, now);
    assert_eq!(c.status(), Status::Idle);
    assert_eq!(counts.borrow().released, 1);
}
#[test]
fn interruptions_retain_an_existing_grant_for_exactly_five_seconds_without_extending_it() {
    let (mut c, counts, now) = controller();
    playing(&mut c, now);
    c.update(true, Viewing::Interrupted, now);
    c.update(
        true,
        Viewing::Interrupted,
        now + INTERRUPTION_GRACE - Duration::from_nanos(1),
    );
    assert_eq!(c.status(), Status::Inhibited);
    c.update(true, Viewing::Interrupted, now + INTERRUPTION_GRACE);
    assert_eq!(c.status(), Status::Idle);
    c.update(true, Viewing::Interrupted, now + INTERRUPTION_GRACE * 2);
    assert_eq!(counts.borrow().acquired, 1);
    assert_eq!(counts.borrow().released, 1);
    playing(&mut c, now + INTERRUPTION_GRACE * 2);
    assert_eq!(counts.borrow().acquired, 2);
}
#[test]
fn short_interruption_resumes_with_the_same_grant() {
    let (mut c, counts, now) = controller();
    playing(&mut c, now);
    c.update(true, Viewing::Interrupted, now);
    c.update(true, Viewing::Playing, now + Duration::from_secs(1));
    c.update(true, Viewing::Interrupted, now + Duration::from_secs(10));
    assert_eq!(c.status(), Status::Inhibited);
    assert_eq!(counts.borrow().acquired, 1);
    drop(c);
    assert_eq!(counts.borrow().released, 1);
}
#[test]
fn disabling_pausing_stopping_and_shutdown_release_once_including_during_acquisition() {
    for confirmed in [false, true] {
        let (mut c, counts, now) = controller();
        c.update(true, Viewing::Playing, now);
        if confirmed {
            c.update(true, Viewing::Playing, now);
        }
        c.update(false, Viewing::Playing, now);
        assert_eq!(c.status(), Status::Disabled);
        assert_eq!(counts.borrow().released, 1);
        playing(&mut c, now);
        c.stop(true);
        c.stop(true);
        assert_eq!(c.status(), Status::Idle);
        assert_eq!(counts.borrow().released, 2);
    }
}
#[test]
fn failed_acquisition_is_visible_and_does_not_retry_until_the_viewing_attempt_changes() {
    let (mut c, counts, now) = controller();
    counts.borrow_mut().fail = true;
    c.update(true, Viewing::Playing, now);
    for _ in 0..10 {
        c.update(true, Viewing::Playing, now);
    }
    assert_eq!(c.status(), Status::Failed);
    assert_eq!(counts.borrow().acquired, 1);
    assert_eq!(counts.borrow().released, 1);
    counts.borrow_mut().fail = false;
    c.update(true, Viewing::Inactive, now);
    playing(&mut c, now);
    assert_eq!(counts.borrow().acquired, 2);
}

proptest::proptest! {
    #[test]
    fn arbitrary_viewing_and_setting_changes_never_duplicate_or_leak_a_grant(
        actions in proptest::collection::vec(0_u8..5, 0..100)
    ) {
        let (mut c, counts, start) = controller();
        for (tick, action) in actions.into_iter().enumerate() {
            let now = start + Duration::from_secs(tick as u64);
            match action {
                0 => c.update(true, Viewing::Playing, now),
                1 => c.update(true, Viewing::Interrupted, now),
                2 => c.update(true, Viewing::Inactive, now),
                3 => c.update(false, Viewing::Playing, now),
                4 => c.stop(true),
                _ => unreachable!("generated action range"),
            }
            let counts = counts.borrow();
            proptest::prop_assert!(counts.acquired >= counts.released);
            proptest::prop_assert!(counts.acquired - counts.released <= 1);
            proptest::prop_assert_eq!(counts.acquired - counts.released == 1,
                matches!(c.status(), Status::Acquiring | Status::Inhibited));
        }
        drop(c);
        proptest::prop_assert_eq!(counts.borrow().acquired, counts.borrow().released);
    }
}
