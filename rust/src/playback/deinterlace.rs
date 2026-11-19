//! Validated startup policy, independent of the Qt presentation.
use gstreamer::{self as gst, prelude::*};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CpuMethod {
    Yadif,
    GreedyL,
    Linear,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fields {
    All,
    One,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Cpu {
        method: CpuMethod,
        fields: Fields,
    },
    Off,
    /// NVDEC GL textures followed by the single-rate OpenGL filter.
    NvidiaGl,
    /// Linux VA surfaces, with field-rate hardware deinterlacing.
    VaApi,
}
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid NAGAMETV_DEINTERLACE={0:?}; expected yadif, greedyl, linear, off, gl or va")]
    Invalid(String),
    #[error("NAGAMETV_DEINTERLACE is not valid Unicode")]
    NonUnicode,
    #[error("Invalid NAGAMETV_DEINTERLACE_FIELDS={0:?}. Expected all or one")]
    InvalidFields(String),
    #[error("NAGAMETV_DEINTERLACE_FIELDS is not valid Unicode")]
    NonUnicodeFields,
    #[error(
        "NAGAMETV_DEINTERLACE_FIELDS=one requires yadif, greedyl or linear. The selected method is {0}"
    )]
    OneFieldRequiresCpu(&'static str),
}
impl Mode {
    /// Resolve the saved method and startup-only environment overrides.
    pub fn for_launch(saved: crate::settings::Deinterlace) -> Result<Self, Error> {
        let mode = match std::env::var("NAGAMETV_DEINTERLACE") {
            Ok(value) => Self::parse_method(&value)?,
            Err(std::env::VarError::NotPresent) => saved.into(),
            Err(std::env::VarError::NotUnicode(_)) => return Err(Error::NonUnicode),
        };
        match std::env::var("NAGAMETV_DEINTERLACE_FIELDS") {
            Ok(value) => mode.with_fields(&value),
            Err(std::env::VarError::NotPresent) => Ok(mode),
            Err(std::env::VarError::NotUnicode(_)) => Err(Error::NonUnicodeFields),
        }
    }

    fn cpu(method: CpuMethod) -> Self {
        Self::Cpu {
            method,
            fields: Fields::All,
        }
    }

    fn parse_method(value: &str) -> Result<Self, Error> {
        match value.trim().to_ascii_lowercase().as_str() {
            "yadif" | "quality" => Ok(Self::cpu(CpuMethod::Yadif)),
            "greedyl" => Ok(Self::cpu(CpuMethod::GreedyL)),
            "linear" | "balanced" => Ok(Self::cpu(CpuMethod::Linear)),
            "off" | "disabled" => Ok(Self::Off),
            "gl" => Ok(Self::NvidiaGl),
            "va" => Ok(Self::VaApi),
            _ => Err(Error::Invalid(value.to_owned())),
        }
    }
    fn with_fields(self, value: &str) -> Result<Self, Error> {
        let fields = match value.trim().to_ascii_lowercase().as_str() {
            "all" => Fields::All,
            "one" => Fields::One,
            _ => return Err(Error::InvalidFields(value.to_owned())),
        };
        match (self, fields) {
            (Self::Cpu { method, .. }, fields) => Ok(Self::Cpu { method, fields }),
            (mode, Fields::All) => Ok(mode),
            (Self::Off, Fields::One) => Err(Error::OneFieldRequiresCpu("off")),
            (Self::NvidiaGl, Fields::One) => Err(Error::OneFieldRequiresCpu("gl")),
            (Self::VaApi, Fields::One) => Err(Error::OneFieldRequiresCpu("va")),
        }
    }

