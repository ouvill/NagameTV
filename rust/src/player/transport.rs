//! Publish recording controls as one coherent Qt snapshot.
use super::{ffi, stream_state::State};
use crate::playback::{
    self,
    timeline::{Error, Resume},
};
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::pin::Pin;

impl ffi::Player {
    pub fn media_active(&self) -> bool {
        self.rust().stream_state.active()
    }
    pub fn paused(&self) -> bool {
        self.rust().stream_state.paused()
    }
    pub fn seeking(&self) -> bool {
        self.rust().stream_state.seeking()
    }
    pub fn ended(&self) -> bool {
        self.rust().stream_state.ended()
    }
    pub fn seekable(&self) -> bool {
        matches!(self.rust().stream_state, State::Recording(_, _))
            && self.rust().timeline.range.is_some()
    }
    pub fn position_ms(&self) -> f64 {
        self.rust()
            .timeline
            .position
            .map_or(-1.0, |value| value.mseconds() as f64)
    }
    pub fn duration_ms(&self) -> f64 {
        self.rust()
            .timeline
            .duration
            .map_or(-1.0, |value| value.mseconds() as f64)
    }
    pub fn pause(self: Pin<&mut Self>) -> bool {
        self.control_recording(|media| media.recording_control()?.resume(Resume::Paused))
    }
    pub(super) fn resume_recording(self: Pin<&mut Self>) -> bool {
        self.control_recording(|media| media.recording_control()?.resume(Resume::Playing))
    }
    pub fn seek_to(self: Pin<&mut Self>, milliseconds: f64) -> bool {
        self.control_recording(|media| media.recording_control()?.seek(milliseconds))
    }
    pub fn skip(self: Pin<&mut Self>, milliseconds: f64) -> bool {
        self.control_recording(|media| media.recording_control()?.skip(milliseconds))
    }
    fn control_recording(
        mut self: Pin<&mut Self>,
        operation: impl FnOnce(&mut playback::Session) -> Result<(), Error>,
    ) -> bool {
        let result = if matches!(self.rust().stream_state, State::Recording(_, _)) {
            operation(&mut self.as_mut().rust_mut().media)
        } else {
            Err(Error::Unavailable)
        };
        let accepted = result.is_ok();
        let error = result
            .err()
            .map_or_else(QString::default, |error| QString::from(error.to_string()));
        let changed = self.rust().transport_error != error;
        self.as_mut().rust_mut().transport_error = error;
        self.as_mut().change_stream_state(|state| state);
        if changed {
            self.as_mut().transport_error_changed();
        }
        if self.seeking() {
            self.as_mut().rust_mut().subtitle_cells = 0;
            self.as_mut().set_subtitle_data(QString::default());
            self.as_mut()
                .set_current_program_data(QString::from("null"));
            self.as_mut().set_program_progress(0.0);
        }
        accepted
    }
    pub(super) fn set_transport_error(mut self: Pin<&mut Self>, message: QString) {
        if self.rust().transport_error != message {
            self.as_mut().rust_mut().transport_error = message;
            self.as_mut().transport_error_changed();
        }
    }
}
