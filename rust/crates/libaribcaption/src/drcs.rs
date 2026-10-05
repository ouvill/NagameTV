use crate::{Error, sys};

// Application-facing decoded masks are bounded even for many distinct glyphs.
pub(crate) const MAX_CAPTION_MASK_BYTES: usize = 1024 * 1024;
const MAX_DIMENSION: usize = 255; // STD-B24 bitmap dimensions are eight-bit fields.

/// Validated, immutable eight-bit coverage mask. Construction checks the packed
/// representation before any caller can use its dimensions to index pixels.
#[derive(Debug)]
pub struct Drcs {
    width: u16,
    height: u16,
    pixels: Box<[u8]>,
}
impl Drcs {
    pub fn from_packed(
        width: usize,
        height: usize,
        depth: u16,
        bits: u8,
        data: &[u8],
    ) -> Result<Self, Error> {
        if !(1..=MAX_DIMENSION).contains(&width)
            || !(1..=MAX_DIMENSION).contains(&height)
            || !(2..=256).contains(&depth)
            || bits != (u16::BITS - (depth - 1).leading_zeros()) as u8
        {
            return Err(Error::InvalidDrcs);
        }
        let count = width * height;
        if data.len() != (count * usize::from(bits)).div_ceil(8) {
            return Err(Error::InvalidDrcs);
        }
        let mut pixels = Vec::with_capacity(count);
        for pixel in 0..count {
            let mut value = 0u16;
            // Pixels are a continuous MSB-first stream; samples may cross bytes.
            for bit in 0..usize::from(bits) {
                let offset = pixel * usize::from(bits) + bit;
                value = (value << 1) | u16::from((data[offset / 8] >> (7 - offset % 8)) & 1);
            }
            if value >= depth {
                return Err(Error::InvalidDrcs);
            }
            pixels.push((value * 255 / (depth - 1)) as u8);
        }
        Ok(Self {
            width: width as u16,
            height: height as u16,
            pixels: pixels.into_boxed_slice(),
        })
    }
    pub fn width(&self) -> u16 {
        self.width
    }
    pub fn height(&self) -> u16 {
        self.height
    }
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
}

// Called only while NativeCaption owns the native map and its glyphs.
pub(crate) fn copy_native(map: *mut sys::aribcc_drcsmap_t, code: u32) -> Result<Drcs, Error> {
    if map.is_null() {
        return Err(Error::InvalidDrcs);
    }
    // SAFETY: The successful decode owns this map for the duration of copying.
    unsafe {
        let glyph = sys::aribcc_drcsmap_get(map, code);
        if glyph.is_null() {
            return Err(Error::InvalidDrcs);
        }
        let (mut width, mut height, mut depth, mut bits) = (0, 0, 0, 0);
        sys::aribcc_drcs_get_size(glyph, &mut width, &mut height);
        sys::aribcc_drcs_get_depth(glyph, &mut depth, &mut bits);
        let mut pixels = std::ptr::null_mut();
        let mut len = 0;
        sys::aribcc_drcs_get_pixels(glyph, &mut pixels, &mut len);
        if pixels.is_null() || len > MAX_DIMENSION * MAX_DIMENSION {
            return Err(Error::InvalidDrcs);
        }
        Drcs::from_packed(
            usize::try_from(width).map_err(|_| Error::InvalidDrcs)?,
            usize::try_from(height).map_err(|_| Error::InvalidDrcs)?,
            u16::try_from(depth).map_err(|_| Error::InvalidDrcs)?,
            u8::try_from(bits).map_err(|_| Error::InvalidDrcs)?,
            std::slice::from_raw_parts(pixels, len),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn packed_samples_cross_bytes_and_rows_and_preserve_full_opacity() {
        let mask = Drcs::from_packed(2, 2, 8, 3, &[0b0000_0101, 0b0111_0000]).unwrap();
        assert_eq!(mask.pixels(), &[0, 36, 72, 255]);
        assert_eq!(
            Drcs::from_packed(3, 1, 3, 2, &[0b00_01_10_00])
                .unwrap()
                .pixels(),
            &[0, 127, 255]
        );
    }
    #[test]
    fn rejects_invalid_dimensions_depth_length_and_samples() {
        for (w, h, depth, bits, data) in [
            (0, 2, 2, 1, vec![0]),
            (256, 1, 2, 1, vec![0; 32]),
            (2, 2, 1, 1, vec![0]),
            (2, 2, 4, 1, vec![0]),
            (2, 2, 2, 1, vec![]),
            (1, 1, 3, 2, vec![0xc0]),
        ] {
            assert!(Drcs::from_packed(w, h, depth, bits, &data).is_err());
        }
    }
}
