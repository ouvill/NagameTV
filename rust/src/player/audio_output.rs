//! User output commands and their Qt projection. No additional tasks or buffers.
use super::ffi;
use crate::playback::audio_output::Output;
use cxx_qt::CxxQtType;
use std::pin::Pin;

impl ffi::Player {
    pub fn volume(mut self: Pin<&mut Self>, fraction: f64) {
        let Some(output) = self.rust().audio_output.adjust(fraction) else {
            return;
        };
        self.as_mut()
            .rust_mut()
            .preferences
            .preferences_mut()
            .volume = output.volume();
        self.apply_audio_output(output);
    }
    pub fn mute(self: Pin<&mut Self>, muted: bool) {
        let output = self.rust().audio_output.with_mute(muted);
        self.apply_audio_output(output);
    }
    fn apply_audio_output(mut self: Pin<&mut Self>, output: Output) {
        if let Some(playback) = &self.rust().playback {
            playback.set_audio_output(output);
        }
        self.as_mut().rust_mut().audio_output = output;
        self.as_mut().set_volume_level(output.volume().fraction());
        self.set_audio_muted(output.muted());
    }
}
