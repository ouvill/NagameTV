//! Typed control for the dual-mono transform. One atomic word binds format and mode.
use gstreamer::{self as gst, prelude::*};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

mod element;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum Mode {
    Both,
    Main,
    Sub,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Format {
    generation: u64,
    pub mode: Option<Mode>,
}

impl Format {
    pub fn generation(self) -> u64 {
        self.generation
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("音声形式が変更されています。もう一度選択してください")]
    Changed,
    #[error("この音声形式では主／副を分離できません")]
    Unsupported,
}

#[derive(Default)]
struct Shared {
    state: AtomicU64,
    stream: Mutex<Option<String>>,
}
#[derive(Clone, Default)]
pub struct Routing(Arc<Shared>);
impl Routing {
    pub fn format(&self) -> Format {
        let state = self.0.state.load(Ordering::Acquire);
        Format {
            generation: state >> 2,
            mode: match state & 3 {
                1 => Some(Mode::Both),
                2 => Some(Mode::Main),
                3 => Some(Mode::Sub),
                _ => None,
            },
        }
    }
    /// Only a caller that has validated current program/PMT metadata may request a side.
    pub fn select(&self, format: Format, mode: Mode) -> Result<(), Error> {
        let result = self
            .0
            .state
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |state| {
                if state >> 2 != format.generation || state & 3 == 0 {
                    return None;
                }
                let bits = match mode {
                    Mode::Both => 1,
                    Mode::Main => 2,
                    Mode::Sub => 3,
                };
                Some((state & !3) | bits)
            });
        match result {
            Ok(_) => Ok(()),
            Err(state) if state >> 2 != format.generation => Err(Error::Changed),
            Err(_) => Err(Error::Unsupported),
        }
    }
    pub fn select_for(&self, format: Format, stream: &str, mode: Mode) -> Result<(), Error> {
        let active = self.0.stream.lock().map_err(|_| Error::Changed)?;
        if active.as_deref() != Some(stream) {
            return Err(Error::Changed);
        }
        self.select(format, mode)
    }
    fn stream_start(&self, stream: &str) -> bool {
        let Ok(mut active) = self.0.stream.lock() else {
            self.renegotiate(false);
            return false;
        };
        // Retain negotiated caps until a new CAPS event, but never retain a side choice.
        self.renegotiate(self.format().mode.is_some());
        *active = Some(stream.to_owned());
        true
    }
    pub fn reset(&self) {
        if let Ok(mut active) = self.0.stream.lock() {
            *active = None;
        }
        self.renegotiate(false);
    }
    fn flush(&self) {
        // FLUSH does not replace sticky CAPS or STREAM_START. Invalidate the side
        // choice while retaining their identity; neither event must be resent.
        let _ = self
            .0
            .state
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |state| {
                Some((state.wrapping_add(4) & !3) | u64::from(state & 3 != 0))
            });
    }
    fn renegotiate(&self, stereo: bool) {
        // Upper 62 bits identify negotiation generations; lower two bits are one valid state.
        // Wrapping prevents arithmetic panic even after the generation space is exhausted.
        let _ = self
            .0
            .state
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |state| {
                Some((state.wrapping_add(4) & !3) | u64::from(stereo))
            });
    }
    #[cfg(test)]
    pub fn filter(&self) -> Result<gst::Bin, gst::glib::BoolError> {
        self.filter_with_tempo(None)
    }
    pub fn filter_with_tempo(
        &self,
        tempo: Option<&gst::Element>,
    ) -> Result<gst::Bin, gst::glib::BoolError> {
        let convert = gst::ElementFactory::make("audioconvert").build()?;
        let transform = element::DualMono::new(self.clone());
        let bin = gst::Bin::new();
        bin.add_many([&convert, transform.upcast_ref()])?;
        convert.link(&transform)?;
        let output = if let Some(tempo) = tempo {
            bin.add(tempo)?;
            transform.link(tempo)?;
            tempo
        } else {
            transform.upcast_ref()
        };
        for (element, name) in [(&convert, "sink"), (output, "src")] {
            let target = element
                .static_pad(name)
                .ok_or_else(|| gst::glib::bool_error!("Audio filter pad missing"))?;
            let ghost = gst::GhostPad::with_target(&target)?;
            bin.add_pad(&ghost)?;
        }
        Ok(bin)
    }
}

fn route(bytes: &mut [u8], mode: Mode) -> Result<(), gst::FlowError> {
    if mode == Mode::Both {
        return Ok(());
    }
    let (frames, remainder) = bytes.as_chunks_mut::<8>();
    if !remainder.is_empty() {
        return Err(gst::FlowError::Error);
    }
    for frame in frames {
        let (left, right) = frame.split_at_mut(4);
        match mode {
            Mode::Main => right.copy_from_slice(left),
            Mode::Sub => left.copy_from_slice(right),
            Mode::Both => {}
        }
    }
    Ok(())
}
