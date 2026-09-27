//! Thin Qt projection of the independent still-image worker.
use super::ffi;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::pin::Pin;

impl ffi::Player {
    pub fn seek_preview_image(&self) -> QString {
        QString::from(self.rust().seek_preview.image())
    }
    pub fn request_seek_preview(mut self: Pin<&mut Self>, milliseconds: f64) {
        let before = self.rust().seek_preview.revision();
        let valid = self.seekable()
            && milliseconds.is_finite()
            && milliseconds >= self.window_start_ms()
            && milliseconds <= self.window_end_ms();
        if valid && let Some(source) = self.rust().media.preview_source() {
            // The right endpoint is exclusive in the retained TS index.
            let target = milliseconds.min((self.window_end_ms() - 1.0).max(self.window_start_ms()));
            self.as_mut()
                .rust_mut()
                .seek_preview
                .request(source, target);
        } else {
            self.as_mut().rust_mut().seek_preview.clear();
        }
        if before != self.rust().seek_preview.revision() {
            self.seek_preview_image_changed();
        }
    }
    pub fn clear_seek_preview(mut self: Pin<&mut Self>) {
        let before = self.rust().seek_preview.revision();
        self.as_mut().rust_mut().seek_preview.clear();
        if before != self.rust().seek_preview.revision() {
            self.seek_preview_image_changed();
        }
    }
    pub(super) fn poll_seek_preview(mut self: Pin<&mut Self>) {
        let before = self.rust().seek_preview.revision();
        let identity = self
            .media_active()
            .then(|| self.rust().media.preview_identity())
            .flatten();
        let mut this = self.as_mut().rust_mut();
        this.seek_preview.synchronize(identity);
        this.seek_preview.poll();
        if before != self.rust().seek_preview.revision() {
            self.seek_preview_image_changed();
        }
    }
}
