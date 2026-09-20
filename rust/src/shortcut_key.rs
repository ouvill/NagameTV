//! Qt sequence matching for keys forwarded directly to a focused Quick item.
#[cxx_qt::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("shortcut_key.h");
        #[rust_name = "matches_shortcut_key"]
        fn matchesShortcutKey(sequence: &QString, key: i32, modifiers: i32) -> bool;
    }
    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        type ShortcutKey = super::Matcher;

        #[qinvokable]
        fn matches(self: &ShortcutKey, sequence: &QString, key: i32, modifiers: i32) -> bool;
    }
}

#[derive(Default)]
pub struct Matcher;

impl ffi::ShortcutKey {
    pub fn matches(&self, sequence: &cxx_qt_lib::QString, key: i32, modifiers: i32) -> bool {
        ffi::matches_shortcut_key(sequence, key, modifiers)
    }
}
