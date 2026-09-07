//! Ownership-safe Japanese ARIB STD-B24 decoder (JIS, profile A, first language).
//! Input is caption PES payload, not MPEG-TS. Results own their data and have no
//! UI dependency. Rendering and DRCS bitmap export are not exposed yet.

use libaribcaption_sys as sys;
use std::{ffi::CStr, fmt, ptr::NonNull, slice};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Allocation,
    Initialization,
    Decode,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Allocation => "could not allocate ARIB decoder",
            Self::Initialization => "could not initialize ARIB decoder",
            Self::Decode => "could not decode ARIB caption payload",
        })
    }
}
impl std::error::Error for Error {}

/// RGBA components, independent of UI-specific color notation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}
impl Color {
    fn from_native(value: u32) -> Self {
        Self {
            red: value as u8,
            green: (value >> 8) as u8,
            blue: (value >> 16) as u8,
            alpha: (value >> 24) as u8,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Character {
    pub text: String,
    pub x: i32,
    pub y: i32,
    pub section_width: i32,
    pub section_height: i32,
    pub width: i32,
    pub height: i32,
    pub horizontal_scale: f32,
    pub vertical_scale: f32,
    pub foreground: Color,
    pub background: Color,
    pub stroke: Color,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub stroked: bool,
}
#[derive(Debug, Clone)]
pub struct Region {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub ruby: bool,
    pub characters: Vec<Character>,
}
#[derive(Debug, Clone)]
pub struct Caption {
    pub text: String,
    pub pts_ms: i64,
    /// None means indefinite; the application decides presentation policy.
    pub duration_ms: Option<i64>,
    pub clear_screen: bool,
    pub plane_width: i32,
    pub plane_height: i32,
    pub regions: Vec<Region>,
}

pub struct Decoder {
    context: NonNull<sys::aribcc_context_t>,
    decoder: NonNull<sys::aribcc_decoder_t>,
}
// SAFETY: Exclusive ownership, no callbacks, mutation only through &mut self.
// The bundled decoder/context own ordinary C++ data, no thread-affine renderer.
// They may move between threads, but cannot be used concurrently (!Sync).
unsafe impl Send for Decoder {}

impl Decoder {
    pub fn new() -> Result<Self, Error> {
        // SAFETY: Context outlives decoder. Failures release acquired resources;
        // successful initialization transfers ownership to Self's Drop.
        unsafe {
            let context = NonNull::new(sys::aribcc_context_alloc()).ok_or(Error::Allocation)?;
            let Some(decoder) = NonNull::new(sys::aribcc_decoder_alloc(context.as_ptr())) else {
                sys::aribcc_context_free(context.as_ptr());
                return Err(Error::Allocation);
            };
            let owner = Self { context, decoder };
            if !sys::aribcc_decoder_initialize(
                decoder.as_ptr(),
                sys::ARIBCC_ENCODING_SCHEME_ARIB_STD_B24_JIS,
                sys::ARIBCC_CAPTIONTYPE_CAPTION,
                sys::ARIBCC_PROFILE_A,
                sys::ARIBCC_LANGUAGEID_FIRST,
            ) {
                return Err(Error::Initialization);
            }
            Ok(owner)
        }
    }

    /// Decodes one PES payload. A management packet may yield Ok(None).
    pub fn decode(&mut self, payload: &[u8], pts_ms: i64) -> Result<Option<Caption>, Error> {
        if payload.is_empty() {
            return Err(Error::Decode);
        }
        let mut raw = sys::aribcc_caption_t::default();
        // SAFETY: Initialized decoder, valid nonempty input and writable output.
        let status = unsafe {
            sys::aribcc_decoder_decode(
                self.decoder.as_ptr(),
                payload.as_ptr(),
                payload.len(),
                pts_ms,
                &mut raw,
            )
        };
        match status {
            sys::ARIBCC_DECODE_STATUS_GOT_CAPTION => {
                // Guard releases returned allocations even if conversion unwinds.
                let mut caption = NativeCaption(raw);
                Ok(Some(caption.copy_to_owned()))
            }
            sys::ARIBCC_DECODE_STATUS_NO_CAPTION => Ok(None),
            _ => Err(Error::Decode),
        }
    }
    pub fn flush(&mut self) {
        // SAFETY: Exclusively borrowed, initialized decoder.
        unsafe { sys::aribcc_decoder_flush(self.decoder.as_ptr()) }
    }
}
impl Drop for Decoder {
    fn drop(&mut self) {
        // SAFETY: Unique owners freed exactly once, in dependency order.
        unsafe {
            sys::aribcc_decoder_free(self.decoder.as_ptr());
            sys::aribcc_context_free(self.context.as_ptr());
        }
    }
}

struct NativeCaption(sys::aribcc_caption_t);
impl Drop for NativeCaption {
    fn drop(&mut self) {
        // SAFETY: Constructed only from successful native decode. Native fields
        // are never freed separately or exposed through the public API.
        unsafe { sys::aribcc_caption_cleanup(&mut self.0) }
    }
}
impl NativeCaption {
    fn copy_to_owned(&mut self) -> Caption {
        let raw = &mut self.0;
        let text = if raw.text.is_null() {
            String::new()
        } else {
            // SAFETY: Native API guarantees null-terminated text until cleanup.
            unsafe { CStr::from_ptr(raw.text) }
                .to_string_lossy()
                .into_owned()
        };
        let mut regions = Vec::new();
        if !raw.regions.is_null() && raw.region_count > 0 {
            // SAFETY: Pointer/count pairs belong to this successful native
            // caption, remain alive during conversion, and have no Rust aliases.
            for region in
                unsafe { slice::from_raw_parts_mut(raw.regions, raw.region_count as usize) }
            {
                let mut characters = Vec::new();
                if !region.chars.is_null() && region.char_count > 0 {
                    // SAFETY: Same native ownership guarantee as above.
                    for ch in unsafe {
                        slice::from_raw_parts_mut(region.chars, region.char_count as usize)
                    } {
                        // The native ABI stores UTF-8 in char[8]. Convert on the
                        // stack, preserving bytes even where C char is signed.
                        // Only the returned owned String needs a heap allocation.
                        let bytes = ch.u8str.map(|byte| byte as u8);
                        let length = bytes
                            .iter()
                            .position(|byte| *byte == 0)
                            .unwrap_or(bytes.len());
                        characters.push(Character {
                            text: String::from_utf8_lossy(&bytes[..length]).into_owned(),
                            x: ch.x,
                            y: ch.y,
                            // SAFETY: Valid uniquely borrowed native character.
                            section_width: unsafe {
                                sys::aribcc_caption_char_get_section_width(ch)
                            },
                            section_height: unsafe {
                                sys::aribcc_caption_char_get_section_height(ch)
                            },
                            width: ch.char_width,
                            height: ch.char_height,
                            horizontal_scale: ch.char_horizontal_scale,
                            vertical_scale: ch.char_vertical_scale,
                            foreground: Color::from_native(ch.text_color),
                            background: Color::from_native(ch.back_color),
                            stroke: Color::from_native(ch.stroke_color),
                            bold: ch.style & sys::ARIBCC_CHARSTYLE_BOLD != 0,
                            italic: ch.style & sys::ARIBCC_CHARSTYLE_ITALIC != 0,
                            underline: ch.style & sys::ARIBCC_CHARSTYLE_UNDERLINE != 0,
                            stroked: ch.style & sys::ARIBCC_CHARSTYLE_STROKE != 0,
                        });
                    }
                }
                regions.push(Region {
                    x: region.x,
                    y: region.y,
                    width: region.width,
                    height: region.height,
                    ruby: region.is_ruby,
                    characters,
                });
            }
        }
        Caption {
            text,
            pts_ms: raw.pts,
            duration_ms: (raw.wait_duration != sys::ARIBCC_RS_DURATION_INDEFINITE)
                .then_some(raw.wait_duration),
            clear_screen: raw.flags & sys::ARIBCC_CAPTIONFLAGS_CLEARSCREEN != 0,
            plane_width: raw.plane_width,
            plane_height: raw.plane_height,
            regions,
        }
    }
}

#[cfg(test)]
mod tests {
    // In tests, unwrap/expect assert successful setup or an expected result.
    // Failures intentionally fail the test; they are not assumed impossible IO.
    use super::*;
    #[test]
    fn color_preserves_alpha_and_channels() {
        assert_eq!(
            Color::from_native(0x80402010),
            Color {
                red: 0x10,
                green: 0x20,
                blue: 0x40,
                alpha: 0x80
            }
        );
    }
    #[test]
    fn decoder_rejects_empty_and_invalid_payloads() {
        let mut decoder = Decoder::new().unwrap();
        assert!(matches!(decoder.decode(&[], 0), Err(Error::Decode)));
        assert!(matches!(decoder.decode(&[0; 16], 0), Err(Error::Decode)));
        decoder.flush();
    }
    #[test]
    fn decoder_can_move_to_another_thread() {
        let decoder = Decoder::new().unwrap();
        std::thread::spawn(move || {
            let mut decoder = decoder;
            decoder.flush();
        })
        .join()
        .unwrap();
    }
}
