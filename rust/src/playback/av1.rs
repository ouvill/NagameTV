//! Restore TU timestamps lost when av1parse splits AV1 into frames.
use gstreamer::{self as gst, prelude::*};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

const MAX_PENDING_UNITS: usize = 256;

#[derive(Clone, Copy)]
struct Timing {
    pts: Option<gst::ClockTime>,
    duration: Option<gst::ClockTime>,
}

#[derive(Default, PartialEq, Eq)]
enum Alignment {
    #[default]
    Unknown,
    TemporalUnit,
    Frame,
    Other,
}

impl Alignment {
    fn from_caps(caps: &gst::CapsRef) -> Self {
        // Missing alignment is legal; only explicitly TU-aligned input gives
        // us a reliable one-buffer-to-one-temporal-unit correspondence.
        match caps.structure(0).map(|s| s.get::<&str>("alignment")) {
            Some(Ok("tu")) => Self::TemporalUnit,
            Some(Ok("frame")) => Self::Frame,
            _ => Self::Other,
        }
    }
}

enum Queue {
    Tracking(VecDeque<Timing>),
    Suspended,
}

impl Default for Queue {
    fn default() -> Self {
        Self::Tracking(VecDeque::new())
    }
}

#[derive(Default)]
struct Restoration {
    input: Alignment,
    output: Alignment,
    queue: Queue,
    warned: bool,
}

impl Restoration {
    fn reset(&mut self) {
        self.queue = Queue::default();
    }

    fn suspend(&mut self, reason: &str) {
        self.queue = Queue::Suspended;
        if !self.warned {
            tracing::warn!(
                reason,
                "AV1 timestamp restoration lost synchronization; cleared pending units and suspended until stream reset"
            );
            self.warned = true;
        }
    }

    fn sink_event(&mut self, event: &gst::EventRef) {
        match event.view() {
            gst::EventView::StreamStart(_) => {
                self.input = Alignment::Unknown;
                self.output = Alignment::Unknown;
                self.reset();
            }
            gst::EventView::Caps(event) => {
                self.input = Alignment::from_caps(event.caps());
                self.reset();
            }
            gst::EventView::FlushStart(_) => self.queue = Queue::Suspended,
            gst::EventView::FlushStop(_) | gst::EventView::Segment(_) => self.reset(),
            // EOS may drain buffered frames; their timings must survive until
            // the corresponding EOS reaches the parser's source pad.
            _ => {}
        }
    }

    fn src_event(&mut self, event: &gst::EventRef) {
        match event.view() {
            gst::EventView::Caps(event) => {
                let output = Alignment::from_caps(event.caps());
                if self.output != Alignment::Unknown && self.output != output {
                    self.reset();
                }
                self.output = output;
            }
            gst::EventView::Eos(_) => self.queue = Queue::Suspended,
            // BaseParse can defer the initial caps/segment until after it has
            // received a buffer. Clearing here would lose that first TU.
            _ => {}
        }
    }

    fn sink_buffer(&mut self, buffer: &gst::BufferRef) {
        if self.input != Alignment::TemporalUnit {
            return;
        }
        if buffer.flags().contains(gst::BufferFlags::DISCONT) {
            self.reset();
        }
        if let Queue::Tracking(pending) = &mut self.queue {
            if pending.len() == MAX_PENDING_UNITS {
                self.suspend("more than 256 temporal units without matching output");
                return;
            }
            pending.push_back(Timing {
                pts: buffer.pts(),
                duration: buffer.duration(),
            });
        }
    }

    fn src_buffer(&mut self, buffer: &mut gst::BufferRef) {
        if self.input != Alignment::TemporalUnit {
            return;
        }
        let Queue::Tracking(pending) = &mut self.queue else {
            return;
        };
        if self.output != Alignment::Frame {
            pending.clear();
            return;
        }
        let Some(timing) = pending.front() else {
            self.suspend(if buffer.flags().contains(gst::BufferFlags::MARKER) {
                "end-of-TU MARKER without a pending temporal unit"
            } else {
                "frame output without a pending temporal unit"
            });
            return;
        };
        // GStreamer 1.28.2 av1parse loses PTS/duration during TU -> frame
        // conversion, including hidden frames. NVDEC then emits untimed video.
        // Do not replace decoder selection or infer a constant frame rate:
        // retain the demuxer's VFR timing, and remove this workaround when the
        // upstream parser preserves it (see docs/recording-playback.md).
        if buffer.pts().is_none() {
            buffer.set_pts(timing.pts);
        }
        if buffer.duration().is_none() {
            buffer.set_duration(timing.duration);
        }
        if buffer.flags().contains(gst::BufferFlags::MARKER) {
            pending.pop_front();
        }
    }

    fn sink_list(&mut self, list: &gst::BufferListRef) {
        if self.input != Alignment::TemporalUnit {
            return;
        }
        // The probe sees the whole list before the parser handles any of it.
        // A reset in the middle would discard timings for its earlier buffers.
        if list
            .iter()
            .skip(1)
            .any(|b| b.flags().contains(gst::BufferFlags::DISCONT))
        {
            self.suspend("discontinuity inside a temporal unit buffer list");
            return;
        }
        for buffer in list.iter() {
            self.sink_buffer(buffer);
        }
    }
}

pub(super) fn configure(element: &gst::Element) -> Result<(), gst::glib::BoolError> {
    if !element.factory().is_some_and(|f| f.name() == "av1parse") {
        return Ok(());
    }
    let sink = element
        .static_pad("sink")
        .ok_or_else(|| gst::glib::bool_error!("AV1 parser has no sink pad"))?;
    let src = element
        .static_pad("src")
        .ok_or_else(|| gst::glib::bool_error!("AV1 parser has no source pad"))?;
    let state = Arc::new(Mutex::new(Restoration::default()));
    let input = state.clone();
    let mask = gst::PadProbeType::BUFFER
        | gst::PadProbeType::BUFFER_LIST
        | gst::PadProbeType::EVENT_DOWNSTREAM
        | gst::PadProbeType::EVENT_FLUSH;
    let sink_probe = sink
        .add_probe(mask, move |_, info| {
            let mut state = input.lock().expect("AV1 timestamp state");
            if let Some(event) = info.event() {
                state.sink_event(event);
            } else if let Some(buffer) = info.buffer() {
                state.sink_buffer(buffer);
            } else if let Some(list) = info.buffer_list() {
                state.sink_list(list);
            }
            gst::PadProbeReturn::Ok
        })
        .ok_or_else(|| gst::glib::bool_error!("Failed to install AV1 input probe"))?;
    if src
        .add_probe(mask, move |_, info| {
            let mut state = state.lock().expect("AV1 timestamp state");
            if let Some(event) = info.event() {
                state.src_event(event);
            } else if let Some(buffer) = info.buffer_mut() {
                state.src_buffer(buffer.make_mut());
            } else if let Some(list) = info.buffer_list_mut() {
                let list = list.make_mut();
                for index in 0..list.len() {
                    state.src_buffer(list.get_mut(index).expect("buffer list index"));
                }
            }
            gst::PadProbeReturn::Ok
        })
        .is_none()
    {
        sink.remove_probe(sink_probe);
        return Err(gst::glib::bool_error!("Failed to install AV1 output probe"));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
