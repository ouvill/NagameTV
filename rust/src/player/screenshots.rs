use super::{ffi, status::tr};
use crate::{
    screenshots::{Captured, Error, Prepared},
    settings::{Change, ScreenshotDirectory, ScreenshotFormat, WebpMode},
};
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QImage, QString, QUrl};
use std::{path::PathBuf, pin::Pin};

impl ffi::Player {
    fn resolved_screenshot_directory(
        &self,
        directory: &ScreenshotDirectory,
    ) -> Result<PathBuf, Error> {
        directory
            .resolve(&PathBuf::from(
                crate::qt::ffi::pictures_directory().to_string(),
            ))
            .ok_or(Error::Directory)
    }

    pub fn screenshot_directory(&self) -> QString {
        self.resolved_screenshot_directory(
            &self.rust().preferences.preferences().screenshot_directory,
        )
        .map(|path| QString::from(path.to_string_lossy().as_ref()))
        .unwrap_or_default()
    }

    pub fn screenshot_directory_url(&self) -> QUrl {
        QUrl::from_local_file(&self.screenshot_directory())
    }

    fn screenshot_failure(self: Pin<&mut Self>, error: impl std::fmt::Display) {
        tracing::error!("Screenshot operation failed: {error}");
        self.set_screenshot_error(tr(
            "Could not access the screenshot folder. Choose a writable folder in Settings.",
        ));
    }

    fn commit_screenshot_directory(
        mut self: Pin<&mut Self>,
        directory: ScreenshotDirectory,
    ) -> bool {
        let result = self
            .resolved_screenshot_directory(&directory)
            .and_then(Prepared::new);
        if let Err(error) = result {
            self.screenshot_failure(error);
            return false;
        }
        drop(result);
        // The writable probe is dropped here; the next capture validates again.
        if self.rust().preferences.preferences().screenshot_directory != directory {
            self.as_mut()
                .rust_mut()
                .preferences
                .change(Change::ScreenshotDirectory(directory));
            self.as_mut().screenshot_directory_changed();
        }
        self.as_mut().set_screenshot_error(QString::default());
        self.save_settings();
        true
    }

    pub fn configure_screenshot_directory(self: Pin<&mut Self>, directory: QUrl) -> bool {
        let choice = directory
            .to_local_file()
            .filter(|path| !path.is_empty())
            .and_then(|path| ScreenshotDirectory::try_from(path.to_string()).ok());
        match choice {
            Some(choice) => self.commit_screenshot_directory(choice),
            None => {
                self.screenshot_failure(Error::Directory);
                false
            }
        }
    }

    pub fn reset_screenshot_directory(self: Pin<&mut Self>) -> bool {
        self.commit_screenshot_directory(ScreenshotDirectory::Pictures)
    }

    pub fn open_screenshot_directory(self: Pin<&mut Self>) -> bool {
        let directory = self
            .resolved_screenshot_directory(
                &self.rust().preferences.preferences().screenshot_directory,
            )
            .and_then(|path| {
                // Existing screenshots remain viewable if the folder becomes read-only.
                std::fs::create_dir_all(&path)?;
                Ok(path)
            });
        match directory {
            Ok(directory) => {
                let path = QString::from(directory.to_string_lossy().as_ref());
                if crate::qt::ffi::open_local_directory(&path) {
                    self.set_screenshot_error(QString::default());
                    true
                } else {
                    self.set_screenshot_error(tr("Could not open the screenshot folder."));
                    false
                }
            }
            Err(error) => {
                self.screenshot_failure(error);
                false
            }
        }
    }

    pub fn screenshot_format(&self) -> QString {
        QString::from(
            self.rust()
                .preferences
                .preferences()
                .screenshot_format
                .key(),
        )
    }

    pub fn open_screenshot_file_directory(self: Pin<&mut Self>, file: QUrl) -> bool {
        let directory = file.to_local_file().and_then(|file| {
            let path = PathBuf::from(file.to_string());
            path.is_absolute()
                .then(|| path.parent().map(PathBuf::from))
                .flatten()
        });
        if let Some(directory) = directory
            && directory.is_dir()
            && crate::qt::ffi::open_local_directory(&QString::from(
                directory.to_string_lossy().as_ref(),
            ))
        {
            self.set_screenshot_error(QString::default());
            return true;
        }
        self.set_screenshot_error(tr("Could not open the screenshot folder."));
        false
    }

    pub fn configure_screenshot_format(mut self: Pin<&mut Self>, format: QString) -> bool {
        let Some(format) = ScreenshotFormat::from_key(&format.to_string()) else {
            return false;
        };
        if self.rust().preferences.preferences().screenshot_format != format {
            self.as_mut()
                .rust_mut()
                .preferences
                .change(Change::ScreenshotFormat(format));
            self.as_mut().screenshot_format_changed();
            self.save_settings();
        }
        true
    }

