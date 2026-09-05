#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qfont.h");
        type QFont = cxx_qt_lib::QFont;
        include!("subtitle_outline.h");
        #[cxx_name = "subtitleOutlinePath"]
        fn subtitle_outline_path(text: &QString, font: &QFont) -> QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
        include!("cxx-qt-lib/qqmlapplicationengine.h");
        type QQmlApplicationEngine = cxx_qt_lib::QQmlApplicationEngine;
        include!("localization.h");
        #[cxx_name = "initializeUiLanguage"]
        fn initialize_ui_language(
            engine: Pin<&mut QQmlApplicationEngine>,
            preference: &QString,
        ) -> bool;
        #[cxx_name = "applyUiLanguage"]
        fn apply_ui_language(preference: &QString) -> QString;
        #[cxx_name = "currentUiLanguage"]
        fn current_ui_language() -> QString;

        include!("qt_helpers.h");
        type QQuickItem;
        #[cxx_name = "configureQtQuickOpenGl"]
        fn configure_qt_quick_open_gl();
        #[cxx_name = "playbackLogDirectory"]
        fn playback_log_directory() -> QString;
        #[cxx_name = "openPlaybackLogDirectory"]
        fn open_playback_log_directory(path: &QString) -> bool;
        #[cxx_name = "qQuickItemAddress"]
        unsafe fn q_quick_item_address(item: *mut QQuickItem) -> usize;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QString, server)]
        #[qproperty(QString, language)]
        #[qproperty(QString, ui_language, cxx_name = "uiLanguage")]
        #[qproperty(QString, service_id, cxx_name = "serviceId")]
        #[qproperty(QString, status)]
        #[qproperty(QString, playback_error, cxx_name = "playbackError")]
        #[qproperty(QString, playback_error_details, cxx_name = "playbackErrorDetails")]
        #[qproperty(bool, playing)]
        #[qproperty(f64, volume)]
        #[qproperty(bool, audio_muted, cxx_name = "audioMuted")]
        #[qproperty(QString, audio_tracks, cxx_name = "audioTracks")]
        #[qproperty(QString, audio_error, cxx_name = "audioError")]
        #[qproperty(bool, danmaku_enabled, cxx_name = "danmakuEnabled")]
        #[qproperty(f64, comment_font_size, cxx_name = "commentFontSize")]
        #[qproperty(f64, comment_opacity, cxx_name = "commentOpacity")]
        #[qproperty(f64, comment_speed, cxx_name = "commentSpeed")]
        #[qproperty(bool, subtitles_enabled, cxx_name = "subtitlesEnabled")]
        #[qproperty(QString, subtitle_text, cxx_name = "subtitleText")]
        #[qproperty(QString, subtitle_data, cxx_name = "subtitleData")]
        #[qproperty(bool, autoplay)]
        #[qproperty(QString, channel_name, cxx_name = "channelName")]
        #[qproperty(QString, program_name, cxx_name = "programName")]
        #[qproperty(QString, program_description, cxx_name = "programDescription")]
        #[qproperty(QString, channel_logo_url, cxx_name = "channelLogoUrl")]
        #[qproperty(f64, program_progress, cxx_name = "programProgress")]
        #[qproperty(QStringList, services)]
        #[qproperty(QStringList, program_titles, cxx_name = "programTitles")]
        #[qproperty(QStringList, program_descriptions, cxx_name = "programDescriptions")]
        #[qproperty(QStringList, program_starts, cxx_name = "programStarts")]
        #[qproperty(QStringList, program_durations, cxx_name = "programDurations")]
        #[qproperty(QStringList, channel_logo_urls, cxx_name = "channelLogoUrls")]
        #[qproperty(QStringList, channel_types, cxx_name = "channelTypes")]
        #[qproperty(QStringList, jikkyo_forces, cxx_name = "jikkyoForces")]
        #[qproperty(QString, guide_start, cxx_name = "guideStart")]
        #[qproperty(QStringList, guide_program_ids, cxx_name = "guideProgramIds")]
        #[qproperty(QStringList, guide_channel_indices, cxx_name = "guideChannelIndices")]
        #[qproperty(QStringList, guide_titles, cxx_name = "guideTitles")]
        #[qproperty(QStringList, guide_descriptions, cxx_name = "guideDescriptions")]
        #[qproperty(QStringList, guide_starts, cxx_name = "guideStarts")]
        #[qproperty(QStringList, guide_durations, cxx_name = "guideDurations")]
        #[qproperty(QStringList, guide_genres, cxx_name = "guideGenres")]
        #[qproperty(QStringList, comment_times, cxx_name = "commentTimes")]
        #[qproperty(QStringList, comment_texts, cxx_name = "commentTexts")]
        #[qproperty(QStringList, comment_sources, cxx_name = "commentSources")]
        #[qproperty(QString, comment_status, cxx_name = "commentStatus")]
        type Player = super::PlayerRust;

        #[qsignal]
        #[cxx_name = "commentReceived"]
        fn comment_received(self: Pin<&mut Player>, text: QString);

        #[qinvokable]
        #[cxx_name = "attachVideoItem"]
        unsafe fn attach_video_item(self: Pin<&mut Player>, item: *mut QQuickItem) -> bool;
        #[qinvokable]
        fn play(self: Pin<&mut Player>);
        #[qinvokable]
        fn stop(self: Pin<&mut Player>);
        #[qinvokable]
        #[cxx_name = "selectAudioTrack"]
        fn select_audio_track(self: Pin<&mut Player>, key: QString);
        #[qinvokable]
        #[cxx_name = "subtitleGlyphOutline"]
        fn subtitle_glyph_outline(self: &Player, text: QString, font: QFont) -> QString;
        #[qinvokable]
        #[cxx_name = "pollEvents"]
        fn poll_events(self: Pin<&mut Player>);
        #[qinvokable]
        #[cxx_name = "pollSubtitles"]
        fn poll_subtitles(self: Pin<&mut Player>);
        #[qinvokable]
        #[cxx_name = "refreshChannels"]
        fn refresh_channels(self: Pin<&mut Player>);
        #[qinvokable]
        #[cxx_name = "refreshCurrentPrograms"]
        fn refresh_current_programs(self: Pin<&mut Player>);
        #[qinvokable]
        #[cxx_name = "selectChannel"]
        fn select_channel(self: Pin<&mut Player>, index: i32);
        #[qinvokable]
        #[cxx_name = "changeChannel"]
        fn change_channel(self: Pin<&mut Player>, offset: i32);
        #[qinvokable]
        #[cxx_name = "saveSettings"]
        fn save_settings(self: Pin<&mut Player>);
        #[qinvokable]
        #[cxx_name = "connectServer"]
        fn connect_server(self: Pin<&mut Player>, server: QString) -> bool;
        #[qinvokable]
        #[cxx_name = "openLogFolder"]
        fn open_log_folder(self: Pin<&mut Player>) -> bool;
        #[qinvokable]
        #[cxx_name = "videoStats"]
        fn video_stats(self: &Player) -> QString;
        #[qinvokable]
        #[cxx_name = "changeLanguage"]
        fn change_language(self: Pin<&mut Player>, language: QString) -> bool;
    }

    impl cxx_qt::Threading for Player {}
}

