use super::{ffi, status::tr};
use crate::{
    screenshots::{Error, Prepared},
    settings::{Change, ScreenshotDirectory},
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
            .resolve(&PathBuf::from(ffi::pictures_directory().to_string()))
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

    fn prepare_screenshot(&self) -> Result<Prepared, Error> {
        Prepared::new(self.resolved_screenshot_directory(
            &self.rust().preferences.preferences().screenshot_directory,
        )?)
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
                if ffi::open_local_directory(&path) {
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

    pub fn save_screenshot(mut self: Pin<&mut Self>, image: &QImage) -> QUrl {
        match self
            .prepare_screenshot()
            .and_then(|prepared| prepared.save(image))
        {
            Ok(path) => {
                self.set_screenshot_error(QString::default());
                QUrl::from_local_file(&QString::from(path.to_string_lossy().as_ref()))
            }
            Err(error) => {
                match error {
                    Error::Encode => self.as_mut().set_screenshot_error(tr(
                        "Could not save the captured image as PNG. Try capturing again.",
                    )),
                    Error::Directory | Error::Io(_) => self.as_mut().screenshot_failure(error),
                }
                QUrl::default()
            }
        }
    }
}
