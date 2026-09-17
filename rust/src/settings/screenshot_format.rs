use serde::{Deserialize, Serialize};

/// Stable file/UI keys; callers cannot construct an unsupported encoder.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScreenshotFormat {
    #[default]
    Png,
    Jpg,
    Webp,
}

impl ScreenshotFormat {
    pub fn key(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpg => "jpg",
            Self::Webp => "webp",
        }
    }
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "png" => Some(Self::Png),
            "jpg" => Some(Self::Jpg),
            "webp" => Some(Self::Webp),
            _ => None,
        }
    }
    pub fn encoding(self, options: ScreenshotOptions) -> Encoding {
        match self {
            Self::Png => Encoding::Png(options.png_compression),
            Self::Jpg => Encoding::Jpg(options.jpg_quality),
            Self::Webp => match options.webp_mode {
                WebpMode::Lossy => Encoding::WebpLossy(options.webp_quality),
                WebpMode::Lossless => Encoding::WebpLossless,
            },
        }
    }
}

macro_rules! bounded_parameter {
    ($name:ident, $min:literal, $max:literal, $default:literal) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(try_from = "i32", into = "i32")]
        pub struct $name(i32);
        impl $name {
            pub const MIN: i32 = $min;
            pub const MAX: i32 = $max;
            pub fn value(self) -> i32 {
                self.0
            }
        }
        impl TryFrom<i32> for $name {
            type Error = &'static str;
            fn try_from(value: i32) -> Result<Self, Self::Error> {
                (Self::MIN..=Self::MAX)
                    .contains(&value)
                    .then_some(Self(value))
                    .ok_or(concat!(stringify!($name), " out of range"))
            }
        }
        impl From<$name> for i32 {
            fn from(value: $name) -> Self {
                value.0
            }
        }
        impl Default for $name {
            fn default() -> Self {
                Self($default)
            }
        }
    };
}

bounded_parameter!(PngCompression, 0, 9, 6);
bounded_parameter!(JpgQuality, 1, 100, 90);
// Qt's WebP plugin reserves quality=100 for lossless encoding. Keep that mode
// explicit instead of silently switching formats at the slider's upper end.
bounded_parameter!(WebpQuality, 1, 99, 90);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WebpMode {
    #[default]
    Lossy,
    Lossless,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScreenshotOptions {
    pub png_compression: PngCompression,
    pub jpg_quality: JpgQuality,
    pub webp_quality: WebpQuality,
    pub webp_mode: WebpMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Png(PngCompression),
    Jpg(JpgQuality),
    WebpLossy(WebpQuality),
    WebpLossless,
}
impl Encoding {
    const QT_WEBP_LOSSLESS: i32 = 100;
    const QT_PNG_SCALE: i32 = 91;
    pub fn format(self) -> ScreenshotFormat {
        match self {
            Self::Png(_) => ScreenshotFormat::Png,
            Self::Jpg(_) => ScreenshotFormat::Jpg,
            Self::WebpLossy(_) | Self::WebpLossless => ScreenshotFormat::Webp,
        }
    }
    pub fn quality(self) -> i32 {
        match self {
            Self::Png(_) => -1,
            Self::Jpg(quality) => quality.value(),
            Self::WebpLossy(quality) => quality.value(),
            Self::WebpLossless => Self::QT_WEBP_LOSSLESS,
        }
    }
    pub fn compression(self) -> i32 {
        match self {
            // QPNGHandler maps [0,100] to [0,9] using (value * 9) / 91.
            // Round upwards to select the requested libpng level exactly.
            Self::Png(level) => {
                (level.value() * Self::QT_PNG_SCALE + PngCompression::MAX - 1) / PngCompression::MAX
            }
            Self::Jpg(_) | Self::WebpLossy(_) | Self::WebpLossless => -1,
        }
    }
}

impl From<ScreenshotFormat> for Encoding {
    fn from(format: ScreenshotFormat) -> Self {
        format.encoding(ScreenshotOptions::default())
    }
}
