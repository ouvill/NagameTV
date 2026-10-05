use libaribcaption::Drcs;
use serde::Serialize;
use std::sync::Arc;

pub(crate) const MAX_SCREEN_MASK_BYTES: usize = 1024 * 1024;
pub(crate) const MAX_PENDING_MASK_BYTES: usize = 8 * 1024 * 1024;

/// A decoded ARIB caption screen ready to be transferred to Qt.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleCue {
    pub text: String,
    pub pts_ms: Option<i64>,
    pub duration_ms: Option<u64>,
    pub clear_screen: bool,
    pub plane_width: i32,
    pub plane_height: i32,
    pub cells: Vec<SubtitleCell>,
}

impl SubtitleCue {
    #[cfg(test)]
    pub fn clear(pts_ms: i64) -> Self {
        Self {
            text: String::new(),
            pts_ms: Some(pts_ms),
            duration_ms: None,
            clear_screen: true,
            plane_width: 960,
            plane_height: 540,
            cells: Vec::new(),
        }
    }

    /// Conservative retained-mask cost, including repeated cell references.
    pub fn mask_bytes(&self) -> usize {
        self.cells
            .iter()
            .map(|cell| match &cell.glyph {
                SubtitleGlyph::Text => 0,
                SubtitleGlyph::Drcs { bitmap } => bitmap.pixels().len(),
            })
            .sum()
    }

    /// A clear-screen flag can accompany a new caption.  It means "replace the
    /// previous screen", not "discard this caption".  Only an empty cue is a
    /// request to clear without presenting a replacement.
    pub fn is_clear_only(&self) -> bool {
        self.clear_screen && self.text.is_empty() && self.cells.is_empty()
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SubtitleGlyph {
    Text,
    Drcs {
        #[serde(skip)]
        bitmap: Arc<Drcs>,
    },
}

/// One ARIB character cell with its broadcast presentation attributes.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleCell {
    pub glyph: SubtitleGlyph,
    pub text: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub glyph_width: i32,
    pub glyph_height: i32,
    pub foreground: String,
    pub background: String,
    pub stroke: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub stroked: bool,
    pub ruby: bool,
}

#[cfg(test)]
mod tests {
    use super::SubtitleCue;

    fn cue(text: &str, clear_screen: bool) -> SubtitleCue {
        SubtitleCue {
            text: text.to_owned(),
            pts_ms: Some(0),
            duration_ms: None,
            clear_screen,
            plane_width: 960,
            plane_height: 540,
            cells: Vec::new(),
        }
    }

    #[test]
    fn clear_flag_with_text_still_presents_the_replacement_caption() {
        assert!(!cue("まず1点目です。", true).is_clear_only());
        assert!(cue("", true).is_clear_only());
        assert!(!cue("", false).is_clear_only());
    }
}
