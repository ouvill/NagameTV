//! Main-compatible audio selection with an explicit, separate test override.
use gstreamer::{self as gst, prelude::*};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Output {
    Automatic,
    Pulse,
    TestDiscard,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid NAGAMETV_AUDIO_SINK={0:?}; expected pulsesink or fakesink")]
    Invalid(String),
    #[error("NAGAMETV_AUDIO_SINK is not valid Unicode")]
    NonUnicode,
}

impl Output {
    pub fn from_environment() -> Result<Self, Error> {
        let pulse_server_present = std::env::var_os("PULSE_SERVER").is_some();
        match std::env::var("NAGAMETV_AUDIO_SINK") {
            Ok(value) => Self::parse(Some(&value), pulse_server_present),
            Err(std::env::VarError::NotPresent) => Self::parse(None, pulse_server_present),
            Err(std::env::VarError::NotUnicode(_)) => Err(Error::NonUnicode),
        }
    }

    fn parse(value: Option<&str>, pulse_server_present: bool) -> Result<Self, Error> {
        match value {
            None if pulse_server_present => Ok(Self::Pulse),
            None => Ok(Self::Automatic),
            Some("pulsesink") => Ok(Self::Pulse),
            Some("fakesink") => Ok(Self::TestDiscard),
            Some(value) => Err(Error::Invalid(value.to_owned())),
        }
    }

    pub fn build(self) -> Result<Option<gst::Element>, gst::glib::BoolError> {
        let factory = match self {
            // Like main, leave audio-sink unset so playbin selects the platform
            // output. An explicit Pulse endpoint avoids probing other backends.
            Self::Automatic => return Ok(None),
            Self::Pulse => "pulsesink",
            Self::TestDiscard => "fakesink",
        };
        let sink = gst::ElementFactory::make(factory)
            .property("enable-last-sample", false)
            .build()?;
        if self == Self::TestDiscard {
            // Match main's explicit test sink: retain clock pacing, no last buffer.
            sink.set_property("sync", true);
            tracing::info!("Audio output: explicit fakesink test mode (audio discarded)");
        }
        Ok(Some(sink))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_output_requires_explicit_selection_and_preserves_clock_pacing()
    -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(Output::parse(None, true)?, Output::Pulse);
        assert_eq!(Output::parse(None, false)?, Output::Automatic);
        assert!(Output::Automatic.build()?.is_none());
        assert_eq!(Output::parse(Some("pulsesink"), false)?, Output::Pulse);
        assert_eq!(Output::parse(Some("fakesink"), true)?, Output::TestDiscard);
        for value in ["", "auto", "fake", "FAKESINK"] {
            assert!(matches!(
                Output::parse(Some(value), false),
                Err(Error::Invalid(_))
            ));
        }
        gst::init()?;
        // Construction only; no pipeline state change or output device access.
        for (output, expected) in [
            (Output::Pulse, "pulsesink"),
            (Output::TestDiscard, "fakesink"),
        ] {
            let sink = output.build()?.ok_or("missing explicit sink")?;
            assert_eq!(
                sink.factory().ok_or("missing sink factory")?.name(),
                expected
            );
            assert!(!sink.property::<bool>("enable-last-sample"));
            assert!(sink.property::<bool>("sync"));
        }
        Ok(())
    }
}
