//! Native output counters and a bounded, credential-free bus history.
use gstreamer::{self as gst, prelude::*};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

const BUS_HISTORY: usize = 24;
#[derive(Clone, Default)]
pub(super) struct Outputs(Arc<Mutex<Option<gst::glib::WeakRef<gst::Element>>>>);
impl Outputs {
    pub fn register(&self, element: &gst::Element) {
        if element
            .factory()
            .is_some_and(|factory| factory.klass().contains("Sink/Audio"))
        {
            self.register_explicit(element);
        }
    }
    pub fn register_explicit(&self, element: &gst::Element) {
        if element.is::<gstreamer_base::BaseSink>() {
            match self.0.lock() {
                Ok(mut sink) => *sink = Some(element.downgrade()),
                Err(error) => {
                    tracing::error!(operation = "observe audio sink", %error, "Could not register live audio output")
                }
            }
        }
    }
    pub fn audio(&self) -> super::Result<Option<u64>> {
        let observed = self.0.lock().map_err(|_| super::Error::LiveObservation)?;
        // Absence and a weak reference expiring during teardown are normal.
        let Some(sink) = observed.as_ref().and_then(|sink| sink.upgrade()) else {
            return Ok(None);
        };
        Ok(super::stats::frame_counters(&sink).map(|counts| counts.rendered))
    }
}

#[derive(Default)]
pub(super) struct BusHistory {
    messages: VecDeque<(std::time::Instant, String)>,
    pub buffering: bool,
}
impl BusHistory {
    pub fn observe(&mut self, message: &gst::MessageRef) {
        let detail = match message.view() {
            gst::MessageView::Buffering(buffering) => {
                self.buffering = buffering.percent() < 100;
                format!("buffering {}", buffering.percent())
            }
            gst::MessageView::StateChanged(state) => format!(
                "state {:?}->{:?} pending {:?}",
                state.old(),
                state.current(),
                state.pending()
            ),
            gst::MessageView::Warning(warning) => format!(
                "warning domain={:?} code={}",
                warning.error().domain(),
                warning.error().code()
            ),
            gst::MessageView::Error(error) => format!(
                "error domain={:?} code={}",
                error.error().domain(),
                error.error().code()
            ),
            gst::MessageView::StreamCollection(_) => "stream-collection".into(),
            gst::MessageView::StreamsSelected(_) => "streams-selected".into(),
            gst::MessageView::StreamStart(_) => "stream-start".into(),
            gst::MessageView::Eos(_) => "eos".into(),
            _ => return,
        };
        if self.messages.len() == BUS_HISTORY {
            self.messages.pop_front();
        }
        self.messages.push_back((std::time::Instant::now(), detail));
    }
    pub fn diagnostic(&self) -> String {
        format!("{:?}", self.messages)
    }
}
