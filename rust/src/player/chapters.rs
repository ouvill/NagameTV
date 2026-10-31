use super::ffi;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::pin::Pin;

impl ffi::Player {
    pub fn chapters_data(&self) -> QString {
        QString::from(self.rust().chapters.json())
    }
    pub fn current_chapter(&self) -> i32 {
        self.rust().chapters.current()
    }
    pub fn seek_chapter(self: Pin<&mut Self>, offset: i32) -> bool {
        if !self.recording() || !self.seekable() {
            return false;
        }
        let position = self.rust().timeline.position.map(|time| time.mseconds());
        let Some(target) = self.rust().chapters.target(offset, position) else {
            return false;
        };
        self.seek_to(target as f64)
    }
}
