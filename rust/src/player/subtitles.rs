use super::ffi;
use crate::playback::Playback;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::pin::Pin;

impl ffi::Player {
    pub fn subtitle_glyph_outline(&self, text: QString, font: ffi::QFont) -> QString {
        ffi::subtitle_outline_path(&text, &font)
    }

    pub fn poll_subtitles(mut self: Pin<&mut Self>) {
        use crate::subtitles::SubtitleUpdate;
        let update = self
            .as_ref()
            .rust()
            .playback
            .as_ref()
            .map(Playback::poll_subtitles)
            .unwrap_or(SubtitleUpdate::Unchanged);
        let changed = !matches!(update, SubtitleUpdate::Unchanged);
        match update {
            SubtitleUpdate::Unchanged => {}
            SubtitleUpdate::Clear => self.as_mut().rust_mut().subtitle_cue = None,
            SubtitleUpdate::Show(cue) => self.as_mut().rust_mut().subtitle_cue = Some(cue),
        }
        let visible = *self.as_ref().subtitles_enabled() && *self.as_ref().playing();
        if !visible {
            if self.as_ref().rust().subtitle_presented {
                self.as_mut().set_subtitle_text(QString::default());
                self.as_mut().set_subtitle_data(QString::default());
            }
            self.as_mut().rust_mut().subtitle_presented = false;
            return;
        }
        if changed || !self.as_ref().rust().subtitle_presented {
            let encoded = self
                .as_ref()
                .rust()
                .subtitle_cue
                .as_ref()
                .map(|cue| {
                    serde_json::to_string(cue)
                        .map(|data| (QString::from(&cue.text), QString::from(data)))
                })
                .transpose();
            let (text, data) = match encoded {
                Ok(value) => value.unwrap_or_default(),
                Err(error) => {
                    tracing::warn!(%error, "Could not serialize subtitle cue");
                    // Clear this failed update; retry only when the cue changes.
                    (QString::default(), QString::default())
                }
            };
            self.as_mut().set_subtitle_text(text);
            self.as_mut().set_subtitle_data(data);
            self.as_mut().rust_mut().subtitle_presented = true;
        }
    }
}
