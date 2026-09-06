use serde::Serialize;

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

    /// A clear-screen flag can accompany a new caption.  It means "replace the
    /// previous screen", not "discard this caption".  Only an empty cue is a
    /// request to clear without presenting a replacement.
    pub fn is_clear_only(&self) -> bool {
        self.clear_screen && self.text.is_empty() && self.cells.is_empty()
    }
}

/// One ARIB character cell with its broadcast presentation attributes.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleCell {
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
