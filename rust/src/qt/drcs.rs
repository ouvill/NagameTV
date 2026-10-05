//! Owned DRCS presentation images, shared by Qt Quick and screenshot snapshots.
use crate::features::subtitles::model::{SubtitleCue, SubtitleGlyph};
use cxx_qt_lib::{QColor, QImage, QImageFormat};
use std::collections::HashMap;

const MAX_RENDER_DIMENSION: i32 = 256;
const ITALIC_SLOPE_DIVISOR: usize = 5;
const UNDERLINE_HEIGHT_DIVISOR: i32 = 24;
const MAX_IMAGE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("DRCS display dimensions exceed 1..=256 pixels")]
    Dimensions,
    #[error("DRCS presentation exceeds the image memory budget")]
    Capacity,
    #[error("invalid DRCS presentation color: {0}")]
    Color(String),
    #[error("could not allocate DRCS presentation image")]
    Allocation,
}
#[derive(Default, Debug)]
pub struct Images(HashMap<usize, Variants>);
#[derive(Debug)]
struct Variants {
    normal: QImage,
    outlined: QImage,
}
impl Images {
    pub fn prepare(cue: &SubtitleCue) -> Result<Self, Error> {
        let mut result = Self::default();
        let mut bytes = 0;
        let mut layouts = Vec::new();
        for (index, cell) in cue.cells.iter().enumerate() {
            let SubtitleGlyph::Drcs { bitmap } = &cell.glyph else {
                continue;
            };
            let (w, h) = (cell.glyph_width, cell.glyph_height);
            if !(1..=MAX_RENDER_DIMENSION).contains(&w) || !(1..=MAX_RENDER_DIMENSION).contains(&h)
            {
                return Err(Error::Dimensions);
            }
            let radius = padding(h);
            let pad = radius + i32::from(cell.bold);
            let (width, height) = ((w + 2 * pad) as usize, (h + 2 * pad) as usize);
            bytes += width * height * 4 * 2;
            if bytes > MAX_IMAGE_BYTES {
                return Err(Error::Capacity);
            }
            layouts.push((index, cell, bitmap, w, h, pad, radius, width, height));
        }
        // Validate the entire screen before allocating/rasterizing any image.
        for (index, cell, bitmap, w, h, pad, radius, width, height) in layouts {
            let mut mask = vec![0; width * height];
            for y in 0..h as usize {
                for x in 0..w as usize {
                    // Fit an italic shear inside the broadcast-authored glyph box.
                    let shift = if cell.italic {
                        ((h as usize - 1 - y) / ITALIC_SLOPE_DIVISOR).min(w as usize - 1)
                    } else {
                        0
                    };
                    let source_x = if cell.italic {
                        let ink_width = (w as usize)
                            .saturating_sub(h as usize / ITALIC_SLOPE_DIVISOR)
                            .max(1);
                        if x < shift || x - shift >= ink_width {
                            continue;
                        }
                        (x - shift) * usize::from(bitmap.width()) / ink_width
                    } else {
                        x * usize::from(bitmap.width()) / w as usize
                    };
                    let source_y = y * usize::from(bitmap.height()) / h as usize;
                    mask[(y + pad as usize) * width + x + pad as usize] =
                        bitmap.pixels()[source_y * usize::from(bitmap.width()) + source_x];
                }
            }
            if cell.bold {
                mask = dilate(&mask, width, height, 1, 0);
            }
            if cell.underline {
                for y in (pad + h - (h / UNDERLINE_HEIGHT_DIVISOR).max(1))..(pad + h) {
                    mask[y as usize * width + pad as usize
                        ..y as usize * width + (pad + w) as usize]
                        .fill(255);
                }
            }
            let outline = dilate(&mask, width, height, radius as usize, radius as usize);
            let foreground = color(&cell.foreground)?;
            let stroke = if cell.stroked {
                color(&cell.stroke)?
            } else {
                QColor::from_rgb(0, 0, 0)
            };
            let outlined = colored(&mask, &outline, width, height, &foreground, &stroke)?;
            let normal = if cell.stroked {
                super::ffi::share_screenshot_image(&outlined)
            } else {
                colored(
                    &mask,
                    &vec![0; mask.len()],
                    width,
                    height,
                    &foreground,
                    &stroke,
                )?
            };
            result.0.insert(index, Variants { normal, outlined });
        }
        Ok(result)
    }
    /// Absence is normal for text cells or a released presentation.
    pub fn get(&self, index: usize, force_outline: bool) -> Option<QImage> {
        self.0.get(&index).map(|images| {
            super::ffi::share_screenshot_image(if force_outline {
                &images.outlined
            } else {
                &images.normal
            })
        })
    }
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
pub fn padding(height: i32) -> i32 {
    (f64::from(height) * 0.06).ceil().max(1.0) as i32
}
fn color(text: &str) -> Result<QColor, Error> {
    QColor::try_from(text).map_err(|_| Error::Color(text.to_owned()))
}
fn colored(
    mask: &[u8],
    outline: &[u8],
    width: usize,
    height: usize,
    foreground: &QColor,
    stroke: &QColor,
) -> Result<QImage, Error> {
    let mut rgba = Vec::with_capacity(width * height * 4);
    for (&coverage, &edge) in mask.iter().zip(outline) {
        let a = u32::from(coverage) * foreground.alpha() as u32 / 255;
        let b = u32::from(edge) * stroke.alpha() as u32 / 255;
        // Source-over in premultiplied RGBA; opaque mask values stay opaque.
        for (front, back) in [
            (foreground.red(), stroke.red()),
            (foreground.green(), stroke.green()),
            (foreground.blue(), stroke.blue()),
        ] {
            rgba.push(((front as u32 * a + back as u32 * b * (255 - a) / 255 + 127) / 255) as u8);
        }
        rgba.push((a + (b * (255 - a) + 127) / 255) as u8);
    }
    // SAFETY: Owned tightly packed RGBA data, exactly width*height*4 bytes.
    let image = unsafe {
        QImage::from_raw_bytes(
            rgba,
            width as i32,
            height as i32,
            QImageFormat::Format_RGBA8888_Premultiplied,
        )
    };
    if image.is_null() {
        Err(Error::Allocation)
    } else {
        Ok(image)
    }
}
// Separable square dilation, O(pixels * radius), bounded to a 16-pixel radius.
fn dilate(mask: &[u8], width: usize, height: usize, rx: usize, ry: usize) -> Vec<u8> {
    let mut horizontal = vec![0; mask.len()];
    for y in 0..height {
        for x in 0..width {
            horizontal[y * width + x] = *mask
                [y * width + x.saturating_sub(rx)..=y * width + (x + rx).min(width - 1)]
                .iter()
                .max()
                .expect("nonempty neighborhood");
        }
    }
    let mut result = vec![0; mask.len()];
    for y in 0..height {
        for x in 0..width {
            result[y * width + x] = (y.saturating_sub(ry)..=(y + ry).min(height - 1))
                .map(|row| horizontal[row * width + x])
                .max()
                .expect("nonempty neighborhood");
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::subtitles::drcs_test_cue;
    #[test]
    fn image_mask_color_outline_and_immutable_snapshot() {
        let mut cue = drcs_test_cue(0x90, false);
        let cell = &mut cue.cells[0];
        cell.glyph_width = 20;
        cell.glyph_height = 20;
        cell.foreground = "#80ff0000".into();
        let images = Images::prepare(&cue).unwrap();
        let image = images.get(0, false).unwrap();
        assert_eq!((image.width(), image.height()), (24, 24));
        assert_eq!(image.pixel_color(5, 5).alpha(), 128);
        assert_eq!(image.pixel_color(18, 5).alpha(), 0);
        assert_eq!(image.pixel_color(0, 5).alpha(), 0);
        assert_eq!(images.get(0, true).unwrap().pixel_color(0, 5).alpha(), 255);
        drop(images);
        assert_eq!(image.pixel_color(5, 5).red(), 255);
        assert!(Images::default().is_empty());
        cue.cells[0].stroked = true;
        cue.cells[0].stroke = "#ff00ff00".into();
        let images = Images::prepare(&cue).unwrap();
        assert_eq!(images.get(0, false).unwrap(), images.get(0, true).unwrap());
        assert_eq!(images.get(0, false).unwrap().pixel_color(0, 5).green(), 255);
    }
    #[test]
    fn raster_preserves_grayscale_and_underlines() {
        let mut cue = drcs_test_cue(0, false);
        cue.cells[0].glyph = SubtitleGlyph::Drcs {
            bitmap: std::sync::Arc::new(
                libaribcaption::Drcs::from_packed(2, 2, 4, 2, &[0b00_01_10_11]).unwrap(),
            ),
        };
        cue.cells[0].glyph_width = 20;
        cue.cells[0].glyph_height = 20;
        cue.cells[0].foreground = "#ffffffff".into();
        let images = Images::prepare(&cue).unwrap();
        let image = images.get(0, false).unwrap();
        assert_eq!(image.pixel_color(5, 5).alpha(), 0);
        assert_eq!(image.pixel_color(18, 5).alpha(), 85);
        assert_eq!(image.pixel_color(5, 18).alpha(), 170);
        assert_eq!(image.pixel_color(18, 18).alpha(), 255);
        cue.cells[0].underline = true;
        let image = Images::prepare(&cue).unwrap().get(0, false).unwrap();
        assert_eq!(image.pixel_color(5, 21).alpha(), 255);
    }
    #[test]
    fn rejects_oversized_images_before_allocating() {
        let mut cue = drcs_test_cue(0x90, false);
        cue.cells[0].glyph_width = i32::MAX;
        assert!(matches!(Images::prepare(&cue), Err(Error::Dimensions)));
        cue.cells.clear();
        for _ in 0..26 {
            let mut cell = drcs_test_cue(0x90, false).cells.remove(0);
            cell.glyph_width = 256;
            cell.glyph_height = 256;
            cue.cells.push(cell);
        }
        assert!(matches!(Images::prepare(&cue), Err(Error::Capacity)));
    }
}
