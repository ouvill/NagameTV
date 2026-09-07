mod audio_output;
mod audio_streams;
mod channel_programs;
mod channel_refresh;
mod channels;
mod comments;
mod epg;
mod guide;
mod language;
mod lifecycle;
use lifecycle::{Failure as StatusFailure, Status as PlaybackStatus};
mod playback_failure;
mod preferences;
mod program_info;
mod startup;
mod statistics;
mod status;
mod subtitle_rendering;
mod subtitle_status;
mod telemetry;

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
        include!("qt_helpers.h");
        #[cxx_name = "playbackLogDirectory"]
        fn playback_log_directory() -> QString;
        #[cxx_name = "openPlaybackLogDirectory"]
        fn open_playback_log_directory(path: &QString) -> bool;
        #[cxx_name = "installQtGcLogging"]
        fn install_qt_gc_logging(callback: fn(category: &str, message: &str));
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
        #[cxx_name = "translateBackend"]
        fn translate_backend(source: &QString) -> QString;
        type QQuickItem;
        include!("pointer_activity.h");
        #[cxx_name = "installPointerActivity"]
        unsafe fn install_pointer_activity(item: *mut QQuickItem);
        #[cxx_name = "configureQtQuickOpenGl"]
        fn configure_qt_quick_open_gl();
        #[cxx_name = "qQuickItemAddress"]
        unsafe fn q_quick_item_address(item: *mut QQuickItem) -> usize;
    }
    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QString, language, READ, NOTIFY)]
        #[qproperty(QString, ui_language, READ, NOTIFY)]
        #[qproperty(QString, server, READ, NOTIFY)]
        #[qproperty(QString, status, READ, NOTIFY)]
        #[qproperty(QString, playback_error, READ, NOTIFY)]
        #[qproperty(QString, playback_message, READ, NOTIFY)]
        #[qproperty(QString, log_error, READ, NOTIFY)]
        #[qproperty(QString, channel_data, READ, NOTIFY)]
        #[qproperty(QString, channel_program_data, READ, NOTIFY)]
        #[qproperty(QString, channel_visibility_data, READ, NOTIFY)]
        #[qproperty(f64, channel_program_now, READ, NOTIFY)]
        #[qproperty(i32, selected, READ, NOTIFY)]
        #[qproperty(bool, loading, READ, NOTIFY)]
        #[qproperty(bool, playing, READ, NOTIFY)]
        #[qproperty(bool, subtitles_enabled, READ, NOTIFY)]
        #[qproperty(bool, epg_enabled, READ, NOTIFY)]
        #[qproperty(bool, comments_enabled, READ, NOTIFY)]
        #[qproperty(bool, danmaku_enabled, READ, NOTIFY)]
        #[qproperty(f64, comment_font_size, READ, NOTIFY)]
        #[qproperty(f64, comment_opacity, READ, NOTIFY)]
        #[qproperty(f64, comment_speed, READ, NOTIFY)]
        #[qproperty(bool, comments_allowed, READ, NOTIFY)]
        #[qproperty(QString, comment_data, READ, NOTIFY)]
        #[qproperty(QString, activity_data, READ, NOTIFY)]
        #[qproperty(QString, comment_status, READ, NOTIFY)]
        #[qproperty(bool, subtitles_allowed, READ, NOTIFY)]
        #[qproperty(bool, epg_allowed, READ, NOTIFY)]
        #[qproperty(bool, subtitles_active, READ, NOTIFY)]
        #[qproperty(bool, subtitle_display, READ, NOTIFY)]
        #[qproperty(QString, subtitle_data, READ, NOTIFY)]
        #[qproperty(QString, subtitle_status, READ, NOTIFY)]
        #[qproperty(QString, epg_data, READ, NOTIFY)]
        #[qproperty(QString, epg_status, READ, NOTIFY)]
        #[qproperty(QString, current_program_data, READ, NOTIFY)]
        #[qproperty(f64, program_progress, READ, NOTIFY)]
        #[qproperty(f64, volume_level, READ, NOTIFY)]
        #[qproperty(bool, audio_muted, READ, NOTIFY)]
        #[qproperty(QString, settings_error, READ, NOTIFY)]
        #[qproperty(QString, diagnostics, READ, NOTIFY)]
        type Player = super::PlayerRust;
        #[qinvokable]
        fn request_language(self: Pin<&mut Player>, language: QString) -> bool;
        #[qinvokable]
        fn configure_features(self: Pin<&mut Player>, subtitles: bool, epg: bool);
        #[qinvokable]
        fn display_subtitles(self: Pin<&mut Player>, display: bool);
        #[qinvokable]
        fn poll_subtitles(self: Pin<&mut Player>);
        #[qinvokable]
        fn browser_open(self: Pin<&mut Player>, open: bool);
        #[qinvokable]
        fn guide_open(self: Pin<&mut Player>, open: bool);
        #[qinvokable]
        fn guide_day(self: Pin<&mut Player>, start: f64, end: f64);
        #[qinvokable]
        fn watch_program(self: Pin<&mut Player>, key: QString) -> QString;
        #[qinvokable]
        fn refresh_epg(self: Pin<&mut Player>);
        #[qinvokable]
        unsafe fn attach(self: Pin<&mut Player>, item: *mut QQuickItem) -> bool;
        #[qinvokable]
        unsafe fn observe_pointer(self: &Player, item: *mut QQuickItem);
        #[qinvokable]
        fn connect_server(self: Pin<&mut Player>, server: QString);
        #[qinvokable]
        fn select(self: Pin<&mut Player>, index: i32);
        #[qinvokable]
        fn play(self: Pin<&mut Player>);
        #[qinvokable]
        fn stop(self: Pin<&mut Player>);
        #[qinvokable]
        fn video_stats(self: &Player) -> QString;
        #[qinvokable]
        fn subtitle_glyph_outline(self: &Player, text: QString, font: QFont) -> QString;
        #[qinvokable]
        fn volume(self: Pin<&mut Player>, value: f64);
        #[qinvokable]
        fn mute(self: Pin<&mut Player>, muted: bool);
        #[qinvokable]
        fn poll(self: Pin<&mut Player>);
        #[qinvokable]
        fn shutdown(self: Pin<&mut Player>);
        #[qinvokable]
        fn record_ui_state(self: Pin<&mut Player>, guide: bool, channels: bool, live_comments: i32);
        #[qinvokable]
        fn open_log_folder(self: Pin<&mut Player>) -> bool;
        #[qinvokable]
        fn save_settings(self: Pin<&mut Player>);
        #[qinvokable]
        fn enable_comments(self: Pin<&mut Player>, enabled: bool);
        #[qsignal]
        #[cxx_name = "commentReceived"]
        fn comment_received(self: Pin<&mut Player>, text: QString);
        #[qinvokable]
        fn configure_danmaku(
            self: Pin<&mut Player>,
            enabled: bool,
            size: f64,
            opacity: f64,
            speed: f64,
        ) -> bool;
        #[qinvokable]
        fn comments_open(self: Pin<&mut Player>, opened: bool);
        #[qinvokable]
        fn refresh_channels(self: Pin<&mut Player>, force: bool);
        #[qinvokable]
        fn step_channel(self: Pin<&mut Player>, offset: i32);
        #[qinvokable]
        fn audio_tracks(self: &Player) -> QString;
        #[qinvokable]
        fn select_audio(self: &Player, id: QString) -> QString;
        #[qinvokable]
        fn audio_error(self: &Player) -> QString;
    }
}

