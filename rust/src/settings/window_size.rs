//! Window dimensions are Qt logical pixels, independent of display pixel density.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "StoredSize", into = "StoredSize")]
pub struct WindowSize {
    width: i32,
    height: i32,
}

#[derive(Serialize, Deserialize)]
struct StoredSize {
    width: i32,
    height: i32,
}

impl WindowSize {
    pub const MINIMUM: Self = Self {
        width: 640,
        height: 360,
    };
    const INITIAL_LIMIT: Self = Self {
        width: 1280,
        height: 720,
    };
    const INITIAL_SCREEN_FRACTION: f64 = 0.8;
    const ASPECT_WIDTH: i32 = 16;
    const ASPECT_HEIGHT: i32 = 9;

    pub fn checked(width: i32, height: i32) -> Option<Self> {
        (width > 0 && height > 0).then_some(Self { width, height })
    }

    pub fn width(self) -> i32 {
        self.width
    }
    pub fn height(self) -> i32 {
        self.height
    }

    /// A validated work area is required before calculating startup geometry.
    /// Even the minimum shrinks on screens too small to contain 640 x 360.
    pub fn minimum_in(self) -> Self {
        let units = (self.width / Self::ASPECT_WIDTH)
            .min(self.height / Self::ASPECT_HEIGHT)
            .min(Self::MINIMUM.width / Self::ASPECT_WIDTH);
        if units > 0 {
            Self {
                width: units * Self::ASPECT_WIDTH,
                height: units * Self::ASPECT_HEIGHT,
            }
        } else {
            // Sub-16x9 work areas cannot represent an integer 16:9 rectangle.
            Self::MINIMUM.fit_in(self)
        }
    }

    pub fn startup_in(self, saved: Option<Self>) -> Self {
        let minimum = self.minimum_in();
        match saved {
            Some(saved) => {
                let fitted = saved.fit_in(self);
                Self {
                    width: fitted.width.max(minimum.width),
                    height: fitted.height.max(minimum.height),
                }
            }
            None => {
                // Whole aspect-ratio units keep the initial size exactly 16:9.
                let units = ((f64::from(self.width()) * Self::INITIAL_SCREEN_FRACTION
                    / f64::from(Self::ASPECT_WIDTH))
                .min(
                    f64::from(self.height()) * Self::INITIAL_SCREEN_FRACTION
                        / f64::from(Self::ASPECT_HEIGHT),
                )
                .floor() as i32)
                    .min(Self::INITIAL_LIMIT.width / Self::ASPECT_WIDTH);
                Self {
                    width: (units * Self::ASPECT_WIDTH).max(minimum.width),
                    height: (units * Self::ASPECT_HEIGHT).max(minimum.height),
                }
            }
        }
    }

    fn fit_in(self, available: Self) -> Self {
        let scale = (f64::from(available.width) / f64::from(self.width))
            .min(f64::from(available.height) / f64::from(self.height))
            .min(1.0);
        Self {
            width: (f64::from(self.width) * scale).floor().max(1.0) as i32,
            height: (f64::from(self.height) * scale).floor().max(1.0) as i32,
        }
    }
}

impl TryFrom<StoredSize> for WindowSize {
    type Error = &'static str;
    fn try_from(value: StoredSize) -> Result<Self, Self::Error> {
        Self::checked(value.width, value.height)
            .ok_or("window_size must contain positive dimensions")
    }
}

impl From<WindowSize> for StoredSize {
    fn from(value: WindowSize) -> Self {
        Self {
            width: value.width,
            height: value.height,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_size_fits_the_logical_work_area_at_sixteen_to_nine() {
        for (available, expected) in [
            ((1920, 1040), (1280, 720)),
            ((1366, 728), (1024, 576)),
            ((960, 500), (704, 396)),
            ((800, 450), (640, 360)),
            ((800, 350), (608, 342)),
        ] {
            let available = WindowSize::checked(available.0, available.1).unwrap();
            let initial = available.startup_in(None);
            assert_eq!((initial.width(), initial.height()), expected);
            assert_eq!(initial.width() * 9, initial.height() * 16);
            assert!(initial.width() <= available.width() && initial.height() <= available.height());
        }
    }

    #[test]
    fn restored_size_preserves_free_aspect_and_fits_a_smaller_screen() {
        let saved = WindowSize::checked(1000, 700).unwrap();
        let desktop = WindowSize::checked(1920, 1040).unwrap();
        assert_eq!(desktop.startup_in(Some(saved)), saved);
        let laptop = WindowSize::checked(960, 600).unwrap();
        assert_eq!(
            laptop.startup_in(Some(saved)),
            WindowSize::checked(857, 600).unwrap()
        );
        let tiny = WindowSize::checked(480, 270).unwrap();
        assert_eq!(tiny.startup_in(None), tiny);
        assert_eq!(tiny.minimum_in(), tiny);
        let too_small = WindowSize::checked(1, 1).unwrap();
        assert_eq!(desktop.startup_in(Some(too_small)), WindowSize::MINIMUM);
    }
}
