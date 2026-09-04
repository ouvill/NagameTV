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
