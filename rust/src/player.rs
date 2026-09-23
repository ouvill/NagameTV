mod actions;
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
#[cfg(target_os = "linux")]
mod desktop_media;
mod epg;
mod guide;
mod language;
mod lifecycle;
use lifecycle::{Failure as StatusFailure, Status as PlaybackStatus};
mod playback_failure;
mod preferences;
mod program_info;
mod recording_library;
#[cfg(feature = "native_tests")]
mod recording_library_checks;
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
mod transport;

#[cxx_qt::bridge]
pub mod ffi {
    #[qenum(Player)]
    enum PlaybackAction {
        Unavailable,
        Play,
        Pause,
        Stop,
    }

    #[cfg(feature = "native_tests")]
    unsafe extern "C++" {
        include!("cxx-qt-lib/common.h");
        #[namespace = "rust::cxxqtlib1"]
        #[cxx_name = "make_unique"]
        fn new_player() -> UniquePtr<Player>;
    }
    unsafe extern "C++" {
        include!("nagametv/src/channel_model.cxxqt.h");
        type ChannelModel = crate::channel_model::ffi::ChannelModel;
        include!("nagametv/src/recording_model.cxxqt.h");
        type RecordingModel = crate::recording_model::ffi::RecordingModel;
        include!("nagametv/src/video_file_model.cxxqt.h");
        type VideoFileModel = crate::video_file_model::ffi::VideoFileModel;
        include!("nagametv/src/comment_model.cxxqt.h");
        type CommentModel = crate::comment_model::ffi::CommentModel;
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qurl.h");
        type QUrl = cxx_qt_lib::QUrl;
        include!("cxx-qt-lib/qimage.h");
        type QImage = cxx_qt_lib::QImage;
        include!("cxx-qt-lib/qfont.h");
        type QFont = cxx_qt_lib::QFont;
        include!("QtQuick/QQuickItem");
        type QQuickItem = crate::qt::ffi::QQuickItem;
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
        #[qproperty(QString, screenshot_format, READ = screenshot_format, NOTIFY)]
        #[qproperty(QString, screenshot_options, READ = screenshot_options, NOTIFY)]
        #[qproperty(bool, screenshot_busy, READ = screenshot_busy, NOTIFY)]
        #[qproperty(QString, server, READ, NOTIFY)]
        #[qproperty(bool, server_configured, READ = server_configured, NOTIFY)]
        #[qproperty(QString, status, READ, NOTIFY)]
        #[qproperty(QString, playback_error, READ, NOTIFY)]
        #[qproperty(QString, playback_message, READ, NOTIFY)]
        #[qproperty(QString, log_error, READ, NOTIFY)]
        #[qproperty(*mut ChannelModel, channels, READ = channels, CONSTANT)]
        #[qproperty(*mut RecordingModel, recordings, READ = recordings, CONSTANT)]
        #[qproperty(*mut VideoFileModel, recording_files, READ = recording_files, CONSTANT)]
        #[qproperty(QString, epgstation_server, READ = epgstation_server, NOTIFY = epgstation_changed)]
        #[qproperty(bool, epgstation_busy, READ = epgstation_busy, NOTIFY = epgstation_changed)]
        #[qproperty(bool, epgstation_loaded, READ = epgstation_loaded, NOTIFY = epgstation_changed)]
        #[qproperty(QString, epgstation_error, READ = epgstation_error, NOTIFY = epgstation_changed)]
        #[qproperty(QString, epgstation_total, READ = epgstation_total, NOTIFY = epgstation_changed)]
        #[qproperty(QString, epgstation_page, READ = epgstation_page, NOTIFY = epgstation_changed)]
        #[qproperty(bool, epgstation_previous, READ = epgstation_previous, NOTIFY = epgstation_changed)]
        #[qproperty(bool, epgstation_next, READ = epgstation_next, NOTIFY = epgstation_changed)]
        #[qproperty(QString, channel_program_data, READ, NOTIFY)]
        #[qproperty(QString, channel_visibility_data, READ, NOTIFY)]
        #[qproperty(QString, guide_visibility_data, READ, NOTIFY)]
        #[qproperty(f64, channel_program_now, READ, NOTIFY)]
        #[qproperty(i32, selected, READ = selected, NOTIFY)]
        #[qproperty(i32, viewing_channel, READ = viewing_channel, NOTIFY)]
        #[qproperty(bool, loading, READ = loading, NOTIFY)]
        #[qproperty(bool, connecting, READ = connecting, NOTIFY)]
        #[qproperty(bool, playing, READ = playing, NOTIFY)]
        #[qproperty(PlaybackAction, playback_action, READ = playback_action, NOTIFY)]
        #[qproperty(bool, media_active, READ = media_active, NOTIFY)]
        #[qproperty(bool, paused, READ = paused, NOTIFY)]
        #[qproperty(bool, seeking, READ = seeking, NOTIFY)]
        #[qproperty(bool, ended, READ = ended, NOTIFY)]
        #[qproperty(bool, seekable, READ = seekable, NOTIFY)]
        #[qproperty(f64, position_ms, READ = position_ms, NOTIFY)]
        #[qproperty(f64, duration_ms, READ = duration_ms, NOTIFY)]
        #[qproperty(bool, duration_estimated, READ = duration_estimated, NOTIFY)]
        #[qproperty(i32, playback_rate, READ = playback_rate, NOTIFY)]
        #[qproperty(i32, requested_playback_rate, READ = requested_playback_rate, NOTIFY)]
        #[qproperty(i32, minimum_playback_rate, READ = minimum_playback_rate, CONSTANT)]
        #[qproperty(i32, maximum_playback_rate, READ = maximum_playback_rate, CONSTANT)]
        #[qproperty(bool, speed_available, READ = speed_available, NOTIFY)]
        #[qproperty(QString, speed_reason, READ = speed_reason, NOTIFY)]
        #[qproperty(bool, at_live_edge, READ = at_live_edge, NOTIFY)]
        #[qproperty(bool, timeshift, READ = timeshift, NOTIFY)]
        #[qproperty(f64, window_start_ms, READ = window_start_ms, NOTIFY)]
        #[qproperty(f64, window_end_ms, READ = window_end_ms, NOTIFY)]
        #[qproperty(f64, live_delay_ms, READ = live_delay_ms, NOTIFY)]
        #[qproperty(QString, timeshift_storage, READ = timeshift_storage, NOTIFY)]
        #[qproperty(QString, timeshift_limits, READ = timeshift_limits, NOTIFY)]
        #[qproperty(QString, live_buffer_options, READ = live_buffer_options, NOTIFY)]
        #[qproperty(f64, timeshift_bytes_per_second, READ, NOTIFY)]
        #[qproperty(QString, live_timeline, READ, NOTIFY)]
        #[qproperty(QString, transport_error, READ = transport_error, NOTIFY)]
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
        #[qproperty(QString, comment_display, READ = comment_display, NOTIFY)]
        #[qproperty(QString, comment_placement, READ = comment_placement, NOTIFY)]
        #[qproperty(bool, evaluation_comment_list, READ = evaluation_comment_list, CONSTANT)]
        #[qproperty(bool, evaluation_wide_comments, READ = evaluation_wide_comments, CONSTANT)]
        #[qproperty(bool, evaluation_collision_layout, READ = evaluation_collision_layout, CONSTANT)]
        #[qproperty(f64, video_aspect_ratio, READ, NOTIFY)]
        #[qproperty(bool, comment_shadow_enabled, READ, NOTIFY)]
        #[qproperty(bool, comments_allowed, READ, NOTIFY)]
        #[qproperty(QString, comment_program_title, READ, NOTIFY)]
        #[qproperty(*mut CommentModel, comment_model, READ = comment_model, CONSTANT)]
        #[qproperty(QString, activity_data, READ, NOTIFY)]
        #[qproperty(QString, comment_status, READ, NOTIFY)]
        #[qproperty(QString, comment_timeline, READ, NOTIFY)]
        #[qproperty(f64, comment_cache_bytes, READ, NOTIFY)]
        #[qproperty(i32, comment_cache_limit_mib, READ = comment_cache_limit_mib, NOTIFY)]
        #[qproperty(QString, comment_draft, READ, NOTIFY)]
        #[qproperty(QString, comment_post_status, READ, NOTIFY)]
        #[qproperty(QString, comment_post_target, READ, NOTIFY)]
        #[qproperty(bool, comment_post_available, READ, NOTIFY)]
        #[qproperty(bool, comment_post_busy, READ, NOTIFY)]
        #[qproperty(bool, comment_send_on_enter, READ, NOTIFY)]
        #[qproperty(bool, subtitles_active, READ, NOTIFY)]
        #[qproperty(bool, subtitle_display, READ, NOTIFY)]
        #[qproperty(bool, subtitle_force_outline, READ = subtitle_force_outline, NOTIFY)]
        #[qproperty(QString, subtitle_data, READ, NOTIFY)]
        #[qproperty(QString, subtitle_status, READ, NOTIFY)]
        #[qproperty(QString, epg_data, READ, NOTIFY)]
        #[qproperty(QString, epg_status, READ, NOTIFY)]
        #[qproperty(bool, guide_visible, READ = guide_visible, NOTIFY)]
        #[qproperty(QString, current_program_data, READ, NOTIFY)]
        #[qproperty(f64, program_progress, READ, NOTIFY)]
        #[qproperty(QString, program_status, READ, NOTIFY)]
        #[qproperty(f64, volume_level, READ, NOTIFY)]
        #[qproperty(bool, audio_muted, READ, NOTIFY)]
        #[qproperty(QString, settings_error, READ, NOTIFY)]
        #[qproperty(QString, diagnostics, READ, NOTIFY)]
        #[qproperty(QString, build_info, READ = build_info, CONSTANT)]
        type Player = super::PlayerRust;
        fn channels(self: &Player) -> *mut ChannelModel;
        fn recordings(self: &Player) -> *mut RecordingModel;
        fn epgstation_server(self: &Player) -> QString;
        fn epgstation_busy(self: &Player) -> bool;
        fn epgstation_loaded(self: &Player) -> bool;
        fn epgstation_error(self: &Player) -> QString;
        fn epgstation_total(self: &Player) -> QString;
        fn epgstation_page(self: &Player) -> QString;
        fn epgstation_previous(self: &Player) -> bool;
        fn epgstation_next(self: &Player) -> bool;
        #[qsignal]
        fn epgstation_changed(self: Pin<&mut Player>);
        #[qinvokable]
        fn browse_epgstation(self: Pin<&mut Player>, server: QString, keyword: QString) -> bool;
        #[qinvokable]
        fn login_epgstation(
            self: Pin<&mut Player>,
            server: QString,
            name: QString,
            password: QString,
        ) -> bool;
        #[qinvokable]
        fn epgstation_change_page(self: Pin<&mut Player>, forward: bool);
        #[qinvokable]
        fn cancel_epgstation(self: Pin<&mut Player>);
        #[qinvokable]
        fn play_epgstation(self: Pin<&mut Player>, id: QString) -> bool;
        #[qinvokable]
        fn choose_epgstation(self: Pin<&mut Player>, id: QString) -> bool;
        #[qinvokable]
        fn play_epgstation_file(self: Pin<&mut Player>, id: QString, video: QString) -> bool;
        fn recording_files(self: &Player) -> *mut VideoFileModel;
        fn selected(self: &Player) -> i32;
        fn build_info(self: &Player) -> QString;
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
        fn open_screenshot_file_directory(self: Pin<&mut Player>, file: QUrl) -> bool;
        #[qinvokable]
        fn save_screenshot(self: Pin<&mut Player>, image: &QImage) -> bool;
        #[qinvokable]
        fn capture_screenshot(self: Pin<&mut Player>) -> bool;
        #[qinvokable]
        fn stage_screenshot(self: &Player, overlay: QString);
        fn screenshot_format(self: &Player) -> QString;
        fn screenshot_options(self: &Player) -> QString;
        fn screenshot_busy(self: &Player) -> bool;
        #[qinvokable]
        fn configure_screenshot_format(self: Pin<&mut Player>, format: QString) -> bool;
        #[qinvokable]
        fn configure_screenshot_options(
            self: Pin<&mut Player>,
            format: QString,
            value: i32,
            lossless: bool,
        ) -> bool;
        #[qinvokable]
        fn poll_screenshot(self: Pin<&mut Player>);
        #[qsignal]
        fn screenshot_finished(self: Pin<&mut Player>, file: QUrl);
        #[qsignal]
        fn desktop_raise_requested(self: Pin<&mut Player>);
        fn server_configured(self: &Player) -> bool;
        fn loading(self: &Player) -> bool;
        fn connecting(self: &Player) -> bool;
        fn playing(self: &Player) -> bool;
        fn viewing_channel(self: &Player) -> i32;
        fn playback_action(self: &Player) -> PlaybackAction;
        fn media_active(self: &Player) -> bool;
        fn paused(self: &Player) -> bool;
        fn seeking(self: &Player) -> bool;
        fn ended(self: &Player) -> bool;
        fn seekable(self: &Player) -> bool;
        fn position_ms(self: &Player) -> f64;
        fn duration_ms(self: &Player) -> f64;
        fn duration_estimated(self: &Player) -> bool;
        fn playback_rate(self: &Player) -> i32;
        fn requested_playback_rate(self: &Player) -> i32;
        fn minimum_playback_rate(self: &Player) -> i32;
        fn maximum_playback_rate(self: &Player) -> i32;
        fn speed_available(self: &Player) -> bool;
        fn speed_reason(self: &Player) -> QString;
        fn at_live_edge(self: &Player) -> bool;
        #[qinvokable]
        fn set_playback_rate(self: Pin<&mut Player>, tenths: i32) -> bool;
        fn timeshift(self: &Player) -> bool;
        fn window_start_ms(self: &Player) -> f64;
        fn window_end_ms(self: &Player) -> f64;
        fn live_delay_ms(self: &Player) -> f64;
        fn timeshift_storage(self: &Player) -> QString;
        fn timeshift_limits(self: &Player) -> QString;
        fn live_buffer_options(self: &Player) -> QString;
        #[qinvokable]
        fn configure_live_buffer(self: Pin<&mut Player>, milliseconds: i32) -> bool;
        fn transport_error(self: &Player) -> QString;
        #[qinvokable]
        fn return_to_live(self: Pin<&mut Player>) -> bool;
        #[qinvokable]
        fn configure_timeshift(self: Pin<&mut Player>, storage: QString) -> bool;
        #[qinvokable]
        fn configure_timeshift_options(
            self: Pin<&mut Player>,
            storage: QString,
            memory_mib: i32,
            filesystem_mib: i32,
            minutes: i32,
        ) -> bool;
        fn recording(self: &Player) -> bool;
        fn recording_name(self: &Player) -> QString;
        fn guide_visible(self: &Player) -> bool;
        #[qinvokable]
        fn request_language(self: Pin<&mut Player>, language: QString) -> bool;
        #[qinvokable]
        fn display_subtitles(self: Pin<&mut Player>, display: bool);
        fn subtitle_force_outline(self: &Player) -> bool;
        #[qinvokable]
        fn configure_subtitle_outline(self: Pin<&mut Player>, enabled: bool);
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
        fn toggle_playback(self: Pin<&mut Player>);
        #[qinvokable]
        fn pause(self: Pin<&mut Player>) -> bool;
        #[qinvokable]
        fn seek_to(self: Pin<&mut Player>, milliseconds: f64) -> bool;
        #[qinvokable]
        fn seek_timeline(self: Pin<&mut Player>, session: QString, milliseconds: f64) -> bool;
        #[qinvokable]
        fn timeline_preview(self: &Player, session: QString, milliseconds: f64) -> QString;
        #[qinvokable]
        fn skip(self: Pin<&mut Player>, milliseconds: f64) -> bool;
        #[qinvokable]
        fn open_recording(self: Pin<&mut Player>, file: QUrl) -> bool;
        #[qinvokable]
        fn open_recording_transfer(self: Pin<&mut Player>, key: QString) -> bool;
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
        fn commentary_position(&self) -> f64;
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
        unsafe fn window_options(self: &Player, item: *mut QQuickItem) -> QString;
        #[qinvokable]
        fn remember_window_size(self: Pin<&mut Player>, width: i32, height: i32) -> bool;
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
        fn clear_comment_cache(self: Pin<&mut Player>);
        #[qinvokable]
        fn refresh_recording_comments(self: Pin<&mut Player>);
        #[qinvokable]
        fn configure_comment_cache_limit(self: Pin<&mut Player>, mib: i32) -> bool;
        fn comment_cache_limit_mib(self: &Player) -> i32;
        #[qinvokable]
        fn configure_comment_presentation(
            self: Pin<&mut Player>,
            display: QString,
            placement: QString,
        ) -> bool;
        fn comment_display(self: &Player) -> QString;
        fn comment_placement(self: &Player) -> QString;
        fn evaluation_comment_list(self: &Player) -> bool;
        fn evaluation_wide_comments(self: &Player) -> bool;
        fn evaluation_collision_layout(self: &Player) -> bool;
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
    #[cfg(target_os = "linux")]
    desktop_media: desktop_media::Registration,
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
    channel_model: cxx::UniquePtr<crate::channel_model::ffi::ChannelModel>,
    recording_model: cxx::UniquePtr<crate::recording_model::ffi::RecordingModel>,
    video_file_model: cxx::UniquePtr<crate::video_file_model::ffi::VideoFileModel>,
    recording_library: crate::epgstation::Library,
    epgstation_input_error: QString,
    channel_program_data: QString,
    channel_visibility_data: QString,
    guide_visibility_data: QString,
    channel_program_now: f64,
    browser_projection: Option<crate::features::program_info::browser::Projection>,
    catalog: crate::channels::catalog::Catalog,
    stream_state: stream_state::State,
    timeline: playback::timeline::Snapshot,
    speed: playback::speed::Snapshot,
    timeshift_bytes_per_second: f64,
    live_timeline: QString,
    transport_message: transport::Message,
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
    video_aspect_ratio: f64,
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
    comment_replay: crate::features::comments::replay::Replay,
    comment_timeline: QString,
    comment_cache_bytes: f64,
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
    program_status: QString,
    current_projection: crate::features::program_info::presentation::Projection,
    next_current_program: Instant,
    diagnostics: QString,
    feature_metrics: Option<telemetry::FeatureMetrics>,
    volume_level: f64,
    audio_muted: bool,
    audio_output: playback::audio_output::Output,
    settings_error: QString,
    screenshot_error: QString,
    screenshot_saves: crate::screenshots::Queue,
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
    property_setter!(
        set_video_aspect_ratio,
        video_aspect_ratio,
        video_aspect_ratio_changed,
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
    pub fn channels(&self) -> *mut crate::channel_model::ffi::ChannelModel {
        self.rust().channel_model.as_ptr().cast_mut()
    }
    pub fn selected(&self) -> i32 {
        self.rust()
            .catalog
            .selected_index()
            .and_then(|i| i32::try_from(i).ok())
            .unwrap_or(-1)
    }
    fn set_selected(mut self: Pin<&mut Self>, value: i32) {
        let before_selected = self.selected();
        let before_action = self.playback_action();
        if let Ok(index) = usize::try_from(value) {
            self.as_mut().rust_mut().catalog.select(index);
        }
        if self.selected() != before_selected {
            self.as_mut().selected_changed();
        }
        if before_action != self.playback_action() {
            self.playback_action_changed();
        }
    }
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
    pub unsafe fn attach(mut self: Pin<&mut Self>, item: *mut crate::qt::ffi::QQuickItem) -> bool {
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
    pub unsafe fn observe_pointer(&self, item: *mut crate::qt::ffi::QQuickItem) {
        // The caller guarantees a live item; the native helper accepts null.
        unsafe { crate::qt::ffi::install_pointer_activity(item) };
    }
}

impl Drop for PlayerRust {
    fn drop(&mut self) {
        self.remote.stop();
        self.epg_events.configure(None);
        // The media owner enforces native shutdown before subtitle destruction,
        // including when QML construction failed before onClosing could run.
        self.screenshot_saves.finish();
        self.media.shutdown_before_drop();
        self.epg.configure(None);
    }
}
