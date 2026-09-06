//! Explicitly removable GStreamer observers. Weak object references prevent cycles.
use gstreamer::{self as gst, glib, prelude::*};
use std::sync::{Arc, Mutex};

#[derive(Default, Clone)]
pub struct Subscriptions(Arc<Mutex<State>>);
#[derive(Default)]
struct State {
    closed: bool,
    entries: Vec<Entry>,
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
    fn add(&self, entry: Entry) {
        let mut state = self.0.lock().unwrap();
        if state.closed {
            drop(state);
            entry.remove();
        } else {
            state.entries.retain(Entry::alive);
            state.entries.push(entry);
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
        self.0.lock().unwrap().entries.len()
    }
    /// Caller first brings the pipeline to READY, joining streaming tasks.
    pub fn close(&self) {
        let entries = {
            let mut state = self.0.lock().unwrap();
            state.closed = true;
            std::mem::take(&mut state.entries)
        };
        for entry in entries.into_iter().rev() {
            entry.remove();
        }
    }
}

#[cfg(test)]
mod tests {
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
}
