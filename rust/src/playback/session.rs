use super::{Playback, PlaybackError, TransportParser};
use gst::prelude::*;
use gstreamer as gst;

/// READY transitions are synchronous: old streaming tasks and dynamic pads
/// stop before the caller installs the next URI. Keep Qt's reusable GL sink.
/// https://gstreamer.freedesktop.org/documentation/additional/design/states.html
pub(super) fn reset_pipeline(playbin: &gst::Element, bus: &gst::Bus) -> Result<(), PlaybackError> {
    bus.set_flushing(true);
    let result = playbin.set_state(gst::State::Ready);
    bus.set_flushing(false);
    result
        .map(|_| ())
        .map_err(|source| PlaybackError::StateChange {
            operation: "reset pipeline",
            source,
        })
}

impl Playback {
    pub(super) fn prepare_stream(
        &self,
        bus: &gst::Bus,
        program: Option<crate::audio::AudioProgram>,
    ) -> Result<(), PlaybackError> {
        reset_pipeline(&self.playbin, bus)?;
        self.reset_stream_state()?;
        self.set_audio_program(program);
        Ok(())
    }

    /// Called after streaming has stopped, before any new source can push data.
    pub(super) fn reset_stream_state(&self) -> Result<(), PlaybackError> {
        self.subtitles.reset();
        *self.audio.borrow_mut() = Default::default();
        self.routing.reset();
        let mut extractor = self
            .extractor
            .lock()
            .map_err(|_| PlaybackError::ExtractorLockPoisoned)?;
        // Drop the entire old PES/PSI buffers and ARIB decoder, not just PMT metadata.
        let enabled = extractor.subtitles_enabled();
        *extractor = TransportParser::new(enabled);
        Ok(())
    }
}
