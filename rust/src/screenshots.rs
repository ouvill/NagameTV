//! PNG files become visible only after encoding succeeds, without overwriting
//! an earlier capture even when several instances capture in the same millisecond.
use cxx_qt_lib::{DateFormat, QDateTime, QImage, QString};
use std::{fs, io, path::PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("No absolute screenshot directory is available")]
    Directory,
    #[error("Could not encode the captured image as PNG")]
    Encode,
    #[error("Screenshot file operation failed: {0}")]
    Io(#[from] io::Error),
}

/// Own the writable temporary file in the destination before encoding starts.
/// Dropping an unused/failed capture removes it; only save can publish a PNG.
pub struct Prepared {
    directory: PathBuf,
    temporary: tempfile::NamedTempFile,
}

impl Prepared {
    pub fn new(directory: PathBuf) -> Result<Self, Error> {
        if !directory.is_absolute() {
            return Err(Error::Directory);
        }
        fs::create_dir_all(&directory)?;
        let temporary = tempfile::Builder::new()
            .prefix(".mirakurun-shot-")
            .tempfile_in(&directory)?;
        Ok(Self {
            directory,
            temporary,
        })
    }

    pub fn save(self, image: &QImage) -> Result<PathBuf, Error> {
        let stamp = QDateTime::current_date_time()
            .format_enum(DateFormat::ISODateWithMs)
            .to_string()
            .replace(['-', ':'], "")
            .replace(['T', '.'], "-");
        self.save_at(image, &stamp)
    }

    fn save_at(self, image: &QImage, stamp: &str) -> Result<PathBuf, Error> {
        if image.is_null()
            || !crate::player::ffi::save_screenshot_png(
                image,
                &QString::from(self.temporary.path().to_string_lossy().as_ref()),
            )
        {
            return Err(Error::Encode);
        }
        let mut temporary = self.temporary;
        for suffix in 0_u64.. {
            let filename = if suffix == 0 {
                format!("Mirakurun-{stamp}.png")
            } else {
                format!("Mirakurun-{stamp}-{suffix}.png")
            };
            let path = self.directory.join(filename);
            match temporary.persist_noclobber(&path) {
                Ok(_) => return Ok(path),
                Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {
                    temporary = error.file;
                }
                Err(error) => return Err(Error::Io(error.error)),
            }
        }
        unreachable!("exhausted screenshot suffixes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cxx_qt_lib::{QColor, QImageFormat};

    #[test]
    fn repeated_capture_preserves_existing_png_and_cleans_up_failed_image()
    -> Result<(), Box<dyn std::error::Error>> {
        let temporary = tempfile::tempdir()?;
        let directory = temporary.path().join("画像 #100%");
        let mut image = QImage::from_width_height_and_format(4, 3, QImageFormat::Format_RGB32);
        image.fill(&QColor::from_rgb(255, 0, 0));
        let first = Prepared::new(directory.clone())?.save_at(&image, "20260914-123456-789")?;
        let bytes = fs::read(&first)?;
        assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
        image.fill(&QColor::from_rgb(0, 255, 0));
        let second = Prepared::new(directory.clone())?.save_at(&image, "20260914-123456-789")?;
        assert_ne!(first, second);
        assert_eq!(fs::read(first)?, bytes);
        assert_ne!(fs::read(second)?, bytes);
        assert!(matches!(
            Prepared::new(directory.clone())?.save(&QImage::default()),
            Err(Error::Encode)
        ));
        drop(Prepared::new(directory.clone())?);
        assert_eq!(fs::read_dir(directory)?.count(), 2);
        Ok(())
    }

    #[test]
    fn rejects_relative_or_unusable_directory() -> Result<(), Box<dyn std::error::Error>> {
        assert!(matches!(
            Prepared::new("relative".into()),
            Err(Error::Directory)
        ));
        let directory = tempfile::tempdir()?;
        let file = directory.path().join("file");
        fs::write(&file, b"keep")?;
        assert!(matches!(Prepared::new(file.clone()), Err(Error::Io(_))));
        assert_eq!(fs::read(file)?, b"keep");
        Ok(())
    }
}