    /// CPU modes keep GStreamer's automatic decoder selection.
    pub fn automatic_decoders(self) -> bool {
        match self {
            Self::Cpu { .. } | Self::Off => true,
            Self::NvidiaGl | Self::VaApi => false,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Cpu { method, fields } => match (method, fields) {
                (CpuMethod::Yadif, Fields::All) => "YADIF (auto / all fields)",
                (CpuMethod::GreedyL, Fields::All) => "GreedyL (auto / all fields)",
                (CpuMethod::Linear, Fields::All) => "Linear (auto / all fields)",
                (CpuMethod::Yadif, Fields::One) => "YADIF (auto / one field)",
                (CpuMethod::GreedyL, Fields::One) => "GreedyL (auto / one field)",
                (CpuMethod::Linear, Fields::One) => "Linear (auto / one field)",
            },
            Self::Off => "Off",
            Self::NvidiaGl => "NVDEC + OpenGL vfir (single rate)",
            Self::VaApi => "VA-API adaptive (all fields / NV12)",
        }
    }
    pub fn build(self) -> Result<gst::Element, gst::glib::BoolError> {
        let (method, fields) = match self {
            Self::Off => return gst::ElementFactory::make("identity").build(),
            Self::Cpu { method, fields } => {
                let method = match method {
                    CpuMethod::Yadif => "yadif",
                    CpuMethod::GreedyL => "greedyl",
                    CpuMethod::Linear => "linear",
                };
                let fields = match fields {
                    Fields::All => "all",
                    // ARIB broadcasts normally carry the top field first.
                    // `bottom` would take the later sample, and `auto` can
                    // negotiate both fields and incur field-rate processing.
                    Fields::One => "top",
                };
                (method, fields)
            }
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
        element.set_property_from_str("fields", fields);
        Ok(element)
    }
}

impl Default for Mode {
    fn default() -> Self {
        crate::settings::Deinterlace::default().into()
    }
}

impl From<crate::settings::Deinterlace> for Mode {
    fn from(saved: crate::settings::Deinterlace) -> Self {
        use crate::settings::Deinterlace;
        match saved {
            Deinterlace::Yadif => Self::cpu(CpuMethod::Yadif),
            Deinterlace::GreedyL => Self::cpu(CpuMethod::GreedyL),
            Deinterlace::Linear => Self::cpu(CpuMethod::Linear),
            Deinterlace::Off => Self::Off,
        }
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
            (" YADIF ", Mode::cpu(CpuMethod::Yadif), "deinterlace"),
            ("quality", Mode::cpu(CpuMethod::Yadif), "deinterlace"),
            ("GreedyL", Mode::cpu(CpuMethod::GreedyL), "deinterlace"),
            ("balanced", Mode::cpu(CpuMethod::Linear), "deinterlace"),
            ("linear", Mode::cpu(CpuMethod::Linear), "deinterlace"),
            ("disabled", Mode::Off, "identity"),
            ("off", Mode::Off, "identity"),
        ] {
            let mode = Mode::parse_method(input)?;
            assert_eq!(mode, expected);
            let element = mode.build()?;
            assert_eq!(element.factory().ok_or("missing factory")?.name(), factory);
        }
        assert!(matches!(
            Mode::parse_method("unknown"),
            Err(Error::Invalid(_))
        ));
        assert!(Mode::parse_method("").is_err());
        Ok(())
    }

    #[test]
    fn field_override_validates_the_effective_method() -> Result<(), Error> {
        use crate::settings::Deinterlace;
        for (name, mode) in [
            ("off", Mode::Off),
            ("gl", Mode::NvidiaGl),
            ("va", Mode::VaApi),
        ] {
            assert!(
                matches!(mode.with_fields("one"), Err(Error::OneFieldRequiresCpu(method)) if method == name)
            );
            assert_eq!(mode.with_fields("all")?, mode);
        }
        assert!(matches!(
            Mode::from(Deinterlace::Off).with_fields("one"),
            Err(Error::OneFieldRequiresCpu("off"))
        ));
        for (saved, method) in [
            (Deinterlace::Yadif, CpuMethod::Yadif),
            (Deinterlace::GreedyL, CpuMethod::GreedyL),
            (Deinterlace::Linear, CpuMethod::Linear),
        ] {
            let mode = Mode::from(saved);
            assert_eq!(
                mode.with_fields("all")?,
                Mode::Cpu {
                    method,
                    fields: Fields::All
                }
            );
            assert_eq!(
                mode.with_fields(" ONE ")?,
                Mode::Cpu {
                    method,
                    fields: Fields::One
                }
            );
            for value in ["", "top", "auto"] {
                assert!(
                    matches!(mode.with_fields(value), Err(Error::InvalidFields(input)) if input == value)
                );
            }
        }
        Ok(())
    }
}
