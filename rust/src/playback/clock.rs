//! Opt-in playback clock selection for comparing latency on the target PC.
use gstreamer::{self as gst, prelude::*};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Policy {
    #[default]
    Automatic,
    System,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid NAGAMETV_PLAYBACK_CLOCK={0:?}; expected auto or system")]
    Invalid(String),
    #[error("NAGAMETV_PLAYBACK_CLOCK is not valid Unicode")]
    NonUnicode,
}

impl Policy {
    pub fn from_environment() -> Result<Self, Error> {
        match std::env::var("NAGAMETV_PLAYBACK_CLOCK") {
            Ok(value) => Self::parse(Some(&value)),
            Err(std::env::VarError::NotPresent) => Self::parse(None),
            Err(std::env::VarError::NotUnicode(_)) => Err(Error::NonUnicode),
        }
    }

    fn parse(value: Option<&str>) -> Result<Self, Error> {
        match value {
            None => Ok(Self::default()),
            Some("auto") => Ok(Self::Automatic),
            Some("system") => Ok(Self::System),
            Some(value) => Err(Error::Invalid(value.to_owned())),
        }
    }

    /// Apply the validated policy before exposing the new player to callers.
    /// Both audio and video retain normal synchronization to this clock.
    pub fn build_playbin(self) -> Result<gst::Element, gst::glib::BoolError> {
        let pipeline = gst::ElementFactory::make("playbin3")
            .build()?
            .downcast::<gst::Pipeline>()
            .map_err(|_| gst::glib::bool_error!("playbin3 is not a GstPipeline"))?;
        match self {
            Self::Automatic => {
                tracing::info!("Playback clock policy: auto");
            }
            Self::System => {
                pipeline.use_clock(Some(&gst::SystemClock::obtain()));
                tracing::info!("Playback clock policy: system");
            }
        }
        Ok(pipeline.upcast())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_clock_requires_explicit_selection() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(Policy::parse(None)?, Policy::Automatic);
        assert_eq!(Policy::parse(Some("auto"))?, Policy::Automatic);
        assert_eq!(Policy::parse(Some("system"))?, Policy::System);
        for value in ["", "audio", "sys", "SYSTEM"] {
            assert!(matches!(Policy::parse(Some(value)), Err(Error::Invalid(_))));
        }
        gst::init()?;
        let automatic = Policy::Automatic.build_playbin()?;
        let pipeline = automatic
            .downcast_ref::<gst::Pipeline>()
            .ok_or("pipeline")?;
        assert!(
            !pipeline
                .pipeline_flags()
                .contains(gst::PipelineFlags::FIXED_CLOCK)
        );
        Ok(())
    }

    #[test]
    fn system_clock_survives_stop_and_replay() -> Result<(), Box<dyn std::error::Error>> {
        gst::init()?;
        let player = Policy::System.build_playbin()?;
        struct StopOnDrop(gst::Element);
        impl Drop for StopOnDrop {
            fn drop(&mut self) {
                let _ = self.0.set_state(gst::State::Null);
            }
        }
        let _cleanup = StopOnDrop(player.clone());
        // Explicit CPU-only outputs; never detect or open an audio/GPU device.
        for property in ["audio-sink", "video-sink"] {
            let sink = gst::ElementFactory::make("fakesink")
                .property("sync", true)
                .build()?;
            player.set_property(property, &sink);
        }
        player.set_property_from_str("flags", "audio+video");
        player.set_property("uri", "testbin://audio,is-live=true+video,is-live=true");
        const STATE_TIMEOUT: gst::ClockTime = gst::ClockTime::from_seconds(5);
        for _ in 0..2 {
            player.set_state(gst::State::Playing)?;
            let (result, current, _) = player.state(STATE_TIMEOUT);
            result?;
            assert_eq!(current, gst::State::Playing);
            assert_eq!(player.clock(), Some(gst::SystemClock::obtain().upcast()));
            player.set_state(gst::State::Ready)?;
            let (result, current, _) = player.state(STATE_TIMEOUT);
            result?;
            assert_eq!(current, gst::State::Ready);
        }
        player.set_state(gst::State::Null)?;
        Ok(())
    }
}
