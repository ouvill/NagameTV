//! Qt key matching and keyboard/mouse Back binding for QML shortcuts.
#[cxx_qt::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("shortcut_key.h");
        #[rust_name = "matches_shortcut_key"]
        fn matchesShortcutKey(sequence: &QString, key: i32, modifiers: i32) -> bool;
        include!("QtQuick/QQuickItem");
        type QQuickItem = crate::qt::ffi::QQuickItem;
    }
    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        type ShortcutKey = super::Matcher;

        #[qinvokable]
        fn matches(self: &ShortcutKey, sequence: &QString, key: i32, modifiers: i32) -> bool;

        #[qinvokable]
        unsafe fn post_key(self: &ShortcutKey, item: *mut QQuickItem, key: i32) -> bool;

        #[qinvokable]
        unsafe fn bind_back_button(self: &ShortcutKey, item: *mut QQuickItem, popup_open: bool);
    }
}

#[derive(Default)]
pub struct Matcher;

impl ffi::ShortcutKey {
    /// # Safety
    /// QML must supply a live item on the GUI thread; it owns the native filter.
    pub unsafe fn bind_back_button(&self, item: *mut crate::qt::ffi::QQuickItem, popup_open: bool) {
        // SAFETY: The caller supplies the item lifetime and thread guarantees.
        unsafe { crate::qt::ffi::install_back_button(item, popup_open) };
    }

    /// # Safety
    /// QML supplies a live item on the GUI thread, or null.
    pub unsafe fn post_key(&self, item: *mut crate::qt::ffi::QQuickItem, key: i32) -> bool {
        if !crate::qt::ffi::supports_navigation_key(key) {
            tracing::warn!(key, "Remote key posting rejected: unsupported Qt key");
            return false;
        }
        // SAFETY: The caller provides the GUI thread and live-item guarantees;
        // Qt takes ownership of the posted events, not the item pointer.
        if !unsafe { crate::qt::ffi::post_navigation_key(item, key) } {
            tracing::warn!(key, "Remote key posting failed: item has no window");
            return false;
        }
        true
    }

    pub fn matches(&self, sequence: &cxx_qt_lib::QString, key: i32, modifiers: i32) -> bool {
        ffi::matches_shortcut_key(sequence, key, modifiers)
    }
}
