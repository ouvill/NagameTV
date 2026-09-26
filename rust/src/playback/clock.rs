//! Shared playback clock and latency negotiation for audio/video presentation.
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
    #[error("Invalid display refresh rate: {0} Hz")]
    InvalidRefreshRate(f64),
}

pub(super) fn render_delay(refresh_rate_hz: f64) -> Result<gst::ClockTime, Error> {
    // No screen is normal while a QML window is being attached or detached.
    if refresh_rate_hz == 0.0 {
        return Ok(gst::ClockTime::ZERO);
    }
    if !refresh_rate_hz.is_finite() || refresh_rate_hz < 0.0 {
        return Err(Error::InvalidRefreshRate(refresh_rate_hz));
    }
    let period = std::time::Duration::try_from_secs_f64(1.0 / refresh_rate_hz)
        .map_err(|_| Error::InvalidRefreshRate(refresh_rate_hz))?;
    gst::ClockTime::try_from(period).map_err(|_| Error::InvalidRefreshRate(refresh_rate_hz))
}

pub(super) fn redistribute_latency(
    player: &gst::Element,
    message: &gst::MessageRef,
) -> Result<(), gst::glib::BoolError> {
    if matches!(message.view(), gst::MessageView::Latency(_)) {
        player
            .downcast_ref::<gst::Bin>()
            .ok_or_else(|| gst::glib::bool_error!("Playback is not a GstBin"))?
            .recalculate_latency()?;
    }
    Ok(())
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
    fn display_latency_changes_are_redistributed_to_both_sinks()
    -> Result<(), Box<dyn std::error::Error>> {
        use gstreamer_base::prelude::BaseSinkExt;
        gst::init()?;
        // Real clock/latency negotiation with CPU sources and discard sinks;
        // no display, GPU or audio device is opened.
        let pipeline = gst::parse::launch(
            "videotestsrc is-live=true ! video/x-raw,framerate=25/1 ! fakesink name=video sync=true \
             audiotestsrc is-live=true samplesperbuffer=480 ! audio/x-raw,rate=48000 ! fakesink name=audio sync=true",
        )?.downcast::<gst::Pipeline>().map_err(|_| "pipeline")?;
        struct Stop(gst::Pipeline);
        impl Drop for Stop {
            fn drop(&mut self) {
                if let Err(error) = self.0.set_state(gst::State::Null) {
                    tracing::error!(%error, "Could not stop latency test pipeline");
                }
            }
        }
        let _stop = Stop(pipeline.clone());
        let sink = |name| {
            pipeline
                .by_name(name)
                .ok_or("sink")?
                .downcast::<gstreamer_base::BaseSink>()
                .map_err(|_| "base sink")
        };
        let video = sink("video")?;
        let audio = sink("audio")?;
        pipeline.set_state(gst::State::Playing)?;
        let (result, state, _) = pipeline.state(gst::ClockTime::from_seconds(5));
        result?;
        assert_eq!(state, gst::State::Playing);
        pipeline.recalculate_latency()?;
        let initial = audio.latency();
        let bus = pipeline.bus().ok_or("bus")?;
        for refresh_rate in [60.0, 120.0, 0.0] {
            let before = audio.latency();
            let delay = render_delay(refresh_rate)?;
            video.set_render_delay(delay);
            // A posted LATENCY message does not itself update the other sink.
            assert_eq!(audio.latency(), before);
            let mut handled = false;
            while let Some(message) = bus.pop() {
                handled |= matches!(message.view(), gst::MessageView::Latency(_));
                redistribute_latency(pipeline.upcast_ref(), &message)?;
            }
            assert!(handled);
            assert_eq!(audio.latency(), initial + delay);
            assert_eq!(video.latency(), audio.latency());
        }
        for invalid in [f64::NAN, f64::INFINITY, -60.0, f64::MIN_POSITIVE] {
            assert!(render_delay(invalid).is_err());
        }
        Ok(())
    }

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
