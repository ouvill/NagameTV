//! Native Qt services shared by startup, playback and presentation adapters.
//! Domain modules must not depend on Player to access these services.
pub mod application;
pub mod variant;

/// GUI-thread screen notifications. The item owns the native connections;
/// consumers can retain weak handles instead of extending playback lifetime.
pub struct RefreshObserver(Box<dyn Fn(f64)>);
impl RefreshObserver {
    pub fn new(changed: impl Fn(f64) + 'static) -> Box<Self> {
        Box::new(Self(Box::new(changed)))
    }
    fn changed(&self, refresh_rate_hz: f64) {
        (self.0)(refresh_rate_hz);
    }
}

#[cxx::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qurl.h");
        type QUrl = cxx_qt_lib::QUrl;
        include!("cxx-qt-lib/qimage.h");
        type QImage = cxx_qt_lib::QImage;
        include!("cxx-qt-lib/qfont.h");
        type QFont = cxx_qt_lib::QFont;
        include!("cxx-qt-lib/qsize.h");
        type QSize = cxx_qt_lib::QSize;
        include!("subtitle_outline.h");
        #[cxx_name = "subtitleOutlinePath"]
        fn subtitle_outline_path(text: &QString, font: &QFont) -> QString;
        include!("qt_helpers.h");
        #[cxx_name = "availableWindowSize"]
        unsafe fn available_window_size(item: *mut QQuickItem) -> Result<QSize>;
        #[cxx_name = "picturesDirectory"]
        fn pictures_directory() -> QString;
        #[cxx_name = "shareScreenshotImage"]
        fn share_screenshot_image(image: &QImage) -> QImage;
        #[cxx_name = "saveScreenshotImage"]
        fn save_screenshot_image(
            image: &QImage,
            path: &QString,
            format: &QString,
            quality: i32,
            compression: i32,
        ) -> bool;
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
        #[cfg(target_os = "linux")]
        #[cxx_name = "useQtQuickDialogs"]
        fn use_qt_quick_dialogs();
        include!("desktop_media.h");
        #[cfg(target_os = "linux")]
        type DesktopMedia;
        #[cfg(target_os = "linux")]
        #[cxx_name = "connectDesktopMedia"]
        fn connect_desktop_media() -> Result<UniquePtr<DesktopMedia>>;
        #[cfg(target_os = "linux")]
        #[cxx_name = "takeCommand"]
        fn take_command(self: Pin<&mut DesktopMedia>) -> QString;
        #[cfg(target_os = "linux")]
        fn publish(self: Pin<&mut DesktopMedia>, json: &QString);
        include!("portal.h");
        #[cfg(target_os = "linux")]
        #[cxx_name = "portalThemeLoaded"]
        fn portal_theme_loaded() -> bool;
        #[cfg(target_os = "linux")]
        #[cxx_name = "portalFileChooserVersion"]
        fn portal_file_chooser_version() -> Result<u32>;
        #[cfg(target_os = "linux")]
        #[cxx_name = "portalRetrieveRecording"]
        fn portal_retrieve_recording(key: &QString) -> Result<QString>;
        include!("video_item.h");
        #[cxx_name = "qml6VideoItemPointer"]
        unsafe fn qml6_video_item_pointer(item: *mut QQuickItem) -> *mut u8;
        include!("screen_refresh.h");
        #[cxx_name = "observeScreenRefresh"]
        unsafe fn observe_screen_refresh(item: *mut QQuickItem, observer: Box<RefreshObserver>);
    }
    extern "Rust" {
        type RefreshObserver;
        fn changed(self: &RefreshObserver, refresh_rate_hz: f64);
    }
}