use crate::{
    features::{program_info::ProgramInfo, subtitles},
    playback, services, settings,
};
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::pin::Pin;
use std::time::Instant;

pub struct PlayerRust {
    language: QString,
    ui_language: QString,
    server: QString,
    status: QString,
    lifecycle_status: PlaybackStatus,
    playback_error: QString,
    playback_message: QString,
    log_error: QString,
    diagnostic_recorder: Option<viewer_diagnostics::recorder::Recorder>,
    diagnostic_ui: telemetry::UiState,
    subtitle_cells: usize,
    error_log: Result<crate::error_log::ErrorLog, crate::error_log::Error>,
    channel_data: QString,
    channel_program_data: QString,
    channel_visibility_data: QString,
    channel_program_now: f64,
    browser_projection: Option<crate::features::program_info::browser::Projection>,
    selected: i32,
    catalog_selection: channels::SelectionPolicy,
    loading: bool,
    playing: bool,
    subtitles_enabled: bool,
    epg_enabled: bool,
    comments_enabled: bool,
    comments_allowed: bool,
    danmaku_enabled: bool,
    comment_font_size: f64,
    comment_opacity: f64,
    comment_speed: f64,
    comment_data: QString,
    activity_data: QString,
    activity: crate::features::comments::activity::Activity,
    comment_status: QString,
    comments_visible: bool,
    comments: crate::features::comments::Comments,
    subtitles_allowed: bool,
    epg_allowed: bool,
    subtitles_active: bool,
    subtitle_display: bool,
    subtitle_data: QString,
    subtitle_status: QString,
    subtitle_phase: subtitle_status::Status,
    epg_data: QString,
    epg_events: viewer_epg_events::controller::Controller,
    epg_status: QString,
    current_program_data: QString,
    program_progress: f64,
    current_projection: crate::features::program_info::presentation::Projection,
    next_current_program: Instant,
    diagnostics: QString,
    volume_level: f64,
    audio_muted: bool,
    audio_output: playback::audio_output::Output,
    settings_error: QString,
    preferences: settings::Session,
    autoplay_pending: bool,
    subtitle_session: Option<subtitles::Session>,
    epg: ProgramInfo,
    active_service: Option<u64>,
    resume_retry_used: bool,
    guide: crate::features::program_info::guide::Guide,
    guide_dirty: bool,
    guide_revision: u64,
    guide_service: Option<crate::channels::BroadcastService>,
    next_diagnostic: Instant,
    request: Option<services::Request>,
    channel_refresh: channel_refresh::Refresh,
    network: Option<services::Network>,
    playback: Option<playback::Playback>,
    entries: Vec<crate::channels::Channel>,
}

