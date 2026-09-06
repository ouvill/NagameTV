//! Explicitly removable GStreamer observers. Weak object references prevent cycles.
use gstreamer::{self as gst, glib, prelude::*};
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Clone)]
pub struct Subscriptions(Arc<Mutex<State>>);
impl Default for Subscriptions {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(State::Open(Vec::new()))))
    }
}
#[derive(Default)]
enum State {
    Open(Vec<Entry>),
    #[default]
    Closed,
}
enum Entry {
    Signal(glib::WeakRef<gst::Object>, glib::SignalHandlerId),
    Probe(glib::WeakRef<gst::Pad>, gst::PadProbeId),
    Stats(glib::WeakRef<gst::Element>),
}
impl Entry {
    fn alive(&self) -> bool {
        match self {
            Self::Signal(object, _) => object.upgrade().is_some(),
            Self::Probe(pad, _) => pad.upgrade().is_some(),
            Self::Stats(element) => element.upgrade().is_some(),
        }
    }

    fn remove(self) {
        match self {
            Self::Signal(object, id) => {
                if let Some(object) = object.upgrade() {
                    object.disconnect(id);
                }
            }
            Self::Probe(pad, id) => {
                if let Some(pad) = pad.upgrade() {
                    pad.remove_probe(id);
                }
            }
            Self::Stats(element) => {
                if let Some(element) = element.upgrade() {
                    element.set_property("emit-stats", false);
                }
            }
        }
    }
}
impl Subscriptions {
    fn state(&self) -> MutexGuard<'_, State> {
        match self.0.lock() {
            Ok(state) => state,
            // This is an ownership ledger, not partially decoded stream data.
            // Each Entry remains independently removable after unwinding; there
            // are no cross-entry invariants to restore. Recover the ledger so
            // close (including during Drop) can still release callbacks. Never
            // reset it to Open: a closed scope must reject late registrations.
            Err(poisoned) => poisoned.into_inner(),
        }
    }
    fn add(&self, entry: Entry) {
        let mut state = self.state();
        match &mut *state {
            State::Closed => {
                drop(state);
                entry.remove();
            }
            State::Open(entries) => {
                entries.retain(Entry::alive);
                entries.push(entry);
            }
        }
    }
    pub fn signal(&self, object: &impl IsA<gst::Object>, id: glib::SignalHandlerId) {
        self.add(Entry::Signal(
            object.upcast_ref::<gst::Object>().downgrade(),
            id,
        ));
    }
    pub fn probe(&self, pad: &gst::Pad, id: Option<gst::PadProbeId>) {
        if let Some(id) = id {
            self.add(Entry::Probe(pad.downgrade(), id));
        }
    }
    pub fn stats(&self, element: &gst::Element) {
        self.add(Entry::Stats(element.downgrade()));
    }
    pub fn count(&self) -> usize {
        match &*self.state() {
            State::Open(entries) => entries.len(),
            State::Closed => 0,
        }
    }
    /// Caller first brings the pipeline to READY, joining streaming tasks.
    pub fn close(&self) {
        let previous = std::mem::take(&mut *self.state());
        // Disconnecting can drop captures or call back into the registry.
        // The guard above is gone before any GStreamer removal takes place.
        if let State::Open(entries) = previous {
            for entry in entries.into_iter().rev() {
                entry.remove();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    // unwrap/expect in tests assert fixture setup and expected outcomes.
    use super::*;
    #[test]
    fn close_removes_probe_and_releases_callback_capture() {
        gst::init().unwrap();
        let pad = gst::Pad::builder(gst::PadDirection::Src).build();
        let owner = Arc::new(());
        let weak = Arc::downgrade(&owner);
        let scope = Subscriptions::default();
        let id = pad.add_probe(gst::PadProbeType::BUFFER, move |_, _| {
            let _ = &owner;
            gst::PadProbeReturn::Ok
        });
        scope.probe(&pad, id);
        assert!(weak.upgrade().is_some());
        scope.close();
        assert!(weak.upgrade().is_none());
        assert_eq!(scope.count(), 0);
    }

    #[test]
    fn poisoned_registry_still_closes_and_rejects_late_registrations() {
        // Pads and tsdemux stay in NULL: no display, audio or GPU is used.
        gst::init().unwrap();
        let scope = Subscriptions::default();
        let pad = gst::Pad::builder(gst::PadDirection::Src).build();
        let demux = gst::ElementFactory::make("tsdemux").build().unwrap();
        demux.set_property("emit-stats", true);
        scope.stats(&demux);
        let captured = Arc::new(());
        let weak = Arc::downgrade(&captured);
        scope.probe(
            &pad,
            pad.add_probe(gst::PadProbeType::BUFFER, move |_, _| {
                let _ = &captured;
                gst::PadProbeReturn::Ok
            }),
        );
        let other = scope.clone();
        assert!(
            std::thread::spawn(move || {
                let _guard = other.0.lock().unwrap();
                panic!("inject a poisoned registry");
            })
            .join()
            .is_err()
        );
        assert!(scope.0.is_poisoned());
        assert_eq!(scope.count(), 2);
        scope.close();
        scope.close();
        assert_eq!(scope.count(), 0);
        assert!(weak.upgrade().is_none());
        assert!(!demux.property::<bool>("emit-stats"));

        let captured = Arc::new(());
        let weak = Arc::downgrade(&captured);
        let signal = demux.connect_pad_added(move |_, _| {
            let _ = &captured;
        });
        scope.signal(&demux, signal);
        assert!(weak.upgrade().is_none());
        let captured = Arc::new(());
        let weak = Arc::downgrade(&captured);
        scope.probe(
            &pad,
            pad.add_probe(gst::PadProbeType::BUFFER, move |_, _| {
                let _ = &captured;
                gst::PadProbeReturn::Ok
            }),
        );
        assert!(weak.upgrade().is_none());
        demux.set_property("emit-stats", true);
        scope.stats(&demux);
        assert!(!demux.property::<bool>("emit-stats"));
        assert_eq!(scope.count(), 0);
    }

    #[test]
    fn callback_destruction_observes_closed_state_without_holding_the_lock() {
        use std::sync::atomic::{AtomicBool, Ordering};
        struct Capture(Subscriptions, Arc<AtomicBool>);
        impl Drop for Capture {
            fn drop(&mut self) {
                let closed = self
                    .0
                    .0
                    .try_lock()
                    .is_ok_and(|state| matches!(*state, State::Closed));
                self.1.store(closed, Ordering::Relaxed);
            }
        }
        gst::init().unwrap();
        let scope = Subscriptions::default();
        let pad = gst::Pad::builder(gst::PadDirection::Src).build();
        let observed = Arc::new(AtomicBool::new(false));
        let capture = Capture(scope.clone(), observed.clone());
        scope.probe(
            &pad,
            pad.add_probe(gst::PadProbeType::BUFFER, move |_, _| {
                let _ = &capture;
                gst::PadProbeReturn::Ok
            }),
        );
        scope.close();
        assert!(observed.load(Ordering::Relaxed));
    }
}
