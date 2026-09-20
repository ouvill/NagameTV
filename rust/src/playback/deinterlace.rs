//! Validated startup policy, independent of the Qt presentation.
use gstreamer::{self as gst, prelude::*};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Yadif,
    Linear,
    Off,
    /// NVDEC GL textures followed by the single-rate OpenGL filter.
    NvidiaGl,
    /// Linux VA surfaces, with field-rate hardware deinterlacing.
    VaApi,
}
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid NAGAMETV_DEINTERLACE={0:?}; expected yadif, linear, off, gl or va")]
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
            "gl" => Ok(Self::NvidiaGl),
            "va" => Ok(Self::VaApi),
            _ => Err(Error::Invalid(value.to_owned())),
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Yadif => "YADIF (auto / all fields)",
            Self::Linear => "Linear (auto / all fields)",
            Self::Off => "Off",
            Self::NvidiaGl => "NVDEC + OpenGL vfir (single rate)",
            Self::VaApi => "VA-API adaptive (all fields / NV12)",
        }
    }
    pub fn build(self) -> Result<gst::Element, gst::glib::BoolError> {
        let method = match self {
            Self::Off => return gst::ElementFactory::make("identity").build(),
            Self::Yadif => "yadif",
            Self::Linear => "linear",
            Self::NvidiaGl => {
                let element = gst::ElementFactory::make("gldeinterlace").build()?;
                // The upstream greedyh implementation retains a raw prev_tex
                // pointer across stop/start after releasing prev_buffer. Use
                // the stateless filter so stream changes cannot dereference it.
                element.set_property_from_str("method", "vfir");
                return Ok(element);
            }
            Self::VaApi => {
                let element = gst::ElementFactory::make("vadeinterlace").build()?;
                // VA registers this enum from the driver's capabilities.
                // property_from_str panics when adaptive is absent; validate
                // the actual property before requesting the algorithm.
                let method = adaptive_method(element.find_property("method"))?;
                element.set_property("method", method);
                return Ok(element);
            }
        };
        let element = gst::ElementFactory::make("deinterlace").build()?;
        // Fixed documented enum nicknames, never unvalidated environment values.
        element.set_property_from_str("method", method);
        element.set_property_from_str("mode", "auto");
        element.set_property_from_str("fields", "all");
        Ok(element)
    }
}

fn adaptive_method(
    property: Option<gst::glib::ParamSpec>,
) -> Result<gst::glib::Value, gst::glib::BoolError> {
    property
        .and_then(|property| property.downcast::<gst::glib::ParamSpecEnum>().ok())
        .and_then(|property| property.enum_class().to_value_by_nick("adaptive"))
        .ok_or_else(|| gst::glib::bool_error!(
            "VA-API driver does not support adaptive deinterlacing; select another processing mode explicitly"
        ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gst::glib;

    // Test driver-dependent enum validation without loading VA or using a GPU.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, gst::glib::Enum)]
    #[enum_type(name = "NagameTestAdaptiveMethod")]
    enum AdaptiveMethod {
        #[default]
        Bob,
        Adaptive,
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, gst::glib::Enum)]
    #[enum_type(name = "NagameTestBobOnlyMethod")]
    enum BobOnlyMethod {
        #[default]
        Bob,
    }

    #[test]
    fn adaptive_requires_a_driver_advertised_enum_value() {
        let supported = gst::glib::ParamSpecEnum::builder::<AdaptiveMethod>("method").build();
        assert_eq!(
            adaptive_method(Some(supported))
                .unwrap()
                .get::<AdaptiveMethod>()
                .unwrap(),
            AdaptiveMethod::Adaptive
        );
        let unsupported = gst::glib::ParamSpecEnum::builder::<BobOnlyMethod>("method").build();
        assert!(adaptive_method(Some(unsupported)).is_err());
        assert!(adaptive_method(None).is_err());
        let wrong_type = gst::glib::ParamSpecString::builder("method").build();
        assert!(adaptive_method(Some(wrong_type)).is_err());
    }

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
