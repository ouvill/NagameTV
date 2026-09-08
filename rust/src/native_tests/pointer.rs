use super::bridge::ffi;
use cxx_qt::casting::Upcast;
use cxx_qt_lib::{QGuiApplication, QPointF, QSizeF};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

pub fn run() -> i32 {
    let app = QGuiApplication::new();
    assert!(!app.is_null());
    let mut first = ffi::new_window();
    let mut second = ffi::new_window();
    let mut item = ffi::new_activity_item();
    // SAFETY: Both windows outlive the item. Only its visual parent changes;
    // Rust's UniquePtr retains ownership and destroys it before either window.
    unsafe { item.pin_mut().set_parent_item(first.content_item()) };
    item.pin_mut().set_size(&QSizeF::new(100.0, 100.0));
    let base: &ffi::QQuickItem = item.as_ref().unwrap().upcast();
    // SAFETY: The item is live on the GUI thread, with no event-loop dispatch.
    unsafe {
        ffi::install_test_pointer_activity(std::ptr::from_ref(base).cast_mut());
        ffi::install_test_pointer_activity(std::ptr::from_ref(base).cast_mut());
    }
    assert_eq!(
        ffi::observer_count(base),
        1,
        "Repeated installation must not duplicate observers"
    );
    let lifetime = ffi::watch_observer(base);
    assert!(
        !lifetime.was_destroyed(),
        "Observer must exist before item destruction"
    );
    let count = Arc::new(AtomicUsize::new(0));
    let received = count.clone();
    let _connection = item.pin_mut().on_activity(move |_| {
        received.fetch_add(1, Ordering::SeqCst);
    });
    ffi::send_mouse_move(first.pin_mut(), &QPointF::new(20.0, 20.0));
    assert_eq!(count.load(Ordering::SeqCst), 1);
    ffi::send_mouse_move(first.pin_mut(), &QPointF::new(20.0, 20.0));
    assert_eq!(
        count.load(Ordering::SeqCst),
        1,
        "Identical positions do not emit twice"
    );
    item.pin_mut().set_enabled(false);
    ffi::send_mouse_move(first.pin_mut(), &QPointF::new(25.0, 25.0));
    assert_eq!(
        count.load(Ordering::SeqCst),
        1,
        "Disabled items do not emit activity"
    );
    item.pin_mut().set_enabled(true);
    // SAFETY: Same live-window and sole-ownership guarantees as above.
    unsafe { item.pin_mut().set_parent_item(second.content_item()) };
    ffi::send_mouse_move(first.pin_mut(), &QPointF::new(30.0, 30.0));
    assert_eq!(
        count.load(Ordering::SeqCst),
        1,
        "The old window must be detached"
    );
    ffi::send_mouse_move(second.pin_mut(), &QPointF::new(30.0, 30.0));
    assert_eq!(
        count.load(Ordering::SeqCst),
        2,
        "The new window must be observed"
    );
    drop(item);
    assert!(
        lifetime.was_destroyed(),
        "Item destruction must delete the observer"
    );
    ffi::send_mouse_move(second.pin_mut(), &QPointF::new(40.0, 40.0));
    assert_eq!(
        count.load(Ordering::SeqCst),
        2,
        "No callback after destruction"
    );
    println!("Pointer activity lifecycle checks passed");
    0
}
