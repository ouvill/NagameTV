use serde::Serialize;

#[derive(Default, Serialize)]
pub struct Snapshot {
    pub playing: bool,
    pub subtitles: bool,
    pub comments: bool,
    pub comments_enabled: bool,
    pub epg_enabled: bool,
    pub guide_open: bool,
    pub channels_open: bool,
    pub live_comments: usize,
    pub channel_count: usize,
    pub epg_programs: usize,
    pub epg_text_capacity_bytes: usize,
    pub comment_history_count: usize,
    pub comment_history_text_capacity_bytes: usize,
    pub subtitle_cells: usize,
    pub subtitle_pending: Option<usize>,
    pub epg_loading: bool,
}
