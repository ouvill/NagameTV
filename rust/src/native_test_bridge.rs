use cxx_qt::CxxQtType;
use cxx_qt_lib::{QFont, QString};
use std::pin::Pin;

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qbytearray.h");
        type QByteArray = cxx_qt_lib::QByteArray;
        include!("cxx-qt-lib/qfont.h");
        type QFont = cxx_qt_lib::QFont;
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
        include!("cxx-qt-lib/qqmlapplicationengine.h");
        type QQmlApplicationEngine = cxx_qt_lib::QQmlApplicationEngine;
        include!("cxx-qt-lib/qpainterpath.h");
        type QPainterPath = cxx_qt_lib::QPainterPath;
        include!("cxx-qt-lib/qimage.h");
        type QImage = cxx_qt_lib::QImage;
        include!("cxx-qt-lib/qpoint.h");
        type QPoint = cxx_qt_lib::QPoint;
        include!("cxx-qt-lib/qpointf.h");
        type QPointF = cxx_qt_lib::QPointF;
        include!("cxx-qt-lib/qsizef.h");
        type QSizeF = cxx_qt_lib::QSizeF;

        include!("subtitle_outline.h");
        #[rust_name = "outline_data"]
        fn subtitleOutlinePathData(path: &QPainterPath) -> QString;
        include!("localization.h");
        #[rust_name = "resolve_language"]
        fn resolveUiLanguage(preference: &QString, system: &QString) -> QString;

        include!("native_tests/qt_test_api.h");
        fn grabRoot(engine: Pin<&mut QQmlApplicationEngine>) -> Result<QImage>;
        fn clickRootKey(engine: Pin<&mut QQmlApplicationEngine>, sequence: &QString) -> Result<()>;
        type FrameTimes;
        fn watchFrames(engine: &QQmlApplicationEngine) -> Result<UniquePtr<FrameTimes>>;
        fn samples(self: &FrameTimes) -> QString;
        #[rust_name = "disable_catalog"]
        fn disableTranslationCatalog();
        #[rust_name = "enable_catalog"]
        fn enableTranslationCatalog();
        #[rust_name = "catalog_exists"]
        fn translationCatalogExists() -> bool;
        #[rust_name = "translator_count"]
        fn translatorCount() -> i32;
        #[rust_name = "set_default_locale"]
        fn setDefaultLocale(name: &QString);
        #[rust_name = "root_count"]
        fn rootCount(engine: &QQmlApplicationEngine) -> i32;
        #[rust_name = "root_property"]
        fn rootProperty(engine: &QQmlApplicationEngine, name: &QString) -> QVariant;
        #[rust_name = "set_root_property"]
        fn setRootProperty(
            engine: Pin<&mut QQmlApplicationEngine>,
            name: &QString,
            value: &QVariant,
        ) -> bool;
        #[rust_name = "evaluate_root"]
        fn evaluateRoot(
            engine: Pin<&mut QQmlApplicationEngine>,
            source: &QString,
        ) -> Result<QVariant>;
        #[rust_name = "drop_file_on_root"]
        fn dropFileOnRoot(
            engine: Pin<&mut QQmlApplicationEngine>,
            url: &QString,
            position: &QPoint,
        ) -> bool;
        #[rust_name = "raster_path"]
        fn rasterPath(path: &QPainterPath, image: &mut QImage, offset: &QPoint);
        #[rust_name = "raster_svg"]
        fn rasterSvg(renderer: Pin<&mut QSvgRenderer>, image: &mut QImage);
        type QSvgRenderer;
        #[rust_name = "is_valid"]
        fn isValid(self: &QSvgRenderer) -> bool;
        type QQuickWindow;
        #[rust_name = "content_item"]
        fn contentItem(self: &QQuickWindow) -> *mut QQuickItem;
        type ObserverLifetime;
        #[rust_name = "was_destroyed"]
        fn isNull(self: &ObserverLifetime) -> bool;
        #[rust_name = "observer_count"]
        fn pointerObserverCount(item: &QQuickItem) -> i32;
        #[rust_name = "watch_observer"]
        fn watchPointerObserver(item: &QQuickItem) -> UniquePtr<ObserverLifetime>;
        #[rust_name = "send_mouse_move"]
        fn sendMouseMove(window: Pin<&mut QQuickWindow>, position: &QPointF);
        #[rust_name = "install_test_pointer_activity"]
        unsafe fn installPointerActivity(item: *mut QQuickItem);
    }
    unsafe extern "C++Qt" {
        include!(<QtQuick/QQuickItem>);
        #[qobject]
        type QQuickItem;
    }
    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(i32, calls, READ, NOTIFY)]
        #[qproperty(bool, benchmark_stroke, READ, CONSTANT)]
        type TestOutlineProvider = super::OutlineProvider;
        #[qinvokable]
        fn subtitle_glyph_outline(
            self: Pin<&mut TestOutlineProvider>,
            text: &QString,
            font: &QFont,
        ) -> QString;

        #[qobject]
        #[base = QQuickItem]
        type NativeActivityItem = super::ActivityItem;
        // Invoked by the production C++ event filter through Qt meta-calls.
        #[qsignal]
        fn activity(self: Pin<&mut NativeActivityItem>);
        #[inherit]
        #[rust_name = "set_size"]
        fn setSize(self: Pin<&mut NativeActivityItem>, size: &QSizeF);
        #[inherit]
        #[rust_name = "set_enabled"]
        fn setEnabled(self: Pin<&mut NativeActivityItem>, enabled: bool);
        #[inherit]
        #[rust_name = "set_parent_item"]
        unsafe fn setParentItem(self: Pin<&mut NativeActivityItem>, parent: *mut QQuickItem);
    }
    impl cxx_qt::Initialize for NativeActivityItem {}

    unsafe extern "C++" {
        include!("cxx-qt-lib/common.h");
        #[namespace = "rust::cxxqtlib1"]
        #[cxx_name = "make_unique"]
        fn new_activity_item() -> UniquePtr<NativeActivityItem>;
        #[namespace = "rust::cxxqtlib1"]
        #[cxx_name = "make_unique"]
        fn new_window() -> UniquePtr<QQuickWindow>;
        #[namespace = "rust::cxxqtlib1"]
        #[cxx_name = "make_unique"]
        fn new_svg(data: &QByteArray) -> UniquePtr<QSvgRenderer>;
    }
}

#[derive(Default)]
pub struct ActivityItem;

pub struct OutlineProvider {
    calls: i32,
    benchmark_stroke: bool,
}
impl Default for OutlineProvider {
    fn default() -> Self {
        Self {
            calls: 0,
            benchmark_stroke: std::env::var("VIEWER_SUBTITLE_BENCHMARK_STROKE").as_deref()
                != Ok("0"),
        }
    }
}
impl ffi::TestOutlineProvider {
    pub fn subtitle_glyph_outline(
        mut self: Pin<&mut Self>,
        text: &QString,
        font: &QFont,
    ) -> QString {
        let calls = self.rust().calls + 1;
        self.as_mut().rust_mut().calls = calls;
        self.as_mut().calls_changed();
        crate::player::ffi::subtitle_outline_path(text, font)
    }
}

impl cxx_qt::Initialize for ffi::NativeActivityItem {
    fn initialize(self: Pin<&mut Self>) {}
}
