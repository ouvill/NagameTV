use serde::Serialize;

#[derive(Default, Serialize)]
pub struct Snapshot {
    pub playing: bool,
    /// Native video sink counters; null when unavailable or stopped. These do not
    /// certify Qt presentation, and can reset when playback is restarted.
    pub video_rendered: Option<u64>,
    pub video_dropped: Option<u64>,
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