macro_rules! property_setter {
    ($method:ident, $field:ident, $signal:ident, $ty:ty) => {
        fn $method(mut self: Pin<&mut Self>, value: $ty) {
            if self.rust().$field != value {
                self.as_mut().rust_mut().$field = value;
                self.as_mut().$signal();
            }
        }
    };
}

impl ffi::Player {
    property_setter!(
        set_comments_enabled,
        comments_enabled,
        comments_enabled_changed,
        bool
    );
    property_setter!(
        set_activity_data,
        activity_data,
        activity_data_changed,
        QString
    );
    property_setter!(
        set_comment_data,
        comment_data,
        comment_data_changed,
        QString
    );
    property_setter!(
        set_comment_status,
        comment_status,
        comment_status_changed,
        QString
    );
    property_setter!(
        set_danmaku_enabled,
        danmaku_enabled,
        danmaku_enabled_changed,
        bool
    );
    property_setter!(
        set_comment_font_size,
        comment_font_size,
        comment_font_size_changed,
        f64
    );
    property_setter!(
        set_comment_opacity,
        comment_opacity,
        comment_opacity_changed,
        f64
    );
    property_setter!(set_comment_speed, comment_speed, comment_speed_changed, f64);
    property_setter!(set_log_error, log_error, log_error_changed, QString);
    property_setter!(set_language, language, language_changed, QString);
    property_setter!(set_ui_language, ui_language, ui_language_changed, QString);
    property_setter!(set_server, server, server_changed, QString);
    property_setter!(set_status, status, status_changed, QString);
    property_setter!(
        set_playback_message,
        playback_message,
        playback_message_changed,
        QString
    );
    property_setter!(
        set_playback_error,
        playback_error,
        playback_error_changed,
        QString
    );
    property_setter!(
        set_channel_data,
        channel_data,
        channel_data_changed,
        QString
    );
    property_setter!(
        set_channel_program_data,
        channel_program_data,
        channel_program_data_changed,
        QString
    );
    property_setter!(
        set_channel_program_now,
        channel_program_now,
        channel_program_now_changed,
        f64
    );
    property_setter!(set_selected, selected, selected_changed, i32);
    property_setter!(set_loading, loading, loading_changed, bool);
    property_setter!(set_playing, playing, playing_changed, bool);
    property_setter!(
        set_subtitles_enabled,
        subtitles_enabled,
        subtitles_enabled_changed,
        bool
    );
    property_setter!(set_epg_enabled, epg_enabled, epg_enabled_changed, bool);
    property_setter!(
        set_subtitles_active,
        subtitles_active,
        subtitles_active_changed,
        bool
    );
    property_setter!(
        set_subtitle_display,
        subtitle_display,
        subtitle_display_changed,
        bool
    );
    property_setter!(
        set_subtitle_data,
        subtitle_data,
        subtitle_data_changed,
        QString
    );
    property_setter!(
        set_subtitle_status,
        subtitle_status,
        subtitle_status_changed,
        QString
    );
    property_setter!(set_epg_data, epg_data, epg_data_changed, QString);
    property_setter!(
        set_current_program_data,
        current_program_data,
        current_program_data_changed,
        QString
    );
    property_setter!(
        set_program_progress,
        program_progress,
        program_progress_changed,
        f64
    );
    property_setter!(set_epg_status, epg_status, epg_status_changed, QString);
    property_setter!(set_diagnostics, diagnostics, diagnostics_changed, QString);
    property_setter!(set_audio_muted, audio_muted, audio_muted_changed, bool);
    property_setter!(set_volume_level, volume_level, volume_level_changed, f64);
    property_setter!(
        set_settings_error,
        settings_error,
        settings_error_changed,
        QString
    );