use crate::comments::{CommentEvent, jikkyo_id};
use crate::epg::{CurrentProgram, EpgStore, Program, Service};
use crate::network::NetworkRuntime;
use crate::playback::{Playback, PlaybackError, PlaybackEvent};
use crate::settings::Settings;
use core::pin::Pin;
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QString, QStringList};
use futures_util::StreamExt;
use serde::Deserialize;
use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

#[derive(Debug, Error)]
enum PlayerError {
    #[error(transparent)]
    Playback(#[from] PlaybackError),
    #[error("Could not initialize the player")]
    PlaybackUnavailable,
    #[error("Enter a valid Mirakurun service ID")]
    InvalidServiceId,
}

#[derive(Debug, Error)]
enum FetchServicesError {
    #[error("Could not load channels: {0}")]
    ServicesRequest(#[source] reqwest::Error),
    #[error("Mirakurun rejected the channel request: {0}")]
    ServicesStatus(#[source] reqwest::Error),
    #[error("Invalid Mirakurun service list: {0}")]
    ServicesResponse(#[source] reqwest::Error),
    #[error("Could not load programs: {0}")]
    ProgramsRequest(#[source] reqwest::Error),
    #[error("Mirakurun rejected the program request: {0}")]
    ProgramsStatus(#[source] reqwest::Error),
    #[error("Invalid Mirakurun program list: {0}")]
    ProgramsResponse(#[source] reqwest::Error),
    #[error("Could not read system time: {0}")]
    SystemTime(#[source] std::time::SystemTimeError),
}

pub struct PlayerRust {
    server: QString,
    language: QString,
    ui_language: QString,
    service_id: QString,
    status: QString,
    playback_error: QString,
    playback_error_details: QString,
    playing: bool,
    volume: f64,
    audio_muted: bool,
    audio_tracks: QString,
    audio_error: QString,
    danmaku_enabled: bool,
    comment_font_size: f64,
    comment_opacity: f64,
    comment_speed: f64,
    subtitles_enabled: bool,
    subtitle_text: QString,
    subtitle_data: QString,
    autoplay: bool,
    channel_name: QString,
    program_name: QString,
    program_description: QString,
    channel_logo_url: QString,
    program_progress: f64,
    services: QStringList,
    program_titles: QStringList,
    program_descriptions: QStringList,
    program_starts: QStringList,
    program_durations: QStringList,
    channel_logo_urls: QStringList,
    channel_types: QStringList,
    jikkyo_forces: QStringList,
    guide_start: QString,
    guide_program_ids: QStringList,
    guide_channel_indices: QStringList,
    guide_titles: QStringList,
    guide_descriptions: QStringList,
    guide_starts: QStringList,
    guide_durations: QStringList,
    guide_genres: QStringList,
    comment_times: QStringList,
    comment_texts: QStringList,
    comment_sources: QStringList,
    comment_status: QString,
    current_program_start: u64,
    current_program_duration: u64,
    applied_volume: f64,
    service_ids: Vec<u64>,
    jikkyo_ids: Vec<Option<String>>,
    active_jikkyo_id: Option<String>,
    comment_generation: Arc<std::sync::atomic::AtomicU64>,
    comment_events_tx: mpsc::Sender<CommentEvent>,
    comment_events_rx: mpsc::Receiver<CommentEvent>,
    comments: VecDeque<(String, String, String)>,
    loading_channels: AtomicBool,
    epg_event_generation: Arc<std::sync::atomic::AtomicU64>,
    epg_event_server: String,
    epg_events_tx: mpsc::Sender<()>,
    epg_events_rx: mpsc::Receiver<()>,
    network: Option<NetworkRuntime>,
    playback: Option<Playback>,
}

impl Default for PlayerRust {
    fn default() -> Self {
        let (comment_events_tx, comment_events_rx) = mpsc::channel();
        let (epg_events_tx, epg_events_rx) = mpsc::channel();
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
        let playback = crate::playback::take_preloaded();
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
            comments: VecDeque::new(),
            loading_channels: AtomicBool::new(false),
            epg_event_generation: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            epg_event_server: String::new(),
            epg_events_tx,
            epg_events_rx,
            network: network.ok(),
            playback: playback.ok(),
        }
    }
}

fn prepare_log_directory() -> std::io::Result<std::path::PathBuf> {
    let path = std::path::PathBuf::from(ffi::playback_log_directory().to_string());
    if !path.is_absolute() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "User log directory unavailable",
        ));
    }
    std::fs::create_dir_all(&path).map_err(|error| {
        std::io::Error::new(error.kind(), format!("{}: {error}", path.display()))
    })?;
    Ok(path)
}

impl ffi::Player {
    pub fn subtitle_glyph_outline(&self, text: QString, font: ffi::QFont) -> QString {
        ffi::subtitle_outline_path(&text, &font)
    }

    pub fn change_language(mut self: Pin<&mut Self>, language: QString) -> bool {
        let preference = crate::settings::normalize_language(&language.to_string());
        let language = QString::from(preference);
        let effective = ffi::apply_ui_language(&language);
        if effective.is_empty() {
            tracing::error!("Could not load UI translation");
            self.as_mut()
                .set_status(QString::from("Could not load UI translation"));
            return false;
        }
        self.as_mut().set_language(language);
        self.as_mut().set_ui_language(effective);
        self.as_mut().save_settings();
        true
    }

    pub fn video_stats(&self) -> QString {
        self.rust()
            .playback
            .as_ref()
            .and_then(|playback| serde_json::to_string(&playback.video_stats()).ok())
            .map(QString::from)
            .unwrap_or_else(|| QString::from("{}"))
    }

    pub fn connect_server(mut self: Pin<&mut Self>, server: QString) -> bool {
        let server = server.to_string().trim().trim_end_matches('/').to_owned();
        if !reqwest::Url::parse(&server)
            .is_ok_and(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
        {
            self.as_mut().set_status(QString::from(
                "Enter a server URL starting with http:// or https://",
            ));
            return false;
        }
        if self.as_ref().server().to_string() != server {
            self.as_mut().stop();
            if *self.as_ref().playing() {
                return false;
            }
            self.as_mut().set_server(QString::from(server));
            self.as_mut().set_service_id(QString::default());
            self.as_mut().set_channel_name(QString::default());
            self.as_mut().set_program_name(QString::default());
            self.as_mut().set_program_description(QString::default());
            self.as_mut().set_channel_logo_url(QString::default());
            self.as_mut().set_subtitle_text(QString::default());
            self.as_mut().set_subtitle_data(QString::default());
            self.as_mut().rust_mut().service_ids.clear();
            self.as_mut().rust_mut().jikkyo_ids.clear();
            self.as_mut().set_services(QStringList::default());
            self.as_mut().set_channel_types(QStringList::default());
            self.as_mut().set_program_titles(QStringList::default());
            self.as_mut()
                .set_program_descriptions(QStringList::default());
            self.as_mut().set_program_starts(QStringList::default());
            self.as_mut().set_program_durations(QStringList::default());
            self.as_mut().set_channel_logo_urls(QStringList::default());
            self.as_mut().set_jikkyo_forces(QStringList::default());
            self.as_mut().set_guide_program_ids(QStringList::default());
            self.as_mut()
                .set_guide_channel_indices(QStringList::default());
            self.as_mut().set_guide_titles(QStringList::default());
            self.as_mut().set_guide_descriptions(QStringList::default());
            self.as_mut().set_guide_starts(QStringList::default());
            self.as_mut().set_guide_durations(QStringList::default());
            self.as_mut().set_guide_genres(QStringList::default());
            self.as_mut().rust_mut().current_program_start = 0;
            self.as_mut().rust_mut().current_program_duration = 0;
            self.as_mut().set_program_progress(0.0);
            self.as_mut().restart_comments();
        }
        self.as_mut().save_settings();
        self.as_mut().refresh_channels();
        true
    }

    pub fn open_log_folder(self: Pin<&mut Self>) -> bool {
        match prepare_log_directory() {
            Ok(path) => {
                let opened = ffi::open_playback_log_directory(&QString::from(
                    path.to_string_lossy().as_ref(),
                ));
                if !opened {
                    tracing::warn!(path = %path.display(), "Could not open log folder");
                }
                opened
            }
            Err(error) => {
                tracing::warn!(%error, "Could not prepare log folder");
                false
            }
        }
    }

    fn report_playback_error(mut self: Pin<&mut Self>, error: &PlayerError) {
        self.as_mut().set_playing(false);
        if let Some(playback) = self.as_ref().rust().playback.as_ref() {
            if let Err(stop_error) = playback.stop() {
                tracing::warn!(%stop_error, "Could not clean up failed playback");
            }
        }
        self.as_mut().set_subtitle_text(QString::default());
        self.as_mut().set_subtitle_data(QString::default());
        let summary = match error {
            PlayerError::Playback(error) => error.user_message(),
            PlayerError::PlaybackUnavailable => "Could not initialize the player",
            PlayerError::InvalidServiceId => "Enter a valid Mirakurun service ID",
        };
        self.as_mut().set_playback_error(QString::from(summary));
        let message = error.to_string();
        self.as_mut()
            .set_playback_error_details(QString::from(&message));
        tracing::error!(%error, "Playback failed");
        match prepare_log_directory() {
            Ok(directory) => {
                let path = directory.join("playback-error.log");
                match std::fs::write(&path, format!("{message}\n")) {
                    Ok(()) => tracing::error!(path = %path.display(), "Playback diagnostic saved"),
                    Err(write_error) => {
                        tracing::warn!(%write_error, path = %path.display(), "Could not save playback diagnostic")
                    }
                }
            }
            Err(error) => tracing::warn!(%error, "Could not prepare playback diagnostic directory"),
        }
        self.as_mut().set_status(QString::from(message));
    }

    pub unsafe fn attach_video_item(mut self: Pin<&mut Self>, item: *mut ffi::QQuickItem) -> bool {
        let address = unsafe { ffi::q_quick_item_address(item) };
        let result: Result<(), PlayerError> = {
            let mut rust = self.as_mut().rust_mut();
            rust.playback
                .as_mut()
                .ok_or(PlayerError::PlaybackUnavailable)
                .and_then(|playback| {
                    playback
                        .attach_video_item(address as *mut c_void)
                        .map_err(Into::into)
                })
        };
        match result {
            Ok(()) => true,
            Err(error) => {
                self.as_mut().report_playback_error(&error);
                false
            }
        }
    }

    pub fn play(mut self: Pin<&mut Self>) {
        self.as_mut().set_subtitle_text(QString::default());
        self.as_mut().set_subtitle_data(QString::default());
        self.as_mut().set_playback_error(QString::default());
        self.as_mut().set_playback_error_details(QString::default());
        let server = self.as_ref().server().to_string();
        let service_id = self.as_ref().service_id().to_string().parse::<u64>();
        let result: Result<(), PlayerError> =
            match (self.as_ref().rust().playback.as_ref(), service_id) {
                (Some(playback), Ok(id)) if id > 0 => {
                    playback.play_service(&server, id).map_err(Into::into)
                }
                (None, _) => Err(PlayerError::PlaybackUnavailable),
                _ => Err(PlayerError::InvalidServiceId),
            };
        match result {
            Ok(()) => {
                self.as_mut().set_playing(true);
                self.as_mut().set_status(QString::from("Connecting..."));
            }
            Err(error) => self.as_mut().report_playback_error(&error),
        }
    }

    pub fn stop(mut self: Pin<&mut Self>) {
        let result = self
            .as_ref()
            .rust()
            .playback
            .as_ref()
            .ok_or(PlayerError::PlaybackUnavailable)
            .and_then(|playback| playback.stop().map_err(Into::into));
        match result {
            Ok(()) => {
                self.as_mut().set_playing(false);
                self.as_mut().set_status(QString::from("Stopped"));
                self.as_mut().set_subtitle_text(QString::default());
                self.as_mut().set_subtitle_data(QString::default());
                self.as_mut().set_playback_error(QString::default());
                self.as_mut().set_playback_error_details(QString::default());
            }
            Err(error) => self.as_mut().report_playback_error(&error),
        }
    }

    fn refresh_audio_state(mut self: Pin<&mut Self>) {
        if let Some((tracks, error)) = self
            .as_ref()
            .rust()
            .playback
            .as_ref()
            .map(Playback::audio_state)
        {
            self.as_mut().set_audio_tracks(QString::from(tracks));
            self.as_mut().set_audio_error(QString::from(error));
        }
    }

    pub fn select_audio_track(mut self: Pin<&mut Self>, key: QString) {
        if let Some(playback) = self.as_ref().rust().playback.as_ref() {
            playback.select_audio_option(&key.to_string());
        }
        self.as_mut().refresh_audio_state();
    }

    pub fn poll_subtitles(mut self: Pin<&mut Self>) {
        use crate::subtitles::SubtitleUpdate;
        let update = self
            .as_ref()
            .rust()
            .playback
            .as_ref()
            .map(Playback::poll_subtitles)
            .unwrap_or(SubtitleUpdate::Unchanged);
        match update {
            SubtitleUpdate::Unchanged => {}
            SubtitleUpdate::Clear => {
                self.as_mut().set_subtitle_text(QString::default());
                self.as_mut().set_subtitle_data(QString::default());
            }
            SubtitleUpdate::Show(cue) => {
                self.as_mut().set_subtitle_text(QString::from(&cue.text));
                if let Ok(data) = serde_json::to_string(&cue) {
                    self.as_mut().set_subtitle_data(QString::from(data));
                }
            }
        }
    }

    pub fn poll_events(mut self: Pin<&mut Self>) {
        // Failed or stopped streams must not overwrite the original diagnostic.
        if *self.as_ref().playing() {
            let event = self
                .as_ref()
                .rust()
                .playback
                .as_ref()
                .ok_or(PlayerError::PlaybackUnavailable)
                .and_then(|playback| playback.drain_events().map_err(Into::into));
            match event {
                Ok(PlaybackEvent::None) => {}
                Ok(PlaybackEvent::Playing) => self.as_mut().set_status(QString::from("Playing")),
                Ok(PlaybackEvent::Ended) => self
                    .as_mut()
                    .report_playback_error(&PlaybackError::StreamEnded.into()),
                Err(error) => self.as_mut().report_playback_error(&error),
            }
        }
        self.as_mut().refresh_audio_state();
        let volume = if *self.as_ref().audio_muted() {
            0.0
        } else {
            *self.as_ref().volume()
        };
        if (volume - self.as_ref().rust().applied_volume).abs() > f64::EPSILON {
            if let Some(playback) = self.as_ref().rust().playback.as_ref() {
                playback.set_volume(volume);
            }
            self.as_mut().rust_mut().applied_volume = volume;
        }
        let (start, duration) = {
            let player = self.as_ref();
            let rust = player.rust();
            (rust.current_program_start, rust.current_program_duration)
        };
        let progress = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .filter(|_| start > 0 && duration > 0)
            .map(|now| now.as_millis().saturating_sub(u128::from(start)) as f64 / duration as f64)
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        if (progress - *self.as_ref().program_progress()).abs() > 0.000_01 {
            self.as_mut().set_program_progress(progress);
        }
        let comment_events = self
            .as_ref()
            .rust()
            .comment_events_rx
            .try_iter()
            .collect::<Vec<_>>();
        let mut comments_changed = false;
        let mut received_texts = Vec::new();
        for event in comment_events {
            match event {
                CommentEvent::Status(status) => {
                    self.as_mut().set_comment_status(QString::from(status))
                }
                CommentEvent::Comment {
                    time,
                    text,
                    source,
                    initial,
                } => {
                    if !initial {
                        received_texts.push(text.clone());
                    }
                    let mut rust = self.as_mut().rust_mut();
                    rust.comments.push_back((time, text, source));
                    while rust.comments.len() > 200 {
                        rust.comments.pop_front();
                    }
                    comments_changed = true;
                }
            }
        }
        if comments_changed {
            let (times, texts, sources) = {
                let player = self.as_ref();
                let rust = player.rust();
                (
                    rust.comments
                        .iter()
                        .map(|item| QString::from(&item.0))
                        .collect(),
                    rust.comments
                        .iter()
                        .map(|item| QString::from(&item.1))
                        .collect(),
                    rust.comments
                        .iter()
                        .map(|item| QString::from(&item.2))
                        .collect(),
                )
            };
            self.as_mut().set_comment_times(times);
            self.as_mut().set_comment_texts(texts);
            self.as_mut().set_comment_sources(sources);
        }
        for text in received_texts {
            self.as_mut().comment_received(QString::from(text));
        }
        if self.as_ref().rust().epg_events_rx.try_recv().is_ok() {
            while self.as_ref().rust().epg_events_rx.try_recv().is_ok() {}
            self.as_mut().refresh_channels();
        }
    }

    pub fn refresh_channels(mut self: Pin<&mut Self>) {
        self.as_mut().ensure_epg_event_stream();
        if self
            .as_ref()
            .rust()
            .loading_channels
            .swap(true, Ordering::AcqRel)
        {
            return;
        }
        let server = self.as_ref().server().to_string();
        let qt_thread = self.qt_thread();
        let network = self
            .as_ref()
            .rust()
            .network
            .as_ref()
            .map(|network| (network.handle(), network.client(), network.epg()));
        let Some((runtime, client, epg)) = network else {
            self.as_ref()
                .rust()
                .loading_channels
                .store(false, Ordering::Release);
            self.as_mut()
                .set_status(QString::from("Network runtime is unavailable"));
            return;
        };
        self.as_mut()
            .set_status(QString::from("Loading channels..."));
        runtime.spawn(async move {
            let result = fetch_services(&client, &epg, &server).await;
            let _ = qt_thread.queue(move |mut player| {
                player
                    .as_ref()
                    .rust()
                    .loading_channels
                    .store(false, Ordering::Release);
                // A previous server's request may complete after settings changed.
                if player.as_ref().server().to_string() != server {
                    player.as_mut().refresh_channels();
                    return;
                }
                match result {
                    Ok(payload) => {
                        let services = payload.channels;
                        let labels = services
                            .iter()
                            .map(|service| QString::from(&service.label))
                            .collect::<QStringList>();
                        let program_titles = services
                            .iter()
                            .map(|service| QString::from(&service.program_title))
                            .collect::<QStringList>();
                        let program_descriptions = services
                            .iter()
                            .map(|service| QString::from(&service.program_description))
                            .collect::<QStringList>();
                        let program_starts = services
                            .iter()
                            .map(|service| QString::from(service.program_start.to_string()))
                            .collect::<QStringList>();
                        let program_durations = services
                            .iter()
                            .map(|service| QString::from(service.program_duration.to_string()))
                            .collect::<QStringList>();
                        let channel_logo_urls = services
                            .iter()
                            .map(|service| {
                                QString::from(if service.has_logo_data {
                                    service_logo_url(&server, service.id)
                                } else {
                                    String::new()
                                })
                            })
                            .collect::<QStringList>();
                        let channel_types = services
                            .iter()
                            .map(|service| QString::from(&service.channel_type))
                            .collect::<QStringList>();
                        let jikkyo_forces = services
                            .iter()
                            .map(|service| {
                                QString::from(
                                    service
                                        .jikkyo_force
                                        .map(|force| force.to_string())
                                        .unwrap_or_default(),
                                )
                            })
                            .collect::<QStringList>();
                        player.as_mut().rust_mut().service_ids =
                            services.iter().map(|service| service.id).collect();
                        player.as_mut().rust_mut().jikkyo_ids = services
                            .iter()
                            .map(|service| service.jikkyo_id.clone())
                            .collect();
                        player.as_mut().set_services(labels);
                        player.as_mut().set_program_titles(program_titles);
                        player
                            .as_mut()
                            .set_program_descriptions(program_descriptions);
                        player.as_mut().set_program_starts(program_starts);
                        player.as_mut().set_program_durations(program_durations);
                        player.as_mut().set_channel_logo_urls(channel_logo_urls);
                        player.as_mut().set_channel_types(channel_types);
                        player.as_mut().set_jikkyo_forces(jikkyo_forces);
                        player
                            .as_mut()
                            .set_guide_start(QString::from(payload.guide_start.to_string()));
                        player.as_mut().set_guide_program_ids(strings(
                            payload.guide.iter().map(|p| p.id.to_string()),
                        ));
                        player.as_mut().set_guide_channel_indices(strings(
                            payload.guide.iter().map(|p| p.channel_index.to_string()),
                        ));
                        player.as_mut().set_guide_titles(strings(
                            payload.guide.iter().map(|p| p.title.clone()),
                        ));
                        player.as_mut().set_guide_descriptions(strings(
                            payload.guide.iter().map(|p| p.description.clone()),
                        ));
                        player.as_mut().set_guide_starts(strings(
                            payload.guide.iter().map(|p| p.start_at.to_string()),
                        ));
                        player.as_mut().set_guide_durations(strings(
                            payload.guide.iter().map(|p| p.duration.to_string()),
                        ));
                        player.as_mut().set_guide_genres(strings(
                            payload.guide.iter().map(|p| p.genre.to_string()),
                        ));
                        if let Ok(current_id) =
                            player.as_ref().service_id().to_string().parse::<u64>()
                            && let Some(index) = player
                                .as_ref()
                                .rust()
                                .service_ids
                                .iter()
                                .position(|id| *id == current_id)
                        {
                            let channel = &services[index];
                            let label = QString::from(&channel.label);
                            let title = QString::from(&channel.program_title);
                            let description = QString::from(&channel.program_description);
                            let logo = QString::from(if channel.has_logo_data {
                                service_logo_url(&server, channel.id)
                            } else {
                                String::new()
                            });
                            let start = channel.program_start;
                            let duration = channel.program_duration;
                            player.as_mut().set_channel_name(label);
                            player.as_mut().set_program_name(title);
                            player.as_mut().set_program_description(description);
                            player.as_mut().set_channel_logo_url(logo);
                            player.as_mut().rust_mut().current_program_start = start;
                            player.as_mut().rust_mut().current_program_duration = duration;
                        }
                        if !*player.as_ref().playing() {
                            player
                                .as_mut()
                                .set_status(QString::from(if services.is_empty() {
                                    "No available channels were found"
                                } else {
                                    "Ready"
                                }));
                        }
                        player.as_mut().restart_comments();
                    }
                    Err(error) => player.as_mut().set_status(QString::from(error.to_string())),
                }
            });
        });
    }

    fn ensure_epg_event_stream(mut self: Pin<&mut Self>) {
        let server = self
            .as_ref()
            .server()
            .to_string()
            .trim()
            .trim_end_matches('/')
            .to_owned();
        if server.is_empty() || self.as_ref().rust().epg_event_server == server {
            return;
        }
        let Some((runtime, client)) = self
            .as_ref()
            .rust()
            .network
            .as_ref()
            .map(|network| (network.handle(), network.client()))
        else {
            return;
        };
        let generation = self
            .as_ref()
            .rust()
            .epg_event_generation
            .fetch_add(1, Ordering::AcqRel)
            + 1;
        let active_generation = self.as_ref().rust().epg_event_generation.clone();
        let events = self.as_ref().rust().epg_events_tx.clone();
        self.as_mut().rust_mut().epg_event_server = server.clone();
        runtime.spawn(async move {
            let url = format!("{server}/api/events/stream?resource=program");
            while active_generation.load(Ordering::Acquire) == generation {
                let response = client.get(&url).send().await;
                if let Ok(response) = response.and_then(reqwest::Response::error_for_status) {
                    let mut stream = response.bytes_stream();
                    let mut last_notification = tokio::time::Instant::now()
                        .checked_sub(std::time::Duration::from_secs(10))
                        .unwrap_or_else(tokio::time::Instant::now);
                    while active_generation.load(Ordering::Acquire) == generation {
                        let Some(chunk) = stream.next().await else {
                            break;
                        };
                        let Ok(chunk) = chunk else { break };
                        if chunk.iter().any(|byte| !byte.is_ascii_whitespace())
                            && last_notification.elapsed() >= std::time::Duration::from_secs(15)
                        {
                            let _ = events.send(());
                            last_notification = tokio::time::Instant::now();
                        }
                    }
                }
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        });
    }

    /// Re-evaluate the current programme from the in-memory EPG snapshot.
    /// This is intentionally network-free so it can run at programme boundaries.
    pub fn refresh_current_programs(mut self: Pin<&mut Self>) {
        let Some(epg) = self
            .as_ref()
            .rust()
            .network
            .as_ref()
            .map(NetworkRuntime::epg)
        else {
            return;
        };
        let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH) else {
            return;
        };
        let snapshot = epg.snapshot();
        if snapshot.services.is_empty() {
            return;
        }
        let current_programs = snapshot
            .current_programs(now.as_millis() as u64)
            .into_iter()
            .map(|program| ((program.network_id, program.service_id), program))
            .collect::<HashMap<_, _>>();
        let services = snapshot
            .services
            .iter()
            .map(|service| (service.id, service))
            .collect::<HashMap<_, _>>();
        let service_ids = self.as_ref().rust().service_ids.clone();
        let program_for = |id: &u64| {
            services
                .get(id)
                .and_then(|service| current_programs.get(&(service.network_id, service.service_id)))
        };
        let titles = service_ids
            .iter()
            .map(|id| {
                QString::from(
                    program_for(id)
                        .and_then(|program| program.name.as_deref())
                        .unwrap_or(""),
                )
            })
            .collect::<QStringList>();
        let descriptions = service_ids
            .iter()
            .map(|id| {
                QString::from(
                    program_for(id)
                        .and_then(|program| program.description.as_deref())
                        .unwrap_or(""),
                )
            })
            .collect::<QStringList>();
        let starts = service_ids
            .iter()
            .map(|id| {
                QString::from(
                    program_for(id)
                        .map_or(0, |program| program.start_at)
                        .to_string(),
                )
            })
            .collect::<QStringList>();
        let durations = service_ids
            .iter()
            .map(|id| {
                QString::from(
                    program_for(id)
                        .map_or(0, |program| program.duration)
                        .to_string(),
                )
            })
            .collect::<QStringList>();
        self.as_mut().set_program_titles(titles);
        self.as_mut().set_program_descriptions(descriptions);
        self.as_mut().set_program_starts(starts);
        self.as_mut().set_program_durations(durations);

        let current_id = self.as_ref().service_id().to_string().parse::<u64>().ok();
        if let Some(playback) = self.as_ref().rust().playback.as_ref() {
            playback.set_audio_program(
                current_id
                    .as_ref()
                    .and_then(program_for)
                    .map(|program| (program.service_id, program.start_at, program.audios.clone())),
            );
        }
        if let Some(program) = current_id.as_ref().and_then(program_for) {
            self.as_mut()
                .set_program_name(QString::from(program.name.as_deref().unwrap_or("")));
            self.as_mut().set_program_description(QString::from(
                program.description.as_deref().unwrap_or(""),
            ));
            self.as_mut().rust_mut().current_program_start = program.start_at;
            self.as_mut().rust_mut().current_program_duration = program.duration;
        }
    }

    pub fn select_channel(mut self: Pin<&mut Self>, index: i32) {
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        let Some(&service_id) = self.as_ref().rust().service_ids.get(index) else {
            return;
        };
        let channel_name = self
            .as_ref()
            .services()
            .get(index as isize)
            .map(|value| value.to_string())
            .unwrap_or_default();
        let program_name = self
            .as_ref()
            .program_titles()
            .get(index as isize)
            .map(|value| value.to_string())
            .unwrap_or_default();
        let program_description = self
            .as_ref()
            .program_descriptions()
            .get(index as isize)
            .map(|value| value.to_string())
            .unwrap_or_default();
        let program_start = self
            .as_ref()
            .program_starts()
            .get(index as isize)
            .and_then(|value| value.to_string().parse::<u64>().ok())
            .unwrap_or(0);
        let program_duration = self
            .as_ref()
            .program_durations()
            .get(index as isize)
            .and_then(|value| value.to_string().parse::<u64>().ok())
            .unwrap_or(0);
        let logo_url = self
            .as_ref()
            .channel_logo_urls()
            .get(index as isize)
            .map(|value| value.to_string())
            .unwrap_or_default();
        self.as_mut()
            .set_service_id(QString::from(service_id.to_string()));
        self.as_mut().set_channel_name(QString::from(channel_name));
        self.as_mut().set_program_name(QString::from(program_name));
        self.as_mut()
            .set_program_description(QString::from(program_description));
        self.as_mut().set_channel_logo_url(QString::from(logo_url));
        self.as_mut().rust_mut().current_program_start = program_start;
        self.as_mut().rust_mut().current_program_duration = program_duration;
        self.as_mut().restart_comments();
        self.as_mut().save_settings();
        self.play();
    }

    pub fn change_channel(self: Pin<&mut Self>, offset: i32) {
        let count = self.as_ref().rust().service_ids.len();
        if count == 0 {
            return;
        }
        let current_id = self.as_ref().service_id().to_string().parse::<u64>().ok();
        let current = current_id
            .and_then(|id| {
                self.as_ref()
                    .rust()
                    .service_ids
                    .iter()
                    .position(|item| *item == id)
            })
            .unwrap_or(0);
        let next = (current as i64 + i64::from(offset)).rem_euclid(count as i64) as i32;
        self.select_channel(next);
    }

    pub fn save_settings(mut self: Pin<&mut Self>) {
        let settings = Settings {
            server: self.as_ref().server().to_string(),
            language: self.as_ref().language().to_string(),
            service_id: self.as_ref().service_id().to_string(),
            volume: (*self.as_ref().volume()).clamp(0.0, 100.0),
            danmaku_enabled: *self.as_ref().danmaku_enabled(),
            comment_font_size: (*self.as_ref().comment_font_size()).clamp(12.0, 48.0),
            comment_opacity: (*self.as_ref().comment_opacity()).clamp(0.1, 1.0),
            comment_speed: (*self.as_ref().comment_speed()).clamp(0.5, 2.0),
            subtitles_enabled: *self.as_ref().subtitles_enabled(),
        };
        if let Err(error) = settings.save() {
            tracing::warn!(%error, "Could not save settings");
            self.as_mut()
                .set_status(QString::from(format!("Could not save settings: {error}")));
        }
    }

    fn restart_comments(mut self: Pin<&mut Self>) {
        let current_id = self.as_ref().service_id().to_string().parse::<u64>().ok();
        let jikkyo = current_id.and_then(|id| {
            let index = self
                .as_ref()
                .rust()
                .service_ids
                .iter()
                .position(|item| *item == id)?;
            self.as_ref().rust().jikkyo_ids.get(index)?.clone()
        });
        if jikkyo.is_some() && self.as_ref().rust().active_jikkyo_id == jikkyo {
            return;
        }
        let generation = self
            .as_ref()
            .rust()
            .comment_generation
            .fetch_add(1, Ordering::AcqRel)
            + 1;
        self.as_mut().rust_mut().active_jikkyo_id = jikkyo.clone();
        self.as_mut().rust_mut().comments.clear();
        self.as_mut().set_comment_times(QStringList::default());
        self.as_mut().set_comment_texts(QStringList::default());
        self.as_mut().set_comment_sources(QStringList::default());
        let Some(channel_id) = jikkyo else {
            self.as_mut()
                .set_comment_status(QString::from("Comments are unavailable for this channel"));
            return;
        };
        let network = self
            .as_ref()
            .rust()
            .network
            .as_ref()
            .map(|network| (network.handle(), network.client()));
        let Some((handle, client)) = network else {
            return;
        };
        self.as_mut()
            .set_comment_status(QString::from("Connecting to comments…"));
        let current_generation = self.as_ref().rust().comment_generation.clone();
        let events = self.as_ref().rust().comment_events_tx.clone();
        handle.spawn(crate::comments::receive(
            client,
            channel_id,
            generation,
            current_generation,
            events,
        ));
    }
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct ProgramSignature {
    event_id: u16,
    start_at: u64,
    duration: u64,
}

struct Channel {
    id: u64,
    has_logo_data: bool,
    label: String,
    channel_number: u16,
    channel_priority: u8,
    service_id: u16,
    physical_channel: String,
    program_signature: Option<ProgramSignature>,
    program_title: String,
    program_description: String,
    program_start: u64,
    program_duration: u64,
    network_id: u16,
    channel_type: String,
    jikkyo_id: Option<String>,
    jikkyo_force: Option<u64>,
}

#[derive(Deserialize)]
struct NxChannel {
    id: String,
    threads: Vec<NxThread>,
}

#[derive(Deserialize)]
struct NxThread {
    status: String,
    jikkyo_force: Option<u64>,
}

struct GuideProgram {
    id: u64,
    channel_index: usize,
    title: String,
    description: String,
    start_at: u64,
    duration: u64,
    genre: u8,
}

struct FetchPayload {
    channels: Vec<Channel>,
    guide: Vec<GuideProgram>,
    guide_start: u64,
}

fn strings(values: impl Iterator<Item = String>) -> QStringList {
    values.map(|value| QString::from(value)).collect()
}

async fn fetch_services(
    client: &reqwest::Client,
    epg: &EpgStore,
    server: &str,
) -> Result<FetchPayload, FetchServicesError> {
    let api = server.trim().trim_end_matches('/');
    let services = client
        .get(format!("{api}/api/services"))
        .send()
        .await
        .map_err(FetchServicesError::ServicesRequest)?
        .error_for_status()
        .map_err(FetchServicesError::ServicesStatus)?
        .json::<Vec<Service>>()
        .await
        .map_err(FetchServicesError::ServicesResponse)?;
    let programs = client
        .get(format!("{api}/api/programs"))
        .send()
        .await
        .map_err(FetchServicesError::ProgramsRequest)?
        .error_for_status()
        .map_err(FetchServicesError::ProgramsStatus)?
        .json::<Vec<Program>>()
        .await
        .map_err(FetchServicesError::ProgramsResponse)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(FetchServicesError::SystemTime)?
        .as_millis() as u64;
    epg.replace(services, programs.clone(), now);
    let snapshot = epg.snapshot();
    let current_programs = snapshot.current_programs(now);
    let mut channels = build_channels(&snapshot.services, current_programs);
    if let Ok(forces) = fetch_jikkyo_forces(client).await {
        for channel in &mut channels {
            channel.jikkyo_force = channel
                .jikkyo_id
                .as_ref()
                .and_then(|id| forces.get(id).copied());
        }
    }
    // QML presents seven local calendar days. Keep a one-day margin before now
    // so today's programmes are available regardless of the local UTC offset,
    // plus enough future data to cover the final tab completely.
    let guide_start = now.saturating_sub(24 * 60 * 60 * 1_000);
    let guide_end = now.saturating_add(8 * 24 * 60 * 60 * 1_000);
    let mut guide = programs
        .into_iter()
        .filter(|program| {
            program.start_at < guide_end
                && program.start_at.saturating_add(program.duration) > guide_start
        })
        .filter_map(|program| {
            let channel_index = channels.iter().position(|channel| {
                channel.network_id == program.network_id && channel.service_id == program.service_id
            })?;
            Some(GuideProgram {
                id: program.id,
                channel_index,
                title: program.name.unwrap_or_default(),
                description: program.description.unwrap_or_default(),
                start_at: program.start_at,
                duration: program.duration,
                genre: program.genres.first().map_or(15, |genre| genre.lv1),
            })
        })
        .collect::<Vec<_>>();
    guide.sort_unstable_by_key(|program| (program.channel_index, program.start_at));
    Ok(FetchPayload {
        channels,
        guide,
        guide_start,
    })
}

fn build_channels(services: &[Service], programs: Vec<CurrentProgram>) -> Vec<Channel> {
    let current_programs = programs
        .iter()
        .map(|program| ((program.network_id, program.service_id), program))
        .collect::<HashMap<_, _>>();
    let mut channels = services
        .iter()
        .filter(|service| service.service_type == 1)
        .map(|service| {
            let remote_key = service.remote_control_key_id.unwrap_or(0);
            let is_terrestrial = service.channel.channel_type == "GR";
            let channel_number = if is_terrestrial {
                remote_key
            } else {
                service.service_id
            };
            let channel_priority = match service.channel.channel_type.as_str() {
                "GR" => 0,
                "BS" => 1,
                "CS" => 2,
                "SKY" => 3,
                _ => 4,
            };
            let program = current_programs.get(&(service.network_id, service.service_id));
            Channel {
                id: service.id,
                has_logo_data: service.has_logo_data,
                label: if is_terrestrial && remote_key == 0 {
                    format!("--   {}", service.name)
                } else if is_terrestrial {
                    format!("{remote_key:02}   {}", service.name)
                } else {
                    format!("{:03}   {}", service.service_id, service.name)
                },
                channel_number,
                channel_priority,
                service_id: service.service_id,
                physical_channel: format!(
                    "{}:{}:{}",
                    service.network_id, service.channel.channel_type, service.channel.channel
                ),
                program_signature: program.map(|program| ProgramSignature {
                    event_id: program.event_id,
                    start_at: program.start_at,
                    duration: program.duration,
                }),
                program_title: program
                    .and_then(|program| program.name.clone())
                    .unwrap_or_default(),
                program_description: program
                    .and_then(|program| program.description.clone())
                    .unwrap_or_default(),
                program_start: program.map_or(0, |program| program.start_at),
                program_duration: program.map_or(0, |program| program.duration),
                network_id: service.network_id,
                channel_type: service.channel.channel_type.clone(),
                jikkyo_id: jikkyo_id(
                    &service.channel.channel_type,
                    service.service_id,
                    &service.name,
                ),
                jikkyo_force: None,
            }
        })
        .collect::<Vec<_>>();
    channels.sort_by(|a, b| {
        (
            a.channel_priority,
            a.channel_number == 0,
            a.channel_number,
            &a.label,
            a.service_id,
        )
            .cmp(&(
                b.channel_priority,
                b.channel_number == 0,
                b.channel_number,
                &b.label,
                b.service_id,
            ))
    });
    let mut main_broadcasts = HashMap::<String, Option<ProgramSignature>>::new();
    let mut broadcasts = HashSet::new();
    channels.retain(|channel| {
        let Some(main_program) = main_broadcasts.get(&channel.physical_channel) else {
            main_broadcasts.insert(
                channel.physical_channel.clone(),
                channel.program_signature.clone(),
            );
            broadcasts.insert((
                channel.physical_channel.clone(),
                channel.program_signature.clone(),
            ));
            return true;
        };
        matches!(
            (main_program, &channel.program_signature),
            (Some(main), Some(subchannel)) if main != subchannel
        ) && broadcasts.insert((
            channel.physical_channel.clone(),
            channel.program_signature.clone(),
        ))
    });
    channels
}

async fn fetch_jikkyo_forces(
    client: &reqwest::Client,
) -> Result<HashMap<String, u64>, reqwest::Error> {
    let channels = client
        .get("https://nx-jikkyo.tsukumijima.net/api/v1/channels")
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<NxChannel>>()
        .await?;
    Ok(channels
        .into_iter()
        .filter_map(|channel| {
            channel
                .threads
                .into_iter()
                .find(|thread| thread.status == "ACTIVE")
                .and_then(|thread| thread.jikkyo_force)
                .map(|force| (channel.id, force))
        })
        .collect())
}

fn service_logo_url(server: &str, service_id: u64) -> String {
    format!(
        "{}/api/services/{service_id}/logo",
        server.trim().trim_end_matches('/')
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::epg::ServiceChannel;

    fn service(service_id: u16, name: &str) -> Service {
        Service {
            id: 32_000_000_u64 + u64::from(service_id),
            service_id,
            network_id: 32_000,
            name: name.to_owned(),
            service_type: 1,
            has_logo_data: true,
            remote_control_key_id: Some(1),
            channel: ServiceChannel {
                channel_type: "GR".to_owned(),
                channel: "26".to_owned(),
            },
        }
    }

    fn program(service_id: u16, event_id: u16) -> CurrentProgram {
        CurrentProgram {
            audios: Vec::new(),
            event_id,
            service_id,
            network_id: 32_000,
            start_at: 1_000,
            duration: 1_800,
            name: Some(format!("Program {event_id}")),
            description: Some(format!("Description {event_id}")),
        }
    }

    #[test]
    fn hides_subchannel_during_simulcast() {
        let services = [service(100, "Main"), service(101, "Sub")];
        let channels = build_channels(&services, vec![program(100, 10), program(101, 10)]);
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].service_id, 100);
    }

    #[test]
    fn keeps_subchannel_during_split_programming() {
        let services = [service(100, "Main"), service(101, "Sub")];
        let channels = build_channels(&services, vec![program(100, 10), program(101, 11)]);
        assert_eq!(channels.len(), 2);
    }

    #[test]
    fn hides_subchannel_when_program_information_is_missing() {
        let services = [service(100, "Main"), service(101, "Sub")];
        let channels = build_channels(&services, vec![program(100, 10)]);
        assert_eq!(channels.len(), 1);
    }
}
