//! Qt font geometry stays at the presentation boundary, outside the TS decoder.
use super::ffi;
use cxx_qt_lib::QString;

impl ffi::Player {
    pub fn subtitle_glyph_outline(&self, text: QString, font: ffi::QFont) -> QString {
        // The native helper returns a value and retains no text, font or path.
        ffi::subtitle_outline_path(&text, &font)
    }
}
