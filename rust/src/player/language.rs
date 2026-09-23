use super::ffi;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::pin::Pin;
impl ffi::Player {
    pub fn request_language(mut self: Pin<&mut Self>, language: QString) -> bool {
        let Some(preference) = crate::settings::Language::parse(&language.to_string()) else {
            return false;
        };
        let effective = crate::qt::ffi::apply_ui_language(&QString::from(preference.code()));
        if effective.is_empty() {
            return false;
        }
        self.as_mut()
            .rust_mut()
            .preferences
            .change(crate::settings::Change::Language(preference));
        self.as_mut().set_language(QString::from(preference.code()));
        self.as_mut().set_ui_language(effective);
        // Reproject existing state only: translating must not poll or restart workers.
        self.as_mut().refresh_status();
        self.as_mut().refresh_comment_status();
        self.as_mut().refresh_comment_posting();
        self.as_mut().refresh_epg_status();
        self.as_mut().refresh_subtitle_status();
        self.as_mut().transport_error_changed();
        self.as_mut().speed_reason_changed();
        self.as_mut().epgstation_changed();
        self.as_mut().refresh_metric_text();
        self.save_settings();
        true
    }
}
