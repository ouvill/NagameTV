//! Qt font geometry stays at the presentation boundary, outside the TS decoder.
use super::{ffi, subtitle_status};
use crate::{features::subtitles, playback};
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::pin::Pin;

impl ffi::Player {
    pub fn subtitle_glyph_outline(&self, text: QString, font: ffi::QFont) -> QString {
        // The native helper returns a value and retains no text, font or path.
        ffi::subtitle_outline_path(&text, &font)
    }
}

impl ffi::Player {
    pub fn display_subtitles(mut self: Pin<&mut Self>, display: bool) {
        if !self.rust().subtitles_enabled {
            return;
        }
        self.as_mut().set_subtitle_display(display);
        {
            let mut this = self.as_mut().rust_mut();
            this.subtitle_cells = 0;
            this.preferences
                .change(crate::settings::Change::SubtitleDisplay(display));
        }
        self.as_mut().set_subtitle_data(QString::default());
        self.save_settings();
    }
    pub fn poll_subtitles(mut self: Pin<&mut Self>) {
        let update = self.rust().media.subtitles().map(|session| {
            session.poll(
                self.rust()
                    .media
                    .playback()
                    .and_then(playback::Playback::position),
            )
        });
        match update {
            Some(Ok(subtitles::SubtitleUpdate::Show(cue))) if self.rust().subtitle_display => {
                match serde_json::to_string(&cue) {
                    Ok(data) => {
                        self.as_mut().rust_mut().subtitle_cells = cue.cells.len();
                        self.set_subtitle_data(QString::from(data));
                    }
                    Err(error) => self.subtitle_presentation_failed(
                        subtitle_status::Status::PresentationFailed(error),
                    ),
                }
            }
            Some(Ok(subtitles::SubtitleUpdate::Clear)) => {
                self.as_mut().rust_mut().subtitle_cells = 0;
                self.set_subtitle_data(QString::default())
            }
            Some(Err(error)) => {
                self.subtitle_presentation_failed(subtitle_status::Status::Failed(error));
            }
            _ => {}
        }
    }
    fn subtitle_presentation_failed(mut self: Pin<&mut Self>, status: subtitle_status::Status) {
        self.as_mut().set_subtitles_active(false);
        self.as_mut().rust_mut().subtitle_cells = 0;
        self.as_mut().set_subtitle_data(QString::default());
        self.update_subtitle_status(status);
    }
}
