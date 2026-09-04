use super::model::{SubtitleCell, SubtitleCue};
use std::ffi::{CStr, c_char, c_void};
use std::{ptr, slice};

#[repr(C)]
struct Context {
    _private: [u8; 0],
}
#[repr(C)]
struct Decoder {
    _private: [u8; 0],
}

#[repr(C)]
struct CaptionChar {
    kind: i32,
    codepoint: u32,
    pua_codepoint: u32,
    drcs_code: u32,
    x: i32,
    y: i32,
    char_width: i32,
    char_height: i32,
    horizontal_spacing: i32,
    vertical_spacing: i32,
    horizontal_scale: f32,
    vertical_scale: f32,
    text_color: u32,
    back_color: u32,
    stroke_color: u32,
    style: i32,
    enclosure_style: i32,
    u8str: [c_char; 8],
}

#[repr(C)]
struct CaptionRegion {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    is_ruby: bool,
    chars: *mut CaptionChar,
    char_count: u32,
}

#[repr(C)]
struct Caption {
    kind: i32,
    flags: i32,
    language: u32,
    text: *mut c_char,
    regions: *mut CaptionRegion,
    region_count: u32,
    drcs_map: *mut c_void,
    pts: i64,
    wait_duration: i64,
    plane_width: i32,
    plane_height: i32,
    has_builtin_sound: bool,
    builtin_sound_id: u8,
}

#[link(name = "aribcaption")]
unsafe extern "C" {
    fn aribcc_context_alloc() -> *mut Context;
    fn aribcc_context_free(context: *mut Context);
    fn aribcc_decoder_alloc(context: *mut Context) -> *mut Decoder;
    fn aribcc_decoder_free(decoder: *mut Decoder);
    fn aribcc_decoder_initialize(
        decoder: *mut Decoder,
        encoding: i32,
        kind: i32,
        profile: i32,
        language: i32,
    ) -> bool;
    fn aribcc_decoder_decode(
        decoder: *mut Decoder,
        data: *const u8,
        length: usize,
        pts: i64,
        caption: *mut Caption,
    ) -> i32;
    fn aribcc_caption_cleanup(caption: *mut Caption);
    fn aribcc_caption_char_get_section_width(character: *mut CaptionChar) -> i32;
    fn aribcc_caption_char_get_section_height(character: *mut CaptionChar) -> i32;
}

pub(super) struct AribDecoder {
    context: *mut Context,
    decoder: *mut Decoder,
}
unsafe impl Send for AribDecoder {}

impl AribDecoder {
    pub(super) fn new() -> Option<Self> {
        unsafe {
            let context = aribcc_context_alloc();
            if context.is_null() {
                return None;
            }
            let decoder = aribcc_decoder_alloc(context);
            if decoder.is_null() || !aribcc_decoder_initialize(decoder, 1, 0x80, 0x0008, 1) {
                if !decoder.is_null() {
                    aribcc_decoder_free(decoder);
                }
                aribcc_context_free(context);
                return None;
            }
            Some(Self { context, decoder })
        }
    }

    pub(super) fn decode_pes(&mut self, pes: &[u8], pts_ms: i64) -> Option<SubtitleCue> {
        unsafe {
            let mut caption: Caption = std::mem::zeroed();
            if aribcc_decoder_decode(self.decoder, pes.as_ptr(), pes.len(), pts_ms, &mut caption)
                != 2
            {
                return None;
            }
            let result = self.convert(&mut caption);
            aribcc_caption_cleanup(&mut caption);
            result
        }
    }

    unsafe fn convert(&self, caption: &mut Caption) -> Option<SubtitleCue> {
        let text = if caption.text.is_null() {
            String::new()
        } else {
            unsafe { CStr::from_ptr(caption.text) }
                .to_string_lossy()
                .into_owned()
        };
        let mut cells = Vec::new();
        if !caption.regions.is_null() {
            for region in
                unsafe { slice::from_raw_parts_mut(caption.regions, caption.region_count as usize) }
            {
                if region.chars.is_null() {
                    continue;
                }
                for character in
                    unsafe { slice::from_raw_parts_mut(region.chars, region.char_count as usize) }
                {
                    let glyph = unsafe { CStr::from_ptr(character.u8str.as_ptr()) }
                        .to_string_lossy()
                        .into_owned();
                    if glyph.is_empty() {
                        continue;
                    }
                    cells.push(SubtitleCell {
                        text: glyph,
                        x: character.x,
                        y: character.y,
                        width: unsafe { aribcc_caption_char_get_section_width(character) }.max(1),
                        height: unsafe { aribcc_caption_char_get_section_height(character) }.max(1),
                        glyph_width: (character.char_width as f32 * character.horizontal_scale)
                            .round() as i32,
                        glyph_height: (character.char_height as f32 * character.vertical_scale)
                            .round() as i32,
                        foreground: rgba(character.text_color),
                        background: rgba(character.back_color),
                        stroke: rgba(character.stroke_color),
                        bold: character.style & 1 != 0,
                        italic: character.style & 2 != 0,
                        underline: character.style & 4 != 0,
                        stroked: character.style & 8 != 0,
                        ruby: region.is_ruby,
                    });
                }
            }
        }
        Some(SubtitleCue {
            text,
            duration_ms: if caption.wait_duration > 0 && caption.wait_duration != i64::MAX {
                caption.wait_duration as u64
            } else {
                7000
            },
            clear_screen: caption.flags & 1 != 0,
            plane_width: caption.plane_width,
            plane_height: caption.plane_height,
            cells,
        })
    }
}

fn rgba(color: u32) -> String {
    let r = color & 0xff;
    let g = (color >> 8) & 0xff;
    let b = (color >> 16) & 0xff;
    let a = (color >> 24) & 0xff;
    format!("#{a:02x}{r:02x}{g:02x}{b:02x}")
}

impl Drop for AribDecoder {
    fn drop(&mut self) {
        unsafe {
            if !self.decoder.is_null() {
                aribcc_decoder_free(self.decoder);
            }
            if !self.context.is_null() {
                aribcc_context_free(self.context);
            }
            self.decoder = ptr::null_mut();
            self.context = ptr::null_mut();
        }
    }
}
