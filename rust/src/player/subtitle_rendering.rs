//! Qt font geometry stays at the presentation boundary, outside the TS decoder.
use super::{ffi, subtitle_status};
use crate::{features::subtitles, playback};
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QImage, QString};
use std::pin::Pin;

impl ffi::Player {
    pub fn subtitle_drcs_image(&self, index: i32, force_outline: bool) -> QImage {
        // Negative indices and absent images are normal during delegate teardown.
        usize::try_from(index)
            .ok()
            .and_then(|index| self.rust().subtitle_images.get(index, force_outline))
            .unwrap_or_default()
    }
    pub fn subtitle_glyph_outline(&self, text: QString, font: ffi::QFont) -> QString {
        // The native helper returns a value and retains no text, font or path.
        crate::qt::ffi::subtitle_outline_path(&text, &font)
    }
}

impl ffi::Player {
    pub fn subtitle_force_outline(&self) -> bool {
        self.rust().preferences.preferences().subtitle_force_outline
    }

    pub fn configure_subtitle_outline(mut self: Pin<&mut Self>, enabled: bool) {
        if !self.rust().subtitles_enabled {
            return;
        }
        if self.subtitle_force_outline() != enabled {
            self.as_mut()
                .rust_mut()
                .preferences
                .change(crate::settings::Change::SubtitleForceOutline(enabled));
            self.as_mut().subtitle_force_outline_changed();
        }
        self.save_settings();
    }

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
        if self.media_subtitle_available() {
            self.poll_media_subtitles();
            return;
        }
        if self.seeking() || self.ended() {
            return;
        }
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
                let images = match crate::qt::drcs::Images::prepare(&cue) {
                    Ok(images) => images,
                    Err(error) => {
                        self.subtitle_presentation_failed(subtitle_status::Status::ImageFailed(
                            error,
                        ));
                        return;
                    }
                };
                let revision = self.rust().subtitle_revision.wrapping_add(1);
                #[derive(serde::Serialize)]
                struct Presentation<'a> {
                    #[serde(flatten)]
                    cue: &'a subtitles::SubtitleCue,
                    revision: String,
                }
                match serde_json::to_string(&Presentation {
                    cue: &cue,
                    revision: revision.to_string(),
                }) {
                    Ok(data) => {
                        {
                            let mut this = self.as_mut().rust_mut();
                            this.subtitle_cells = cue.cells.len();
                            this.subtitle_images = images;
                            this.subtitle_revision = revision;
                        }
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
