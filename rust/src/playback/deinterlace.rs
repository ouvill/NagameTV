//! Validated startup policy, independent of the Qt presentation.
use gstreamer::{self as gst, prelude::*};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Yadif,
    Linear,
    Off,
}
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid NAGAMETV_DEINTERLACE={0:?}; expected yadif, linear or off")]
    Invalid(String),
    #[error("NAGAMETV_DEINTERLACE is not valid Unicode")]
    NonUnicode,
}
impl Mode {
    pub fn from_environment() -> Result<Self, Error> {
        match std::env::var("NAGAMETV_DEINTERLACE") {
            Ok(value) => Self::parse(&value),
            Err(std::env::VarError::NotPresent) => Ok(Self::default()),
            Err(std::env::VarError::NotUnicode(_)) => Err(Error::NonUnicode),
        }
    }
    pub fn parse(value: &str) -> Result<Self, Error> {
        match value.trim().to_ascii_lowercase().as_str() {
            "yadif" | "quality" => Ok(Self::Yadif),
            "linear" | "balanced" => Ok(Self::Linear),
            "off" | "disabled" => Ok(Self::Off),
            _ => Err(Error::Invalid(value.to_owned())),
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Yadif => "YADIF (auto / all fields)",
            Self::Linear => "Linear (auto / all fields)",
            Self::Off => "Off",
        }
    }
    pub fn build(self) -> Result<gst::Element, gst::glib::BoolError> {
        let method = match self {
            Self::Off => return gst::ElementFactory::make("identity").build(),
            Self::Yadif => "yadif",
            Self::Linear => "linear",
        };
        let element = gst::ElementFactory::make("deinterlace").build()?;
        // Fixed documented enum nicknames, never unvalidated environment values.
        element.set_property_from_str("method", method);
        element.set_property_from_str("mode", "auto");
        element.set_property_from_str("fields", "all");
        Ok(element)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_aliases_and_constructs_each_cpu_processor()
    -> Result<(), Box<dyn std::error::Error>> {
        gst::init()?;
        for (input, expected, factory) in [
            (" YADIF ", Mode::Yadif, "deinterlace"),
            ("quality", Mode::Yadif, "deinterlace"),
            ("balanced", Mode::Linear, "deinterlace"),
            ("linear", Mode::Linear, "deinterlace"),
            ("disabled", Mode::Off, "identity"),
            ("off", Mode::Off, "identity"),
        ] {
            let mode = Mode::parse(input)?;
            assert_eq!(mode, expected);
            let element = mode.build()?;
            assert_eq!(element.factory().ok_or("missing factory")?.name(), factory);
        }
        assert!(matches!(Mode::parse("unknown"), Err(Error::Invalid(_))));
        assert!(Mode::parse("").is_err());
        Ok(())
    }
}
