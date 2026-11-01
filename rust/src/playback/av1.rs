//! Restore TU timestamps lost when av1parse splits AV1 into frames.
use gstreamer::{self as gst, prelude::*};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

const MAX_PENDING_UNITS: usize = 256;
const MAX_PENDING_BYTES: usize = 32 * 1024 * 1024;

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
        let Some(structure) = caps.structure(0) else {
            return Self::Other;
        };
        // Format conversion can rewrite payloads, so byte correspondence is
        // only meaningful when both pads negotiate the same OBU stream format.
        if !matches!(structure.get::<&str>("stream-format"), Ok("obu-stream")) {
            return Self::Other;
        }
        match structure.get::<&str>("alignment") {
            Ok("tu") => Self::TemporalUnit,
            Ok("frame") => Self::Frame,
            _ => Self::Other,
        }
    }
}

struct PendingUnit {
    timing: Timing,
    bytes: Vec<u8>,
    consumed: usize,
}

fn take_timing(pending: &mut VecDeque<PendingUnit>, frame: &[u8]) -> Option<Timing> {
    if frame.is_empty() {
        return None;
    }
    let mut matches = pending.iter().enumerate().filter_map(|(index, unit)| {
        unit.bytes[unit.consumed..]
            .windows(frame.len())
            .position(|bytes| bytes == frame)
            .map(|offset| (index, unit.consumed + offset + frame.len()))
    });
    let (index, end) = matches.next()?;
    if matches.next().is_some() {
        // Repeated encoded frames can occur in different TUs. Neither MARKER
        // nor the latest sink buffer disambiguates delayed/batched output.
        return None;
    }
    pending.drain(..index);
    let unit = pending.front_mut()?;
    let timing = unit.timing;
    unit.consumed = end;
    if end == unit.bytes.len() {
        pending.pop_front();
    }
    Some(timing)
}

enum Queue {
    Tracking(VecDeque<PendingUnit>),
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
            if buffer.size() > MAX_PENDING_BYTES
                || pending.iter().map(|unit| unit.bytes.len()).sum::<usize>()
                    > MAX_PENDING_BYTES - buffer.size()
            {
                self.suspend("more than 32 MiB of pending temporal unit payloads");
                return;
            }
            let bytes = match buffer.map_readable() {
                Ok(map) => map.as_slice().to_vec(),
                Err(error) => {
                    self.suspend(&format!("cannot read temporal unit payload: {error}"));
                    return;
                }
            };
            pending.push_back(PendingUnit {
                timing: Timing {
                    pts: buffer.pts(),
                    duration: buffer.duration(),
                },
                bytes,
                consumed: 0,
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
        let timing = match buffer.map_readable() {
            Ok(map) => take_timing(pending, map.as_slice()),
            Err(error) => {
                self.suspend(&format!("cannot read frame payload: {error}"));
                return;
            }
        };
        let Some(timing) = timing else {
            self.suspend("frame payload has no unique pending temporal unit");
            return;
        };
        // av1parse emits OBU-stream frames in encoded order, but 1.24 marks
        // every frame and can drop whole TUs. Match unchanged encoded bytes,
        // not MARKER counts, before restoring missing timing (including hidden
        // frames needed by NVDEC). See docs/recording-playback.md.
        if buffer.pts().is_none() {
            buffer.set_pts(timing.pts);
        }
        if buffer.duration().is_none() {
            buffer.set_duration(timing.duration);
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
