mod audio_output;
mod audio_streams;
mod channel_programs;
mod channel_refresh;
mod channels;
mod comment_posting;
mod comments;
mod connection;
#[cfg(feature = "native_tests")]
pub(crate) mod connection_checks;
mod epg;
mod guide;
mod language;
mod lifecycle;
use lifecycle::{Failure as StatusFailure, Status as PlaybackStatus};
mod playback_failure;
mod preferences;
mod program_info;
mod recordings;
mod remote;
#[cfg(feature = "native_tests")]
mod remote_checks;
mod screenshots;
mod startup;
mod statistics;
mod status;
mod stream;
mod stream_state;
mod subtitle_rendering;
mod subtitle_status;
mod telemetry;

#[cxx_qt::bridge]
pub mod ffi {
    #[cfg(feature = "native_tests")]
    unsafe extern "C++" {
        include!("cxx-qt-lib/common.h");
        #[namespace = "rust::cxxqtlib1"]
        #[cxx_name = "make_unique"]
        fn new_player() -> UniquePtr<Player>;
    }
    unsafe extern "C++" {
        include!("mirakurun-viewer/src/comment_model.cxxqt.h");
        type CommentModel = crate::comment_model::ffi::CommentModel;
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qurl.h");
        type QUrl = cxx_qt_lib::QUrl;
        include!("cxx-qt-lib/qimage.h");
        type QImage = cxx_qt_lib::QImage;
        include!("cxx-qt-lib/qfont.h");
        type QFont = cxx_qt_lib::QFont;
        include!("subtitle_outline.h");
        #[cxx_name = "subtitleOutlinePath"]
        fn subtitle_outline_path(text: &QString, font: &QFont) -> QString;
        include!("qt_helpers.h");
        #[cxx_name = "picturesDirectory"]
        fn pictures_directory() -> QString;
        #[cxx_name = "saveScreenshotPng"]
        fn save_screenshot_png(image: &QImage, path: &QString) -> bool;
        #[cxx_name = "playbackLogDirectory"]
        fn playback_log_directory() -> QString;
        #[cxx_name = "openLocalDirectory"]
        fn open_local_directory(path: &QString) -> bool;
        #[cxx_name = "installQtLogging"]
        fn install_qt_logging(callback: fn(level: u8, category: &str, message: &str));
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
        include!("video_item.h");
        #[cxx_name = "qml6VideoItemPointer"]
        unsafe fn qml6_video_item_pointer(item: *mut QQuickItem) -> *mut u8;
    }
    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QString, language, READ, NOTIFY)]
        #[qproperty(QString, ui_language, READ, NOTIFY)]
        #[qproperty(bool, autoplay, READ = autoplay, NOTIFY)]
        #[qproperty(bool, remote_enabled, READ = remote_enabled, NOTIFY)]
        #[qproperty(QString, remote_address, READ = remote_address, NOTIFY)]
        #[qproperty(i32, remote_port, READ = remote_port, NOTIFY)]
        #[qproperty(QString, remote_status, READ = remote_status, NOTIFY)]
        #[qproperty(QString, remote_error, READ = remote_error, NOTIFY)]
        #[qproperty(QString, remote_save_error, READ = remote_save_error, NOTIFY)]
        #[qproperty(QString, remote_endpoints, READ = remote_endpoints, NOTIFY)]
        #[qproperty(bool, remote_session_only, READ = remote_session_only, NOTIFY)]
        #[qproperty(QString, screenshot_directory, READ = screenshot_directory, NOTIFY)]
        #[qproperty(QString, screenshot_error, READ, NOTIFY)]
        #[qproperty(QString, server, READ, NOTIFY)]
        #[qproperty(bool, server_configured, READ = server_configured, NOTIFY)]
        #[qproperty(QString, status, READ, NOTIFY)]
        #[qproperty(QString, playback_error, READ, NOTIFY)]
        #[qproperty(QString, playback_message, READ, NOTIFY)]
        #[qproperty(QString, log_error, READ, NOTIFY)]
        #[qproperty(QString, channel_data, READ, NOTIFY)]
        #[qproperty(QString, channel_program_data, READ, NOTIFY)]
        #[qproperty(QString, channel_visibility_data, READ, NOTIFY)]
        #[qproperty(QString, guide_visibility_data, READ, NOTIFY)]
        #[qproperty(f64, channel_program_now, READ, NOTIFY)]
        #[qproperty(i32, selected, READ, NOTIFY)]
        #[qproperty(bool, loading, READ = loading, NOTIFY)]
        #[qproperty(bool, connecting, READ = connecting, NOTIFY)]
        #[qproperty(bool, playing, READ = playing, NOTIFY)]
        #[qproperty(bool, recording, READ = recording, NOTIFY)]
        #[qproperty(QString, recording_name, READ = recording_name, NOTIFY)]
        #[qproperty(QString, file_error, READ, NOTIFY)]
        #[qproperty(bool, recording_loading, READ = recording_loading, NOTIFY)]
        #[qproperty(bool, subtitles_enabled, READ, CONSTANT)]
        #[qproperty(bool, epg_enabled, READ, CONSTANT)]
        #[qproperty(bool, comments_enabled, READ, NOTIFY)]
        #[qproperty(bool, danmaku_enabled, READ, NOTIFY)]
        #[qproperty(f64, comment_font_size, READ, NOTIFY)]
        #[qproperty(f64, comment_opacity, READ, NOTIFY)]
        #[qproperty(f64, comment_speed, READ, NOTIFY)]
        #[qproperty(bool, comment_shadow_enabled, READ, NOTIFY)]
        #[qproperty(bool, comments_allowed, READ, NOTIFY)]
        #[qproperty(QString, comment_program_title, READ, NOTIFY)]
        #[qproperty(*mut CommentModel, comment_model, READ = comment_model, CONSTANT)]
        #[qproperty(QString, activity_data, READ, NOTIFY)]
        #[qproperty(QString, comment_status, READ, NOTIFY)]
        #[qproperty(QString, comment_draft, READ, NOTIFY)]
        #[qproperty(QString, comment_post_status, READ, NOTIFY)]
        #[qproperty(QString, comment_post_target, READ, NOTIFY)]
        #[qproperty(bool, comment_post_available, READ, NOTIFY)]
        #[qproperty(bool, comment_post_busy, READ, NOTIFY)]
        #[qproperty(bool, comment_send_on_enter, READ, NOTIFY)]
        #[qproperty(bool, subtitles_active, READ, NOTIFY)]
        #[qproperty(bool, subtitle_display, READ, NOTIFY)]
        #[qproperty(QString, subtitle_data, READ, NOTIFY)]
        #[qproperty(QString, subtitle_status, READ, NOTIFY)]
        #[qproperty(QString, epg_data, READ, NOTIFY)]
        #[qproperty(QString, epg_status, READ, NOTIFY)]
        #[qproperty(bool, guide_visible, READ = guide_visible, NOTIFY)]
        #[qproperty(QString, current_program_data, READ, NOTIFY)]
        #[qproperty(f64, program_progress, READ, NOTIFY)]
        #[qproperty(f64, volume_level, READ, NOTIFY)]
        #[qproperty(bool, audio_muted, READ, NOTIFY)]
        #[qproperty(QString, settings_error, READ, NOTIFY)]
        #[qproperty(QString, diagnostics, READ, NOTIFY)]
        type Player = super::PlayerRust;
        fn autoplay(self: &Player) -> bool;
        fn remote_enabled(self: &Player) -> bool;
        fn remote_address(self: &Player) -> QString;
        fn remote_port(self: &Player) -> i32;
        fn remote_status(self: &Player) -> QString;
        fn remote_error(self: &Player) -> QString;
        fn remote_save_error(self: &Player) -> QString;
        fn remote_endpoints(self: &Player) -> QString;
        fn remote_session_only(self: &Player) -> bool;
        #[qinvokable]
        fn configure_remote(
            self: Pin<&mut Player>,
            enabled: bool,
            address: QString,
            port: i32,
        ) -> bool;
        #[qinvokable]
        fn refresh_remote_addresses(self: Pin<&mut Player>);
        fn screenshot_directory(self: &Player) -> QString;
        #[qinvokable]
        fn screenshot_directory_url(self: &Player) -> QUrl;
        #[qinvokable]
        fn configure_screenshot_directory(self: Pin<&mut Player>, directory: QUrl) -> bool;
        #[qinvokable]
        fn reset_screenshot_directory(self: Pin<&mut Player>) -> bool;
        #[qinvokable]
        fn open_screenshot_directory(self: Pin<&mut Player>) -> bool;
        #[qinvokable]
        fn save_screenshot(self: Pin<&mut Player>, image: &QImage) -> QUrl;
        fn server_configured(self: &Player) -> bool;
        fn loading(self: &Player) -> bool;
        fn connecting(self: &Player) -> bool;
        fn playing(self: &Player) -> bool;
        fn recording(self: &Player) -> bool;
        fn recording_name(self: &Player) -> QString;
        fn guide_visible(self: &Player) -> bool;
        #[qinvokable]
        fn request_language(self: Pin<&mut Player>, language: QString) -> bool;
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
        fn connect_server(self: Pin<&mut Player>, server: QString) -> bool;
        #[qsignal]
        #[cxx_name = "connectionFinished"]
        fn connection_finished(self: Pin<&mut Player>, success: bool, channels: i32);
        #[qinvokable]
        fn select(self: Pin<&mut Player>, index: i32);
        #[qinvokable]
        fn play(self: Pin<&mut Player>);
        #[qinvokable]
        fn open_recording(self: Pin<&mut Player>, file: QUrl) -> bool;
        fn recording_loading(self: &Player) -> bool;
        #[qsignal]
        #[cxx_name = "recordingOpened"]
        fn recording_opened(self: Pin<&mut Player>, success: bool);
        #[qinvokable]
        fn cancel_recording_open(self: Pin<&mut Player>);
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
        fn shutdown(self: Pin<&mut Player>) -> bool;
        #[qinvokable]
        fn record_ui_state(self: Pin<&mut Player>, guide: bool, channels: bool, live_comments: i32);
        #[qinvokable]
        fn open_log_folder(self: Pin<&mut Player>) -> bool;
        #[qinvokable]
        fn save_settings(self: Pin<&mut Player>);
        #[qinvokable]
        fn configure_autoplay(self: Pin<&mut Player>, enabled: bool);
        #[qinvokable]
        fn enable_comments(self: Pin<&mut Player>, enabled: bool);
        #[qinvokable]
        fn edit_comment_draft(self: Pin<&mut Player>, text: QString);
        #[qinvokable]
        fn post_comment(self: Pin<&mut Player>) -> bool;
        #[qinvokable]
        fn configure_comment_send_on_enter(self: Pin<&mut Player>, enabled: bool);
        #[qinvokable]
        fn configure_comment_shadow(self: Pin<&mut Player>, enabled: bool);
        #[qsignal]
        #[cxx_name = "commentReceived"]
        fn comment_received(
            self: Pin<&mut Player>,
            text: QString,
            position: QString,
            color: u32,
            own: bool,
        );
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
        fn comment_model(self: &Player) -> *mut CommentModel;
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

