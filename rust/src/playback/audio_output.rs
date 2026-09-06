//! Value-only output policy. Muting preserves the volume chosen by the user.
use crate::settings::Volume;
use gstreamer::{self as gst, prelude::*};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Output {
    Audible(Volume),
    Muted(Volume),
}
impl Output {
    pub fn volume(self) -> Volume {
        match self {
            Self::Audible(volume) | Self::Muted(volume) => volume,
        }
    }
    pub fn muted(self) -> bool {
        matches!(self, Self::Muted(_))
    }
    pub fn with_mute(self, muted: bool) -> Self {
        if muted {
            Self::Muted(self.volume())
        } else {
            Self::Audible(self.volume())
        }
    }
    /// Matches main: adjusting the slider explicitly resumes audible output.
    pub fn adjust(self, fraction: f64) -> Option<Self> {
        Volume::from_fraction(fraction).map(Self::Audible)
    }
    pub(super) fn apply(self, player: &gst::Element) {
        // Set volume before unmuting so an adjustment cannot briefly play at the
        // previous (possibly louder) level. playbin's soft-volume flag is enabled.
        if self.muted() {
            player.set_property("mute", true);
        }
        player.set_property("volume", self.volume().fraction());
        if !self.muted() {
            player.set_property("mute", false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mute_preserves_volume_and_adjustment_unmutes() -> Result<(), Box<dyn std::error::Error>> {
        let audible = Output::Audible(Volume::from(73.0));
        let muted = audible.with_mute(true);
        assert_eq!(muted.volume(), audible.volume());
        assert_eq!(muted.with_mute(false), audible);
        assert_eq!(muted.with_mute(true), muted);
        assert_eq!(
            muted.adjust(0.2).ok_or("valid volume rejected")?,
            Output::Audible(Volume::from(20.0))
        );
        assert!(muted.adjust(f64::NAN).is_none());
        assert!(muted.adjust(f64::INFINITY).is_none());
        assert_eq!(
            muted
                .adjust(-1.0)
                .ok_or("clamped volume rejected")?
                .volume()
                .fraction(),
            0.0
        );
        Ok(())
    }
    #[test]
    fn native_playbin_preserves_output_across_ready_and_null()
    -> Result<(), Box<dyn std::error::Error>> {
        gst::init()?;
        // No URI or PLAYING transition: no sink, audio device, display or GPU use.
        let player = gst::ElementFactory::make("playbin3").build()?;
        let initial = Output::Audible(Volume::from(73.0));
        for output in [
            initial,
            initial.with_mute(true),
            initial.adjust(0.2).ok_or("invalid volume")?,
        ] {
            output.apply(&player);
            for state in [gst::State::Ready, gst::State::Null] {
                player.set_state(state)?;
                assert_eq!(player.property::<bool>("mute"), output.muted());
                assert_eq!(player.property::<f64>("volume"), output.volume().fraction());
            }
        }
        Ok(())
    }
}
