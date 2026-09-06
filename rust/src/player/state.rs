//! Qt-owned presentation fields. Application state is owned by Runtime.
use crate::runtime::Runtime;
use cxx_qt_lib::{QString, QStringList};
use viewer_core::app::State;

pub struct PlayerRust {
    pub(super) server: QString,
    pub(super) language: QString,
    pub(super) ui_language: QString,
    pub(super) service_id: QString,
    pub(super) status: QString,
    pub(super) playback_error: QString,
    pub(super) playback_error_details: QString,
    pub(super) playing: bool,
    pub(super) volume: f64,
    pub(super) audio_muted: bool,
    pub(super) audio_tracks: QString,
    pub(super) audio_error: QString,
    pub(super) danmaku_enabled: bool,
    pub(super) comment_font_size: f64,
    pub(super) comment_opacity: f64,
    pub(super) comment_speed: f64,
    pub(super) subtitles_enabled: bool,
    pub(super) subtitle_text: QString,
    pub(super) subtitle_data: QString,
    pub(super) autoplay: bool,
    pub(super) channel_name: QString,
    pub(super) program_name: QString,
    pub(super) program_description: QString,
    pub(super) channel_logo_url: QString,
    pub(super) program_progress: f64,
    pub(super) services: QStringList,
    pub(super) program_titles: QStringList,
    pub(super) program_descriptions: QStringList,
    pub(super) program_starts: QStringList,
    pub(super) program_durations: QStringList,
    pub(super) channel_logo_urls: QStringList,
    pub(super) channel_types: QStringList,
    pub(super) jikkyo_forces: QStringList,
    pub(super) guide_start: QString,
    pub(super) guide_program_ids: QStringList,
    pub(super) guide_channel_indices: QStringList,
    pub(super) guide_titles: QStringList,
    pub(super) guide_descriptions: QStringList,
    pub(super) guide_starts: QStringList,
    pub(super) guide_durations: QStringList,
    pub(super) guide_genres: QStringList,
    pub(super) comment_times: QStringList,
    pub(super) comment_texts: QStringList,
    pub(super) comment_sources: QStringList,
    pub(super) comment_status: QString,
    pub(super) runtime: Runtime,
    pub(super) rendered: Option<State>,
}
impl Default for PlayerRust {
    fn default() -> Self {
        let directory = Some(std::path::PathBuf::from(
            super::ffi::playback_log_directory().to_string(),
        ));
        let runtime = Runtime::new(super::ffi::current_ui_language().to_string(), directory);
        Self::from_runtime(runtime)
    }
}

impl PlayerRust {
    pub(super) fn from_runtime(runtime: Runtime) -> Self {
        let mut value = Self {
            server: Default::default(),
            language: Default::default(),
            ui_language: Default::default(),
            service_id: Default::default(),
            status: Default::default(),
            playback_error: Default::default(),
            playback_error_details: Default::default(),
            playing: Default::default(),
            volume: Default::default(),
            audio_muted: Default::default(),
            audio_tracks: Default::default(),
            audio_error: Default::default(),
            danmaku_enabled: Default::default(),
            comment_font_size: Default::default(),
            comment_opacity: Default::default(),
            comment_speed: Default::default(),
            subtitles_enabled: Default::default(),
            subtitle_text: Default::default(),
            subtitle_data: Default::default(),
            autoplay: Default::default(),
            channel_name: Default::default(),
            program_name: Default::default(),
            program_description: Default::default(),
            channel_logo_url: Default::default(),
            program_progress: Default::default(),
            services: Default::default(),
            program_titles: Default::default(),
            program_descriptions: Default::default(),
            program_starts: Default::default(),
            program_durations: Default::default(),
            channel_logo_urls: Default::default(),
            channel_types: Default::default(),
            jikkyo_forces: Default::default(),
            guide_start: Default::default(),
            guide_program_ids: Default::default(),
            guide_channel_indices: Default::default(),
            guide_titles: Default::default(),
            guide_descriptions: Default::default(),
            guide_starts: Default::default(),
            guide_durations: Default::default(),
            guide_genres: Default::default(),
            comment_times: Default::default(),
            comment_texts: Default::default(),
            comment_sources: Default::default(),
            comment_status: Default::default(),
            runtime,
            rendered: None,
        };
        super::render::project(&mut value);
        value
    }
}
