use super::ffi;
use crate::media_subtitles::Error;
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QImage, QString, QUrl};
use std::pin::Pin;

impl ffi::Player {
    pub fn subtitle_tracks(&self) -> *mut crate::subtitle_model::ffi::SubtitleModel {
        self.rust().subtitle_model.as_ptr().cast_mut()
    }
    pub fn media_subtitle_available(&self) -> bool {
        self.rust().media.media_subtitles().is_some()
    }
    pub fn subtitle_loading(&self) -> bool {
        self.rust()
            .media
            .media_subtitles()
            .is_some_and(|session| session.loading())
    }
    fn subtitle_result(mut self: Pin<&mut Self>, result: Result<(), Error>) -> bool {
        let success = result.is_ok();
        self.as_mut()
            .set_media_subtitle_error(result.err().map(subtitle_error).unwrap_or_default());
        success
    }
    pub fn open_subtitle(mut self: Pin<&mut Self>, file: QUrl) -> bool {
        let path = url::Url::parse(&file.to_string())
            .ok()
            .and_then(|url| url.to_file_path().ok());
        let result = match (path, self.as_mut().rust_mut().media.media_subtitles_mut()) {
            (Some(path), Some(session)) => session.load(path),
            _ => Err(Error::Unavailable),
        };
        self.as_mut().subtitle_loading_changed();
        self.subtitle_result(result)
    }
    pub fn select_subtitle(mut self: Pin<&mut Self>, id: QString) -> bool {
        let id = id.to_string();
        let result = if id == "external" {
            self.as_mut()
                .rust_mut()
                .media
                .media_subtitles_mut()
                .ok_or(Error::Unavailable)
                .and_then(|s| s.select_external())
        } else if let Some(id) = id.strip_prefix("embedded:") {
            self.rust()
                .media
                .playback()
                .ok_or(Error::Unavailable)
                .and_then(|p| p.select_subtitle(id))
                .and_then(|()| {
                    self.as_mut()
                        .rust_mut()
                        .media
                        .media_subtitles_mut()
                        .ok_or(Error::Unavailable)
                        .and_then(|s| s.select_embedded())
                })
        } else {
            Err(Error::Unavailable)
        };
        if result.is_ok() {
            let script = self
                .rust()
                .media
                .media_subtitles()
                .and_then(|s| s.external());
            self.as_mut()
                .rust_mut()
                .stream_state
                .retain_subtitle(script);
        }
        self.as_mut().publish_subtitle_tracks();
        self.subtitle_result(result)
    }
    pub(super) fn publish_subtitle_tracks(mut self: Pin<&mut Self>) {
        let tracks = match (
            self.rust().media.media_subtitles(),
            self.rust().media.playback(),
        ) {
            (Some(session), Some(playback)) => session.tracks(playback.subtitle_tracks()),
            _ => Vec::new(),
        };
        self.as_mut()
            .rust_mut()
            .subtitle_model
            .pin_mut()
            .replace(tracks.into());
    }
    pub(super) fn reset_media_subtitles(mut self: Pin<&mut Self>) {
        self.as_mut().set_media_subtitle_image(QImage::default());
        self.as_mut().set_media_subtitle_error(QString::default());
        self.as_mut().publish_subtitle_tracks();
        self.as_mut().media_subtitle_available_changed();
        self.subtitle_loading_changed();
    }
    pub(super) fn poll_media_subtitles(mut self: Pin<&mut Self>) {
        let load = self
            .as_mut()
            .rust_mut()
            .media
            .media_subtitles_mut()
            .and_then(|s| s.poll_load());
        if let Some(result) = load {
            if result.is_ok() {
                let script = self
                    .rust()
                    .media
                    .media_subtitles()
                    .and_then(|s| s.external());
                self.as_mut()
                    .rust_mut()
                    .stream_state
                    .retain_subtitle(script);
            }
            self.as_mut().subtitle_result(result);
            self.as_mut().subtitle_loading_changed();
        }
        self.as_mut().publish_subtitle_tracks();
        if self.seeking() || self.ended() {
            if let Some(session) = self.as_mut().rust_mut().media.media_subtitles_mut() {
                session.invalidate();
            }
            self.set_media_subtitle_image(QImage::default());
            return;
        }
        let output = self
            .rust()
            .media
            .playback()
            .and_then(|p| p.position().map(|position| (position, p.subtitle_canvas())));
        let frame = output.and_then(|(position, (width, height))| {
            self.as_mut()
                .rust_mut()
                .media
                .media_subtitles_mut()
                .map(|s| s.render(position, width, height))
        });
        match frame {
            Some(Ok(Some(frame))) => {
                // SAFETY: the renderer returns exactly width * height * 4 premultiplied RGBA bytes.
                let image = unsafe {
                    QImage::from_raw_bytes(
                        frame.pixels,
                        frame.width,
                        frame.height,
                        cxx_qt_lib::QImageFormat::Format_RGBA8888_Premultiplied,
                    )
                };
                self.set_media_subtitle_image(image);
            }
            Some(Err(error)) => {
                self.as_mut().set_media_subtitle_image(QImage::default());
                self.subtitle_result(Err(error));
            }
            Some(Ok(None)) | None => {}
        }
    }
}

fn subtitle_error(error: Error) -> QString {
    use super::status::{tr, with_detail};
    match error {
        Error::Read(error) => with_detail("Could not read subtitles: %1", error),
        Error::Format => tr("Select a valid SRT or ASS subtitle file"),
        Error::Encoding => tr("Save the subtitle file as UTF-8"),
        Error::Capacity => tr("Subtitle data exceeds the supported limit"),
        Error::Renderer(error) => with_detail("Could not render subtitles: %1", error),
        Error::Sink(error) => with_detail("Could not receive subtitles: %1", error),
        Error::Unavailable => tr("Subtitle track is no longer available"),
        Error::Rejected => tr("Subtitle selection was rejected"),
        Error::Worker => tr("Subtitle worker stopped unexpectedly"),
    }
}
