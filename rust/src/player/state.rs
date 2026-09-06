use super::{
    catalog_request::CatalogRequest,
    comments::CommentHistory,
    ffi,
    telemetry::{PlaybackFlags, UiState, prepare_log_directory},
};
use crate::{
    comments::CommentEvent, network::NetworkRuntime, playback::Playback, settings::Settings,
};
use cxx_qt_lib::{QString, QStringList};
use std::sync::{Arc, mpsc};

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
    pub(super) subtitle_cue: Option<crate::subtitles::SubtitleCue>,
    pub(super) subtitle_presented: bool,
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
    pub(super) current_program_start: u64,
    pub(super) current_program_duration: u64,
    pub(super) applied_volume: f64,
    pub(super) service_ids: Vec<u64>,
    pub(super) jikkyo_ids: Vec<Option<String>>,
    pub(super) active_jikkyo_id: Option<String>,
    pub(super) comment_generation: Arc<std::sync::atomic::AtomicU64>,
    pub(super) comment_events_tx: mpsc::SyncSender<(u64, CommentEvent)>,
    pub(super) comment_events_rx: mpsc::Receiver<(u64, CommentEvent)>,
    pub(super) comment_task: Option<crate::network::NetworkTask>,
    pub(super) epg_task: Option<crate::network::NetworkTask>,
    pub(super) comments: CommentHistory,
    pub(super) catalog_request: CatalogRequest,
    pub(super) epg_event_generation: Arc<std::sync::atomic::AtomicU64>,
    pub(super) epg_event_server: String,
    pub(super) epg_events_tx: mpsc::SyncSender<u64>,
    pub(super) epg_events_rx: mpsc::Receiver<u64>,
    pub(super) diagnostics: Option<crate::diagnostics::Recorder>,
    pub(super) diagnostic_ui: UiState,
    pub(super) diagnostic_flags: PlaybackFlags,
    pub(super) network: Option<NetworkRuntime>,
    pub(super) playback: Option<Playback>,
}

impl Default for PlayerRust {
    fn default() -> Self {
        let (comment_events_tx, comment_events_rx) =
            mpsc::sync_channel(crate::comments::QUEUE_CAPACITY);
        let (epg_events_tx, epg_events_rx) = mpsc::sync_channel(1);
        let mut settings = Settings::load().unwrap_or_else(|error| {
            tracing::warn!(%error, "Could not load settings");
            Settings::default()
        });
        if let Ok(server) = std::env::var("MIRAKURUN_SERVER") {
            settings.server = server;
        }
        if let Ok(service_id) = std::env::var("MIRAKURUN_SERVICE_ID") {
            settings.service_id = service_id;
        }
        let playback = crate::playback::take_preloaded().and_then(|playback| {
            playback.set_subtitles_enabled(settings.subtitles_enabled)?;
            Ok(playback)
        });
        let network = NetworkRuntime::new();
        let status = playback
            .as_ref()
            .err()
            .map(ToString::to_string)
            .or_else(|| network.as_ref().err().map(ToString::to_string))
            .unwrap_or_else(|| "Ready".to_owned());
        Self {
            server: QString::from(settings.server),
            language: QString::from(settings.language),
            ui_language: ffi::current_ui_language(),
            service_id: QString::from(settings.service_id),
            status: QString::from(status),
            playback_error: QString::default(),
            playback_error_details: QString::default(),
            playing: false,
            volume: settings.volume,
            audio_muted: false,
            audio_tracks: QString::from("[]"),
            audio_error: QString::default(),
            danmaku_enabled: settings.danmaku_enabled,
            comment_font_size: settings.comment_font_size,
            comment_opacity: settings.comment_opacity,
            comment_speed: settings.comment_speed,
            subtitles_enabled: settings.subtitles_enabled,
            subtitle_text: QString::default(),
            subtitle_data: QString::default(),
            subtitle_cue: None,
            subtitle_presented: false,
            autoplay: std::env::var("MIRAKURUN_AUTOPLAY").is_ok_and(|value| value != "0"),
            channel_name: QString::default(),
            program_name: QString::default(),
            program_description: QString::default(),
            channel_logo_url: QString::default(),
            program_progress: 0.0,
            services: QStringList::default(),
            program_titles: QStringList::default(),
            program_descriptions: QStringList::default(),
            program_starts: QStringList::default(),
            program_durations: QStringList::default(),
            channel_logo_urls: QStringList::default(),
            channel_types: QStringList::default(),
            jikkyo_forces: QStringList::default(),
            guide_start: QString::default(),
            guide_program_ids: QStringList::default(),
            guide_channel_indices: QStringList::default(),
            guide_titles: QStringList::default(),
            guide_descriptions: QStringList::default(),
            guide_starts: QStringList::default(),
            guide_durations: QStringList::default(),
            guide_genres: QStringList::default(),
            comment_times: QStringList::default(),
            comment_texts: QStringList::default(),
            comment_sources: QStringList::default(),
            comment_status: QString::from("Select a channel"),
            current_program_start: 0,
            current_program_duration: 0,
            // Force the first event poll to apply a persisted non-default volume.
            applied_volume: -1.0,
            service_ids: Vec::new(),
            jikkyo_ids: Vec::new(),
            active_jikkyo_id: None,
            comment_generation: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            comment_events_tx,
            comment_events_rx,
            comment_task: None,
            epg_task: None,
            comments: CommentHistory::default(),
            catalog_request: CatalogRequest::default(),
            epg_event_generation: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            epg_event_server: String::new(),
            epg_events_tx,
            epg_events_rx,
            diagnostics: if std::env::var("MIRAKURUN_DIAGNOSTICS").is_ok_and(|value| value == "0") {
                None
            } else {
                prepare_log_directory()
                    .and_then(|path| crate::diagnostics::Recorder::start(path.join("usage")))
                    .map_err(|error| tracing::warn!(%error, "Could not start passive diagnostics"))
                    .ok()
            },
            diagnostic_ui: UiState::default(),
            diagnostic_flags: PlaybackFlags::default(),
            network: network.ok(),
            playback: playback.ok(),
        }
    }
}
