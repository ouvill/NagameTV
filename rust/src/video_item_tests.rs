//! Main-thread integration checks using cxx-qt and gstreamer-rs.
//! QML only creates the real native items; all assertions live here.
use cxx_qt_lib::{QByteArray, QGuiApplication, QQmlApplicationEngine, QUrl};
use gstreamer::{self as gst, prelude::*};
use std::{
    cell::Cell,
    ffi::c_void,
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
    sync::atomic::{AtomicPtr, Ordering},
};

#[cxx_qt::bridge]
mod ffi {
    unsafe extern "C++" {
        include!(<QtQuick/QQuickItem>);
        type QQuickItem = crate::player::ffi::QQuickItem;
    }
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        type VideoItemProbe = super::Probe;

        #[qinvokable]
        unsafe fn check(self: &VideoItemProbe, ordinary: *mut QQuickItem, video: *mut QQuickItem);
    }
}

#[derive(Default)]
pub struct Probe;

thread_local! {
    static OUTCOME: Cell<Option<bool>> = const { Cell::new(None) };
}

impl ffi::VideoItemProbe {
    /// # Safety
    /// The synchronous QML fixture supplies live GUI-thread items and does not
    /// destroy them or process events until this method returns.
    pub unsafe fn check(&self, ordinary: *mut ffi::QQuickItem, video: *mut ffi::QQuickItem) {
        // Never unwind through the generated Qt/CXX invocation boundary.
        let passed = catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: Forward the fixture's synchronous lifetime guarantee.
            unsafe { check_items(ordinary, video) };
        }))
        .is_ok();
        OUTCOME.with(|outcome| outcome.set(Some(passed)));
    }
}

// The test never leaves NULL state. Detach on both success and assertion failure,
// before the QML engine destroys its items.
struct AttachedSink(gst::Element);
impl Drop for AttachedSink {
    fn drop(&mut self) {
        self.0.set_property("widget", ptr::null_mut::<c_void>());
    }
}

unsafe fn check_items(ordinary: *mut ffi::QQuickItem, video: *mut ffi::QQuickItem) {
    use crate::player::ffi::qml6_video_item_pointer;
    assert!(!ordinary.is_null());
    assert!(!video.is_null());
    // SAFETY: Both items belong to the synchronous fixture; null is accepted.
    unsafe {
        assert!(qml6_video_item_pointer(ptr::null_mut()).is_null());
        assert!(qml6_video_item_pointer(ordinary).is_null());
    }
    // SAFETY: The QML engine owns the live video item throughout this call.
    let pointer = unsafe { qml6_video_item_pointer(video) };
    assert!(!pointer.is_null(), "Rejecting a genuine QML video subclass");

    let sink = AttachedSink(gst::ElementFactory::make("qml6glsink").build().unwrap());
    sink.0.set_property("widget", pointer.cast::<c_void>());
    assert_eq!(sink.0.property::<*mut c_void>("widget"), video.cast());

    // AtomicPtr transports the address without converting it to an integer or
    // declaring QQuickItem Send. The worker only calls the rejection helper.
    let address = AtomicPtr::new(video);
    let rejected = std::thread::scope(|scope| {
        scope
            .spawn(|| {
                // SAFETY: The GUI thread waits for this worker and processes no
                // events, so the item remains live. The helper rejects the
                // non-GUI thread before accessing the item (see video_item.h).
                unsafe { qml6_video_item_pointer(address.load(Ordering::Relaxed)).is_null() }
            })
            .join()
            .unwrap()
    });
    assert!(rejected, "Accepting a video item from a worker thread");
    sink.0.set_property("widget", ptr::null_mut::<c_void>());
    assert!(sink.0.property::<*mut c_void>("widget").is_null());
    drop(sink);
    // SAFETY: Same fixture lifetime as the helper checks above. This also
    // covers the application's actual Playback attachment/teardown methods.
    unsafe { crate::playback::video_item_checks::check(ordinary, video) };
}

pub fn run() -> i32 {
    OUTCOME.with(|outcome| outcome.set(None));
    let app = QGuiApplication::new();
    assert!(!app.is_null(), "Could not create the Qt application");
    gst::init().unwrap();
    // Loading the real plugin registers the native QML video type.
    let _registration = gst::ElementFactory::make("qml6glsink").build().unwrap();
    cxx_qt::init_qml_module!("MinimalViewer");
    let mut engine = QQmlApplicationEngine::new();
    engine.pin_mut().load_data(
        &QByteArray::from(include_str!("../../tests/video_item.qml")),
        &QUrl::from("file:///video_item.qml"),
    );
    // A failed QML load or a callback that never ran must fail the test.
    let passed = OUTCOME.with(|outcome| outcome.get()) == Some(true);
    drop(engine);
    if passed {
        println!("Video item integration checks passed");
        0
    } else {
        eprintln!("Video item integration checks failed or QML fixture did not run");
        1
    }
}