    property_setter!(
        set_channel_visibility_data,
        channel_visibility_data,
        channel_visibility_data_changed,
        QString
    );

    /// READY joins streaming callbacks before dropping their subscriptions/state.
    fn end_stream(mut self: Pin<&mut Self>) -> Result<(), playback::Error> {
        // Reveal controls even if the native stop itself fails.
        self.as_mut().set_playing(false);
        if let Some(playback) = &self.rust().playback {
            playback.stop()?;
        }
        self.as_mut().rust_mut().subtitle_session = None;
        self.as_mut().rust_mut().active_service = None;
        self.as_mut().set_subtitles_active(false);
        self.as_mut().rust_mut().subtitle_cells = 0;
        self.as_mut().set_subtitle_data(QString::default());
        self.as_mut()
            .update_subtitle_status(subtitle_status::Status::Stopped);
        Ok(())
    }
    pub fn configure_features(mut self: Pin<&mut Self>, subtitles: bool, epg: bool) {
        let subtitles = subtitles && self.rust().subtitles_allowed;
        let epg = epg && self.rust().epg_allowed;
        let restart =
            subtitles != self.rust().subtitles_enabled && self.rust().active_service.is_some();
        if restart && let Err(error) = self.as_mut().end_stream() {
            self.status_error(StatusFailure::Operation, error);
            return;
        }
        self.as_mut().set_subtitles_enabled(subtitles);
        self.as_mut().set_epg_enabled(epg);
        {
            let mut this = self.as_mut().rust_mut();
            let prefs = this.preferences.preferences_mut();
            prefs.subtitles_enabled = subtitles;
            prefs.epg_enabled = epg;
        }
        self.as_mut().configure_epg();
        self.as_mut().save_settings();
        if restart {
            self.as_mut().play();
        }
    }
    fn poll_features(mut self: Pin<&mut Self>) {
        // Consume notifications before acquisition so a pending change can start now.
        self.as_mut().poll_epg_events();
        self.as_mut().poll_comments();
        self.as_mut().poll_epg();
        self.poll_feature_metrics();
    }
    /// QML supplies a live GUI-thread item and calls shutdown before destroying it.
    pub unsafe fn attach(mut self: Pin<&mut Self>, item: *mut ffi::QQuickItem) -> bool {
        let address = unsafe { ffi::q_quick_item_address(item) };
        let result = self
            .as_mut()
            .rust_mut()
            .playback
            .as_mut()
            .ok_or(playback::Error::Unavailable)
            .and_then(|p| unsafe { p.attach(address) });
        if let Err(error) = result {
            self.playback_failed(error);
            return false;
        }
        true
    }
    /// The QML GUI-thread item owns the observer and declares activity().
    pub unsafe fn observe_pointer(&self, item: *mut ffi::QQuickItem) {
        // The caller guarantees a live item; the native helper accepts null.
        unsafe { ffi::install_pointer_activity(item) };
    }
    pub fn connect_server(mut self: Pin<&mut Self>, server: QString) {
        self.as_mut().rust_mut().epg_events.configure(None);
        self.as_mut().rust_mut().catalog_selection = channels::SelectionPolicy::Initial;
        self.as_mut().rust_mut().channel_refresh = channel_refresh::Refresh::Disabled;
        self.as_mut().rust_mut().comments.configure(false, None);
        self.as_mut().rust_mut().activity.configure(false);
        self.as_mut().set_activity_data(QString::from("[]"));
        self.as_mut().clear_playback_failure();
        self.as_mut().rust_mut().request = None;
        self.as_mut().set_loading(false);
        self.as_mut().rust_mut().epg.configure(None);
        self.as_mut().set_epg_data(QString::from("[]"));
        if let Err(error) = self.as_mut().end_stream() {
            self.playback_failed(error);
            return;
        }
        self.as_mut().set_channel_program_data(QString::from("[]"));
        self.as_mut().rust_mut().entries.clear();
        self.as_mut().set_channel_data(QString::from("[]"));
        self.as_mut().set_selected(-1);
        let server = match services::server_url(&server.to_string()) {
            Ok(server) => server,
            Err(error) => {
                self.status_error(StatusFailure::Server, error);
                return;
            }
        };
        let Some(network) = &self.rust().network else {
            self.update_status(PlaybackStatus::NetworkUnavailable);
            return;
        };
        let request = network.fetch(&server);
        self.as_mut()
            .rust_mut()
            .preferences
            .preferences_mut()
            .apply_overrides(Some(server.clone()), None);
        self.as_mut().rust_mut().request = Some(request);
        self.as_mut()
            .rust_mut()
            .channel_refresh
            .requested(Instant::now());
        self.as_mut().set_server(QString::from(server));
        self.as_mut().configure_epg_events();
        self.as_mut().set_loading(true);
        self.as_mut().save_settings();
        self.update_status(PlaybackStatus::Loading);
    }
    pub fn select(mut self: Pin<&mut Self>, index: i32) {
        if index < 0 || index as usize >= self.rust().entries.len() {
            return;
        }
        let id = self.rust().entries[index as usize].id;
        self.as_mut()
            .rust_mut()
            .preferences
            .preferences_mut()
            .service_id = id.to_string();
        self.as_mut().set_selected(index);
        self.record_diagnostic(viewer_diagnostics::recorder::Event::ChannelSelected);
        self.as_mut().save_settings();
        self.play();
    }
    pub fn play(mut self: Pin<&mut Self>) {
        self.record_diagnostic(viewer_diagnostics::recorder::Event::PlayRequested);
        self.as_mut().clear_playback_failure();
        self.as_mut().rust_mut().resume_retry_used = false;
        self.start_stream();
    }
    fn start_stream(mut self: Pin<&mut Self>) {
        let Some(entry) = self.rust().entries.get(*self.selected() as usize) else {
            return;
        };
        let (id, name, broadcast) = (entry.id, entry.name.clone(), entry.broadcast);
        let server = self.server().to_string();
        if self.rust().active_service == Some(id) {
            return;
        }
        if let Err(error) = self.as_mut().end_stream() {
            self.playback_failed(error);
            return;
        }
        if self.rust().subtitles_enabled {
            let result = self
                .rust()
                .playback
                .as_ref()
                .ok_or(subtitles::Error::PlaybackUnavailable)
                .and_then(|p| subtitles::Session::start(p.element(), broadcast));
            match result {
                Ok(session) => {
                    self.as_mut().rust_mut().subtitle_session = Some(session);
                    self.as_mut().set_subtitles_active(true);
                    self.as_mut()
                        .update_subtitle_status(subtitle_status::Status::Parsing);
                }
                Err(error) => self
                    .as_mut()
                    .update_subtitle_status(subtitle_status::Status::Failed(error)),
            }
        }
        let result = self
            .rust()
            .playback
            .as_ref()
            .map(|p| p.play(&server, id, broadcast));
        match result {
            Some(Ok(_)) => {
                self.as_mut()
                    .rust_mut()
                    .preferences
                    .preferences_mut()
                    .service_id = id.to_string();
                self.as_mut().rust_mut().active_service = Some(id);
                self.as_mut()
                    .update_status(PlaybackStatus::Connecting(name));
            }
            Some(Err(error)) => {
                let failure = match self.as_mut().end_stream() {
                    Ok(()) => error,
                    Err(cleanup) => playback::Error::Cleanup {
                        primary: Box::new(error),
                        cleanup: Box::new(cleanup),
                    },
                };
                self.as_mut().playback_failed(failure);
            }
            None => self.as_mut().playback_failed(playback::Error::Unavailable),
        }
    }
    pub fn stop(mut self: Pin<&mut Self>) {
        // An explicit stop supersedes startup autoplay, including a still-empty catalog.
        self.as_mut().rust_mut().autoplay_pending = false;
        self.record_diagnostic(viewer_diagnostics::recorder::Event::StopRequested);
        match self.as_mut().end_stream() {
            Ok(()) => self.update_status(PlaybackStatus::Stopped),
            Err(error) => self.playback_failed(error),
        }
    }
    pub fn poll(mut self: Pin<&mut Self>) {
        self.as_mut().refresh_channels_if_due();
        let fetched = self
            .rust()
            .request
            .as_ref()
            .and_then(services::Request::poll);
        if let Some(result) = fetched {
            self.as_mut().rust_mut().request = None;
            self.as_mut().set_loading(false);
            match result {
                Ok(entries) => {
                    // Unchanged catalogs must not rebuild the guide or browser payloads.
                    if self.rust().entries != entries {
                        let presentation =
                            match channels::presentation(&entries, &self.server().to_string()) {
                                Ok(json) => QString::from(json),
                                Err(error) => {
                                    self.status_error(StatusFailure::ChannelPresentation, error);
                                    return;
                                }
                            };
                        let selected = channels::selected_after_update(
                            self.rust().catalog_selection,
                            &self.rust().entries,
                            self.rust().selected,
                            &entries,
                            self.rust().preferences.preferences(),
                        )
                        .and_then(|index| i32::try_from(index).ok())
                        .unwrap_or(-1);
                        self.as_mut().rust_mut().entries = entries;
                        self.as_mut().rust_mut().activity.dirty = true;
                        self.as_mut().rust_mut().guide_dirty = true;
                        // Physical channel metadata also affects subchannel visibility.
                        // Reset the small browser projection even if EPG revision is unchanged.
                        if self.rust().browser_projection.is_some() {
                            self.as_mut().rust_mut().browser_projection = Some(Default::default());
                        }
                        self.as_mut().rust_mut().next_current_program = Instant::now();
                        self.as_mut().set_channel_data(presentation);
                        self.as_mut().set_selected(selected);
                        self.as_mut().configure_epg();
                    }
                    if !self.rust().entries.is_empty() {
                        self.as_mut().rust_mut().catalog_selection =
                            channels::SelectionPolicy::Preserve;
                    }
                    let status = if self.rust().entries.is_empty() {
                        PlaybackStatus::Empty
                    } else {
                        PlaybackStatus::Select
                    };
                    if self.rust().active_service.is_none() {
                        self.as_mut().update_status(status);
                    }
                    if self.rust().autoplay_pending && self.rust().selected >= 0 {
                        self.as_mut().rust_mut().autoplay_pending = false;
                        self.as_mut().play();
                    }
                }
                Err(error) => self
                    .as_mut()
                    .status_error(StatusFailure::ChannelFetch, error),
            }
        }
        self.as_mut().poll_features();
        let result = self.rust().playback.as_ref().map(playback::Playback::poll);
        self.poll_audio_choice();
        match result {
            Some(Ok(true)) => {
                self.as_mut().clear_playback_failure();
                self.as_mut().set_playing(true);
                if let Some(entry) = self.rust().entries.get(*self.selected() as usize) {
                    let status = PlaybackStatus::Playing(entry.name.clone());
                    eprintln!("Pipeline PLAYING service {}", entry.id);
                    self.as_mut().update_status(status);
                }
            }
            Some(Err(error)) => {
                let text = error.to_string();
                eprintln!("Playback error: {text}");
                let recover = error.is_live_resume_rejected()
                    && self.rust().active_service.is_some()
                    && !self.rust().resume_retry_used;
                if let Err(stop_error) = self.as_mut().end_stream() {
                    self.playback_failed(playback::Error::Cleanup {
                        primary: Box::new(error),
                        cleanup: Box::new(stop_error),
                    });
                    return;
                }
                if recover {
                    self.as_mut().rust_mut().resume_retry_used = true;
                    eprintln!("Live resume rejected; opening one fresh stream connection");
                    self.as_mut().start_stream();
                    if self.rust().active_service.is_some() {
                        self.update_status(PlaybackStatus::Reconnecting);
                    }
                } else {
                    self.playback_failed(error);
                }
            }
            _ => {}
        }
    }
    pub fn shutdown(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().epg_events.configure(None);
        self.as_mut().rust_mut().channel_refresh = channel_refresh::Refresh::Disabled;
        self.as_mut().stop_diagnostics();
        self.as_mut().rust_mut().comments.configure(false, None);
        self.as_mut().rust_mut().activity.configure(false);
        self.as_mut().set_activity_data(QString::from("[]"));
        self.as_mut().browser_open(false);
        self.as_mut().rust_mut().epg.configure(None);
        self.as_mut().guide_open(false);
        let _ = self.as_mut().end_stream();
        self.as_mut().rust_mut().request = None;
        if let Some(playback) = self.as_mut().rust_mut().playback.as_mut() {
            playback.shutdown();
        }
        self.save_settings();
    }
}

impl Drop for PlayerRust {
    fn drop(&mut self) {
        self.epg_events.configure(None);
        // Qt normally calls shutdown; also cover a failed QML construction.
        if let Some(playback) = self.playback.as_mut() {
            playback.shutdown();
        }
        self.subtitle_session = None;
        self.epg.configure(None);
    }
}
