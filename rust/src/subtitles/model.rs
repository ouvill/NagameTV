use serde::Serialize;

/// A decoded ARIB caption screen ready to be transferred to Qt.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleCue {
    pub text: String,
    pub duration_ms: u64,
    pub clear_screen: bool,
    pub plane_width: i32,
    pub plane_height: i32,
    pub cells: Vec<SubtitleCell>,
}

impl SubtitleCue {
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
pub(crate) struct SubtitleCell {
    pub(crate) text: String,
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) width: i32,
    pub(crate) height: i32,
    pub(crate) glyph_width: i32,
    pub(crate) glyph_height: i32,
    pub(crate) foreground: String,
    pub(crate) background: String,
    pub(crate) stroke: String,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    pub(crate) underline: bool,
    pub(crate) stroked: bool,
    pub(crate) ruby: bool,
}

#[cfg(test)]
mod tests {
    use super::SubtitleCue;

    fn cue(text: &str, clear_screen: bool) -> SubtitleCue {
        SubtitleCue {
            text: text.to_owned(),
            duration_ms: 7000,
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
