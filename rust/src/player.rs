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
        include!("pointer_activity.h");
        #[cxx_name = "installPointerActivity"]
        unsafe fn install_pointer_activity(item: *mut QQuickItem);
        type QQuickItem;
        #[cxx_name = "configureQtQuickOpenGl"]
        fn configure_qt_quick_open_gl();
        #[cxx_name = "installQtGcLogging"]
        fn install_qt_gc_logging(callback: fn(category: &str, message: &str));
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
        #[cxx_name = "attachPointerActivity"]
        unsafe fn attach_pointer_activity(self: Pin<&mut Player>, item: *mut QQuickItem);
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
        #[cxx_name = "recordUiState"]
        fn record_ui_state(self: Pin<&mut Player>, guide: bool, channels: bool, live_comments: i32);
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
}

mod catalog;
mod catalog_request;
mod catalog_view;
mod comments;
mod fetch;
mod playback_control;
mod pointer_activity;
mod preferences;
mod selection;
mod state;
mod subtitles;
mod telemetry;

pub use state::PlayerRust;