    pub fn screenshot_options(&self) -> QString {
        let options = self.rust().preferences.preferences().screenshot_options;
        let mut value = serde_json::to_value(options).expect("screenshot parameters");
        use crate::settings::{JpgQuality, PngCompression, WebpQuality};
        value["ranges"] = serde_json::json!({
            "png": {"min": PngCompression::MIN, "max": PngCompression::MAX},
            "jpg": {"min": JpgQuality::MIN, "max": JpgQuality::MAX},
            "webp": {"min": WebpQuality::MIN, "max": WebpQuality::MAX},
        });
        QString::from(value.to_string().as_str())
    }

    pub fn configure_screenshot_options(
        mut self: Pin<&mut Self>,
        format: QString,
        value: i32,
        lossless: bool,
    ) -> bool {
        let Some(format) = ScreenshotFormat::from_key(&format.to_string()) else {
            return false;
        };
        let previous = self.rust().preferences.preferences().screenshot_options;
        let mut options = previous;
        match format {
            ScreenshotFormat::Png => {
                let Ok(value) = value.try_into() else {
                    return false;
                };
                options.png_compression = value;
            }
            ScreenshotFormat::Jpg => {
                let Ok(value) = value.try_into() else {
                    return false;
                };
                options.jpg_quality = value;
            }
            ScreenshotFormat::Webp => {
                let Ok(value) = value.try_into() else {
                    return false;
                };
                options.webp_quality = value;
                options.webp_mode = if lossless {
                    WebpMode::Lossless
                } else {
                    WebpMode::Lossy
                };
            }
        }
        if options != previous {
            self.as_mut()
                .rust_mut()
                .preferences
                .change(Change::ScreenshotOptions(options));
            self.as_mut().screenshot_options_changed();
            self.save_settings();
        }
        true
    }

    pub fn screenshot_busy(&self) -> bool {
        !self.rust().screenshot_saves.is_empty()
    }

    pub fn stage_screenshot(&self, overlay: QString) {
        if let Some(playback) = self.rust().media.playback() {
            match crate::screenshots::overlay::Overlay::parse(&overlay.to_string()) {
                Ok(overlay) => playback.presentation().stage(overlay),
                Err(error) => {
                    playback.presentation().clear();
                    tracing::warn!("Invalid screenshot overlay: {error}");
                }
            }
        }
    }

    pub fn capture_screenshot(self: Pin<&mut Self>) -> bool {
        if !self.media_active() || self.seeking() {
            return self.submit_screenshot(Err(Error::Unavailable));
        }
        let capture = self
            .rust()
            .media
            .playback()
            .ok_or(Error::Unavailable)
            .and_then(|playback| playback.presentation().capture())
            .and_then(Captured::presented);
        self.submit_screenshot(capture)
    }

    pub fn save_screenshot(self: Pin<&mut Self>, image: &QImage) -> bool {
        let capture = Captured::image(image);
        self.submit_screenshot(capture)
    }

    fn submit_screenshot(mut self: Pin<&mut Self>, capture: Result<Captured, Error>) -> bool {
        let preferences = self.rust().preferences.preferences();
        let directory = self.resolved_screenshot_directory(&preferences.screenshot_directory);
        let encoding = preferences
            .screenshot_format
            .encoding(preferences.screenshot_options);
        let result = capture.and_then(|capture| {
            directory.and_then(|directory| {
                self.as_mut()
                    .rust_mut()
                    .screenshot_saves
                    .submit(capture, directory, encoding)
            })
        });
        match result {
            Ok(_) => {
                self.as_mut().set_screenshot_error(QString::default());
                self.screenshot_busy_changed();
                true
            }
            Err(error) => {
                self.screenshot_save_failure(error);
                false
            }
        }
    }

    pub fn poll_screenshot(mut self: Pin<&mut Self>) {
        let completed = self.as_mut().rust_mut().screenshot_saves.poll();
        if completed.is_empty() {
            return;
        }
        for result in completed {
            let file = match result.result {
                Ok(path) => {
                    self.as_mut().set_screenshot_error(QString::default());
                    QUrl::from_local_file(&QString::from(path.to_string_lossy().as_ref()))
                }
                Err(error) => {
                    self.as_mut().screenshot_save_failure(error);
                    QUrl::default()
                }
            };
            self.as_mut().screenshot_finished(file);
        }
        self.screenshot_busy_changed();
    }

    fn screenshot_save_failure(self: Pin<&mut Self>, error: Error) {
        match error {
            Error::Capacity => self.set_screenshot_error(tr(
                "Too many screenshots are waiting to save. Try again shortly.",
            )),
            Error::Unavailable => self.set_screenshot_error(tr(
                "Could not capture the picture. Try again while the video is playing.",
            )),
            Error::Encode | Error::WorkerStopped => {
                tracing::error!("Screenshot operation failed: {error}");
                self.set_screenshot_error(tr(
                            "Could not save the captured image. Try again or choose another format in Settings.",
                        ));
            }
            Error::Directory | Error::Io(_) => self.screenshot_failure(error),
        }
    }
}
