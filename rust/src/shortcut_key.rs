//! Qt key matching and mouse Back binding for QML shortcuts.
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
        unsafe fn bind_back_button(self: &ShortcutKey, item: *mut QQuickItem);
    }
}

#[derive(Default)]
pub struct Matcher;

impl ffi::ShortcutKey {
    /// # Safety
    /// QML must supply a live item on the GUI thread; it owns the native filter.
    pub unsafe fn bind_back_button(&self, item: *mut crate::qt::ffi::QQuickItem) {
        // SAFETY: The caller supplies the item lifetime and thread guarantees.
        unsafe { crate::qt::ffi::install_back_button(item) };
    }

    pub fn matches(&self, sequence: &cxx_qt_lib::QString, key: i32, modifiers: i32) -> bool {
        ffi::matches_shortcut_key(sequence, key, modifiers)
    }
}
