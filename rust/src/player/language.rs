use super::ffi;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::pin::Pin;
impl ffi::Player {
    pub fn request_language(mut self: Pin<&mut Self>, language: QString) -> bool {
        let Some(preference) = crate::settings::Language::parse(&language.to_string()) else {
            return false;
        };
        let effective = ffi::apply_ui_language(&QString::from(preference.code()));
        if effective.is_empty() {
            return false;
        }
        self.as_mut()
            .rust_mut()
            .preferences
            .preferences_mut()
            .language = preference;
        self.as_mut().set_language(QString::from(preference.code()));
        self.as_mut().set_ui_language(effective);
        self.save_settings();
        true
    }
}
