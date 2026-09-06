use super::ffi;
use std::pin::Pin;

impl ffi::Player {
    /// # Safety
    /// `item` must be a live GUI-thread QQuickItem declaring activity().
    /// The native observer is owned by that item and destroyed with it.
    pub unsafe fn attach_pointer_activity(self: Pin<&mut Self>, item: *mut ffi::QQuickItem) {
        // SAFETY: QML supplies its live activity item on the GUI thread.
        unsafe { ffi::install_pointer_activity(item) };
    }
}
