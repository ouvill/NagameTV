//! Blocking change notifications. Observe before checking input so a change
//! between that check and wait cannot be lost. No timer or polling fallback.
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};

#[derive(Clone, Default)]
pub(super) struct Activity(Arc<Inner>);

#[derive(Default)]
struct Inner {
    revision: Mutex<Revision>,
    changed: Condvar,
    #[cfg(test)]
    wait_observer: Mutex<Option<std::sync::mpsc::Sender<()>>>,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Revision {
    data: u64,
    control: u64,
}

pub(super) enum Change {
    Data,
    Control,
}

pub(super) enum Interest {
    Any,
    Control,
}

/// A checkpoint belongs to the exact input observed, and is consumed by wait.
pub(super) struct Observation<'a> {
    activity: &'a Activity,
    revision: Revision,
}

impl Activity {
    #[cfg(test)]
    pub fn watch_waits(&self) -> std::sync::mpsc::Receiver<()> {
        let (sender, receiver) = std::sync::mpsc::channel();
        *self.0.wait_observer.lock().unwrap() = Some(sender);
        receiver
    }

    fn recover<'a>(
        &self,
        error: PoisonError<MutexGuard<'a, Revision>>,
    ) -> MutexGuard<'a, Revision> {
        tracing::error!(%error, "Could not lock TS input notifications; recovering wake state");
        self.0.revision.clear_poison();
        error.into_inner()
    }

    pub fn observe(&self) -> Observation<'_> {
        Observation {
            activity: self,
            revision: *self
                .0
                .revision
                .lock()
                .unwrap_or_else(|error| self.recover(error)),
        }
    }

    pub fn notify(&self, change: Change) {
        let mut revision = self
            .0
            .revision
            .lock()
            .unwrap_or_else(|error| self.recover(error));
        match change {
            Change::Data => revision.data = revision.data.wrapping_add(1),
            Change::Control => revision.control = revision.control.wrapping_add(1),
        }
        drop(revision);
        self.0.changed.notify_all();
    }
}

impl Observation<'_> {
    pub fn wait(self, interest: Interest) {
        let inner = &self.activity.0;
        let revision = inner
            .revision
            .lock()
            .unwrap_or_else(|error| self.activity.recover(error));
        let unchanged = |current: &Revision| match interest {
            Interest::Any => *current == self.revision,
            Interest::Control => current.control == self.revision.control,
        };
        #[cfg(test)]
        if unchanged(&revision)
            && let Some(observer) = &*inner.wait_observer.lock().unwrap()
            && observer.send(()).is_err()
        {
            // A test may stop observing before its reader is shut down.
        }
        // wait_while also absorbs spurious wakeups without retrying the reader.
        drop(
            inner
                .changed
                .wait_while(revision, |current| unchanged(current))
                .unwrap_or_else(|error| self.activity.recover(error)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_between_inspection_and_wait_are_not_lost() {
        let activity = Activity::default();
        let waiting = activity.watch_waits();
        for (change, interest) in [
            (Change::Data, Interest::Any),
            (Change::Control, Interest::Any),
            (Change::Control, Interest::Control),
        ] {
            let observed = activity.observe();
            activity.notify(change);
            observed.wait(interest);
            assert_eq!(
                waiting.try_recv(),
                Err(std::sync::mpsc::TryRecvError::Empty)
            );
        }
    }
}
