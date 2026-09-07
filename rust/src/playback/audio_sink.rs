//! Explicit audio output selection. A missing device never selects test output.
use gstreamer::{self as gst, prelude::*};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Output {
    Pulse,
    TestDiscard,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid MIRAKURUN_AUDIO_SINK={0:?}; expected pulsesink or fakesink")]
    Invalid(String),
    #[error("MIRAKURUN_AUDIO_SINK is not valid Unicode")]
    NonUnicode,
}

impl Output {
    pub fn from_environment() -> Result<Self, Error> {
        match std::env::var("MIRAKURUN_AUDIO_SINK") {
            Ok(value) => Self::parse(Some(&value)),
            Err(std::env::VarError::NotPresent) => Self::parse(None),
            Err(std::env::VarError::NotUnicode(_)) => Err(Error::NonUnicode),
        }
    }

    fn parse(value: Option<&str>) -> Result<Self, Error> {
        match value {
            None | Some("pulsesink") => Ok(Self::Pulse),
            Some("fakesink") => Ok(Self::TestDiscard),
            Some(value) => Err(Error::Invalid(value.to_owned())),
        }
    }

    pub fn build(self) -> Result<gst::Element, gst::glib::BoolError> {
        let factory = match self {
            Self::Pulse => "pulsesink",
            Self::TestDiscard => "fakesink",
        };
        let sink = gst::ElementFactory::make(factory)
            .property("enable-last-sample", false)
            .build()?;
        if self == Self::TestDiscard {
            // Match main's explicit test sink: retain clock pacing, no last buffer.
            sink.set_property("sync", true);
            eprintln!("Audio output: explicit fakesink test mode (audio discarded)");
        }
        Ok(sink)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_output_requires_explicit_selection_and_preserves_clock_pacing()
    -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(Output::parse(None)?, Output::Pulse);
        assert_eq!(Output::parse(Some("pulsesink"))?, Output::Pulse);
        assert_eq!(Output::parse(Some("fakesink"))?, Output::TestDiscard);
        for value in ["", "auto", "fake", "FAKESINK"] {
            assert!(matches!(Output::parse(Some(value)), Err(Error::Invalid(_))));
        }
        gst::init()?;
        // Construction only; no pipeline state change or output device access.
        for (output, expected) in [
            (Output::Pulse, "pulsesink"),
            (Output::TestDiscard, "fakesink"),
        ] {
            let sink = output.build()?;
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