use crate::{features::program_info::ProgramInfo, playback, services, settings};
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::pin::Pin;
use std::time::Instant;

pub struct PlayerRust {
    language: QString,
    ui_language: QString,
    server: QString,
    pending_server: Option<services::ServerUrl>,
    status: QString,
    lifecycle_status: PlaybackStatus,
    playback_error: QString,
    playback_message: QString,
    log_error: QString,
    diagnostic_recorder: Option<crate::diagnostics::Client>,
    diagnostic_ui: telemetry::UiState,
    subtitle_cells: usize,
    error_log: Result<crate::error_log::ErrorLog, crate::error_log::Error>,
    channel_data: QString,
    channel_program_data: QString,
    channel_visibility_data: QString,
    guide_visibility_data: QString,
    channel_program_now: f64,
    browser_projection: Option<crate::features::program_info::browser::Projection>,
    selected: i32,
    catalog_selection: channels::SelectionPolicy,
    stream_state: stream_state::State,
    recording_loader: playback::recording::Loader,
    file_error: QString,
    subtitles_enabled: bool,
    epg_enabled: bool,
    comments_enabled: bool,
    comments_allowed: bool,
    danmaku_enabled: bool,
    comment_font_size: f64,
    comment_opacity: f64,
    comment_speed: f64,
    comment_shadow_enabled: bool,
    comment_program_title: QString,
    comment_model: cxx::UniquePtr<crate::comment_model::ffi::CommentModel>,
    activity_data: QString,
    activity: crate::features::comments::activity::Activity,
    comment_status: QString,
    comment_draft: QString,
    comment_post_status: QString,
    comment_post_target: QString,
    comment_post_available: bool,
    comment_post_busy: bool,
    comment_send_on_enter: bool,
    comments: crate::features::comments::Comments,
    subtitles_active: bool,
    subtitle_display: bool,
    subtitle_data: QString,
    subtitle_status: QString,
    subtitle_phase: subtitle_status::Status,
    epg_data: QString,
    epg_events: viewer_epg_events::controller::Controller,
    epg_status: QString,
    guide_error: Option<serde_json::Error>,
    current_program_data: QString,
    program_progress: f64,
    current_projection: crate::features::program_info::presentation::Projection,
    next_current_program: Instant,
    diagnostics: QString,
    feature_metrics: Option<telemetry::FeatureMetrics>,
    volume_level: f64,
    audio_muted: bool,
    audio_output: playback::audio_output::Output,
    settings_error: QString,
    screenshot_error: QString,
    preferences: settings::Session,
    autoplay_pending: bool,
    epg: ProgramInfo,
    guide: crate::features::program_info::guide::Guide,
    guide_dirty: bool,
    guide_revision: u64,
    next_diagnostic: Instant,
    request: crate::features::channel_catalog::Acquisition,
    channel_refresh: channel_refresh::Refresh,
    remote: crate::remote::Control,
    network: Option<services::Network>,
    media: playback::Session,
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
    property_setter!(set_file_error, file_error, file_error_changed, QString);
    property_setter!(
        set_comment_draft,
        comment_draft,
        comment_draft_changed,
        QString
    );
    property_setter!(
        set_comment_post_status,
        comment_post_status,
        comment_post_status_changed,
        QString
    );
    property_setter!(
        set_comment_post_target,
        comment_post_target,
        comment_post_target_changed,
        QString
    );
    property_setter!(
        set_comment_post_available,
        comment_post_available,
        comment_post_available_changed,
        bool
    );
    property_setter!(
        set_comment_post_busy,
        comment_post_busy,
        comment_post_busy_changed,
        bool
    );
    property_setter!(
        set_comment_send_on_enter,
        comment_send_on_enter,
        comment_send_on_enter_changed,
        bool
    );
    property_setter!(
        set_comment_program_title,
        comment_program_title,
        comment_program_title_changed,
        QString
    );
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
    property_setter!(
        set_comment_shadow_enabled,
        comment_shadow_enabled,
        comment_shadow_enabled_changed,
        bool
    );
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
        set_screenshot_error,
        screenshot_error,
        screenshot_error_changed,
        QString
    );

    property_setter!(
        set_guide_visibility_data,
        guide_visibility_data,
        guide_visibility_data_changed,
        QString
    );
    property_setter!(
        set_channel_visibility_data,
        channel_visibility_data,
        channel_visibility_data_changed,
        QString
    );

    fn poll_features(mut self: Pin<&mut Self>) {
        // Consume notifications before acquisition so a pending change can start now.
        self.as_mut().poll_epg_events();
        self.as_mut().poll_comments();
        self.as_mut().poll_epg();
        self.poll_feature_metrics();
    }
    /// Attach the video output; null and incompatible items are rejected.
    ///
    /// # Safety
    /// Call on the GUI thread with null or a live QQuickItem owned by that thread.
    /// The item must not be destroyed during this call. On success, its QML owner
    /// must obtain a successful shutdown before destroying it. Native types must use truthful Qt
    /// meta-objects; the GStreamer QML module must come from the installed plugin.
    pub unsafe fn attach(mut self: Pin<&mut Self>, item: *mut ffi::QQuickItem) -> bool {
        // SAFETY: The QML caller supplies the lifetime/thread guarantees above.
        // The session validates the concrete type before passing it to Gst.
        let result = unsafe { self.as_mut().rust_mut().media.attach(item) };
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
}

impl Drop for PlayerRust {
    fn drop(&mut self) {
        self.remote.stop();
        self.epg_events.configure(None);
        // The media owner enforces native shutdown before subtitle destruction,
        // including when QML construction failed before onClosing could run.
        self.media.shutdown_before_drop();
        self.epg.configure(None);
    }
}
