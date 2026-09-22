//! Image files become visible only after encoding succeeds, without overwriting
//! an earlier capture even when several instances capture in the same millisecond.
use crate::settings::ScreenshotEncoding;
#[cfg(test)]
use crate::settings::ScreenshotFormat;
use cxx_qt_lib::{DateFormat, QDateTime, QImage, QString};
use std::{fs, io, path::PathBuf};
#[path = "screenshot_native.rs"]
pub mod native;
#[path = "screenshot_overlay.rs"]
pub mod overlay;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Screenshot queue is full")]
    Capacity,
    #[error("No presented video frame is available")]
    Unavailable,
    #[error("No absolute screenshot directory is available")]
    Directory,
    #[error("Could not encode the captured image")]
    Encode,
    #[error("The screenshot worker stopped unexpectedly")]
    WorkerStopped,
    #[error("Screenshot file operation failed: {0}")]
    Io(#[from] io::Error),
}

/// Own the writable temporary file in the destination before encoding starts.
/// Dropping an unused/failed capture removes it; only save can publish an image.
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

    #[cfg(test)]
    pub fn save(self, image: &QImage, format: ScreenshotFormat) -> Result<PathBuf, Error> {
        let stamp = QDateTime::current_date_time()
            .format_enum(DateFormat::ISODateWithMs)
            .to_string()
            .replace(['-', ':'], "")
            .replace(['T', '.'], "-");
        self.save_at(image, format.into(), &stamp)
    }

    fn save_at(
        self,
        image: &QImage,
        encoding: ScreenshotEncoding,
        stamp: &str,
    ) -> Result<PathBuf, Error> {
        let format = encoding.format();
        if image.is_null()
            || !crate::qt::ffi::save_screenshot_image(
                image,
                &QString::from(self.temporary.path().to_string_lossy().as_ref()),
                &QString::from(format.key()),
                encoding.quality(),
                encoding.compression(),
            )
        {
            return Err(Error::Encode);
        }
        let extension = format.key();
        let mut temporary = self.temporary;
        for suffix in 0_u64.. {
            let filename = if suffix == 0 {
                format!("Mirakurun-{stamp}.{extension}")
            } else {
                format!("Mirakurun-{stamp}-{suffix}.{extension}")
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

// Small worker count keeps encoding from consuming every playback CPU. The
// memory budget includes a native frame and its output image during conversion.
const SAVE_WORKERS: usize = 2;
const MAX_CAPTURES: usize = 32;
const MAX_CAPTURE_BYTES: usize = 256 * 1024 * 1024;

/// A fixed image, never a deferred request to take a later frame. Only these
/// constructors may create accepted capture work and account for its memory.
pub struct Captured {
    pixels: Box<dyn FnOnce() -> Result<QImage, Error> + Send>,
    bytes: usize,
    stamp: String,
}
impl Captured {
    pub fn presented(frame: native::Presented) -> Result<Self, Error> {
        Ok(Self::new(frame.bytes()?, move || frame.image()))
    }
    pub fn image(image: &QImage) -> Result<Self, Error> {
        if image.is_null() {
            return Err(Error::Encode);
        }
        let bytes = (image.width() as usize)
            .checked_mul(image.height() as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or(Error::Capacity)?;
        let image = crate::qt::ffi::share_screenshot_image(image);
        Ok(Self::new(bytes, move || Ok(image)))
    }
    fn new(bytes: usize, pixels: impl FnOnce() -> Result<QImage, Error> + Send + 'static) -> Self {
        let stamp = QDateTime::current_date_time()
            .format_enum(DateFormat::ISODateWithMs)
            .to_string()
            .replace(['-', ':'], "")
            .replace(['T', '.'], "-");
        Self {
            pixels: Box::new(pixels),
            bytes,
            stamp,
        }
    }
}

struct Accepted {
    id: u64,
    capture: Captured,
    directory: PathBuf,
    encoding: ScreenshotEncoding,
}
struct Running {
    id: u64,
    bytes: usize,
    worker: std::thread::JoinHandle<Result<PathBuf, Error>>,
}
pub struct Completed {
    pub id: u64,
    pub result: Result<PathBuf, Error>,
}
#[derive(Default)]
pub struct Queue {
    waiting: std::collections::VecDeque<Accepted>,
    running: Vec<Running>,
    bytes: usize,
    next_id: u64,
}
impl Queue {
    pub fn len(&self) -> usize {
        self.waiting.len() + self.running.len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn submit(
        &mut self,
        capture: Captured,
        directory: PathBuf,
        encoding: ScreenshotEncoding,
    ) -> Result<u64, Error> {
        if self.len() >= MAX_CAPTURES
            || capture.bytes > MAX_CAPTURE_BYTES.saturating_sub(self.bytes)
        {
            return Err(Error::Capacity);
        }
        self.next_id = self.next_id.checked_add(1).ok_or(Error::Capacity)?;
        self.bytes += capture.bytes;
        self.waiting.push_back(Accepted {
            id: self.next_id,
            capture,
            directory,
            encoding,
        });
        Ok(self.next_id)
    }
    // Poll only joins completed workers. A request owns its image and settings
    // from acceptance through atomic publication, even after playback changes.
    pub fn poll(&mut self) -> Vec<Completed> {
        let mut completed = Vec::new();
        let mut index = 0;
        while index < self.running.len() {
            if !self.running[index].worker.is_finished() {
                index += 1;
                continue;
            }
            let running = self.running.swap_remove(index);
            self.bytes -= running.bytes;
            completed.push(Completed {
                id: running.id,
                result: running.worker.join().unwrap_or(Err(Error::WorkerStopped)),
            });
        }
        while self.running.len() < SAVE_WORKERS {
            let Some(job) = self.waiting.pop_front() else {
                break;
            };
            let id = job.id;
            let bytes = job.capture.bytes;
            match std::thread::Builder::new()
                .name(format!("screenshot-save-{id}"))
                .spawn(move || {
                    let image = (job.capture.pixels)()?;
                    Prepared::new(job.directory)?.save_at(&image, job.encoding, &job.capture.stamp)
                }) {
                Ok(worker) => self.running.push(Running { id, bytes, worker }),
                Err(error) => {
                    self.bytes -= bytes;
                    completed.push(Completed {
                        id,
                        result: Err(error.into()),
                    });
                }
            }
        }
        completed
    }
    pub fn finish(&mut self) {
        while !self.is_empty() {
            for completed in self.poll() {
                if let Err(error) = completed.result {
                    tracing::error!(
                        "Screenshot {} failed during shutdown: {error}",
                        completed.id
                    );
                }
            }
            // Shutdown alone may wait. Normal event-loop polling never joins
            // a worker before its completion.
            for running in self.running.drain(..) {
                self.bytes -= running.bytes;
                match running.worker.join() {
                    Ok(Ok(_)) => {}
                    result => tracing::error!(
                        "Screenshot {} failed during shutdown: {result:?}",
                        running.id
                    ),
                }
            }
        }
    }
}
impl Drop for Queue {
    fn drop(&mut self) {
        self.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cxx_qt_lib::{QColor, QImageFormat};

    #[test]
    fn queue_runs_two_workers_without_waiting_and_keeps_settings_per_capture()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::{sync::mpsc, time::Duration};
        let directory = tempfile::tempdir()?;
        let mut queue = Queue::default();
        let (started, starts) = mpsc::channel();
        let mut releases = Vec::new();
        for format in [ScreenshotFormat::Png, ScreenshotFormat::Webp] {
            let (release, gate) = mpsc::channel();
            releases.push(release);
            let started = started.clone();
            let capture = Captured::new(16, move || {
                started.send(()).unwrap();
                gate.recv_timeout(Duration::from_secs(5))
                    .map_err(|_| Error::WorkerStopped)?;
                let mut image =
                    QImage::from_width_height_and_format(2, 2, QImageFormat::Format_RGB32);
                image.fill(&QColor::from_rgb(255, 0, 0));
                Ok(image)
            });
            queue.submit(capture, directory.path().into(), format.into())?;
        }
        let failed = queue.submit(
            Captured::new(16, || Err(Error::Encode)),
            directory.path().into(),
            ScreenshotFormat::Jpg.into(),
        )?;
        assert!(queue.poll().is_empty());
        starts.recv_timeout(Duration::from_secs(2))?;
        starts.recv_timeout(Duration::from_secs(2))?;
        assert!(
            queue.poll().is_empty(),
            "poll must not join the blocked workers"
        );
        assert_eq!(queue.running.len(), SAVE_WORKERS);
        assert_eq!(queue.waiting.len(), 1);
        // Release in reverse order: completion may be out of capture order.
        for release in releases.into_iter().rev() {
            release.send(())?;
        }
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut outcomes = Vec::new();
        while !queue.is_empty() {
            outcomes.extend(queue.poll());
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert_eq!(outcomes.len(), 3);
        assert!(
            outcomes
                .iter()
                .any(|result| result.id == failed && matches!(result.result, Err(Error::Encode)))
        );
        let mut extensions = outcomes
            .into_iter()
            .filter_map(|result| result.result.ok())
            .map(|path| path.extension().unwrap().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        extensions.sort();
        assert_eq!(extensions, ["png", "webp"]);
        assert_eq!(queue.bytes, 0);
        Ok(())
    }

    #[test]
    fn queue_limits_memory_and_count_and_finishes_accepted_images_on_drop()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let mut image = QImage::from_width_height_and_format(2, 2, QImageFormat::Format_RGB32);
        image.fill(&QColor::from_rgb(255, 0, 0));
        let mut queue = Queue::default();
        assert!(matches!(
            queue.submit(
                Captured::new(MAX_CAPTURE_BYTES + 1, || unreachable!()),
                directory.path().into(),
                ScreenshotFormat::Png.into()
            ),
            Err(Error::Capacity)
        ));
        for _ in 0..MAX_CAPTURES {
            queue.submit(
                Captured::image(&image)?,
                directory.path().into(),
                ScreenshotFormat::Png.into(),
            )?;
        }
        assert!(matches!(
            queue.submit(
                Captured::image(&image)?,
                directory.path().into(),
                ScreenshotFormat::Png.into()
            ),
            Err(Error::Capacity)
        ));
        image.fill(&QColor::from_rgb(0, 255, 0));
        drop(queue);
        let files = fs::read_dir(directory.path())?.collect::<Result<Vec<_>, _>>()?;
        assert_eq!(files.len(), MAX_CAPTURES);
        for file in files {
            let image = QImage::from_data(&fs::read(file.path())?, Some("png")).unwrap();
            assert_eq!(image.pixel_color(0, 0), QColor::from_rgb(255, 0, 0));
        }
        Ok(())
    }

    #[test]
    fn compression_modes_encode_the_requested_formats_and_preserve_lossless_pixels()
    -> Result<(), Box<dyn std::error::Error>> {
        use crate::settings::{ScreenshotOptions, WebpMode};
        let directory = tempfile::tempdir()?;
        let mut image = QImage::from_width_height_and_format(128, 96, QImageFormat::Format_RGB32);
        for y in 0..image.height() {
            for x in 0..image.width() {
                image.set_pixel_color(
                    x,
                    y,
                    &QColor::from_rgb((x * 13 + y * 7) % 256, (x + y * 19) % 256, x * 2),
                );
            }
        }
        let mut png_sizes = Vec::new();
        for level in [0, 6, 9] {
            let options = ScreenshotOptions {
                png_compression: level.try_into()?,
                ..Default::default()
            };
            let path = Prepared::new(directory.path().into())?.save_at(
                &image,
                ScreenshotFormat::Png.encoding(options),
                &level.to_string(),
            )?;
            let bytes = fs::read(path)?;
            png_sizes.push(bytes.len());
            assert_eq!(QImage::from_data(&bytes, Some("png")).unwrap(), image);
        }
        assert!(
            png_sizes[0] > png_sizes[2],
            "compression level had no effect"
        );
        for format in [ScreenshotFormat::Jpg, ScreenshotFormat::Webp] {
            let mut sizes = Vec::new();
            for quality in [20, 90] {
                let options = ScreenshotOptions {
                    jpg_quality: quality.try_into()?,
                    webp_quality: quality.try_into()?,
                    ..Default::default()
                };
                let path = Prepared::new(directory.path().into())?.save_at(
                    &image,
                    format.encoding(options),
                    &format!("quality-{quality}"),
                )?;
                sizes.push(fs::metadata(path)?.len());
            }
            assert!(sizes[1] > sizes[0], "{format:?} quality had no effect");
        }
        for mode in [WebpMode::Lossy, WebpMode::Lossless] {
            let options = ScreenshotOptions {
                webp_quality: 99.try_into()?,
                webp_mode: mode,
                ..Default::default()
            };
            let path = Prepared::new(directory.path().into())?.save_at(
                &image,
                ScreenshotFormat::Webp.encoding(options),
                "webp",
            )?;
            let bytes = fs::read(path)?;
            let decoded = QImage::from_data(&bytes, Some("webp")).unwrap();
            match mode {
                WebpMode::Lossy => {
                    assert_eq!(&bytes[12..16], b"VP8 ");
                    assert_ne!(decoded, image);
                }
                WebpMode::Lossless => {
                    assert_eq!(&bytes[12..16], b"VP8L");
                    for y in 0..image.height() {
                        for x in 0..image.width() {
                            assert_eq!(decoded.pixel_color(x, y), image.pixel_color(x, y));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    #[test]
    fn encodes_all_formats_with_matching_extensions() -> Result<(), Box<dyn std::error::Error>> {
        let temporary = tempfile::tempdir()?;
        let mut image = QImage::from_width_height_and_format(32, 24, QImageFormat::Format_RGB32);
        image.fill(&QColor::from_rgb(255, 0, 0));
        for format in [
            ScreenshotFormat::Png,
            ScreenshotFormat::Jpg,
            ScreenshotFormat::Webp,
        ] {
            let path = Prepared::new(temporary.path().into())?.save(&image, format)?;
            assert_eq!(
                path.extension().and_then(|extension| extension.to_str()),
                Some(format.key())
            );
            let bytes = fs::read(path)?;
            match format {
                ScreenshotFormat::Png => assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n")),
                ScreenshotFormat::Jpg => assert!(bytes.starts_with(b"\xff\xd8\xff")),
                ScreenshotFormat::Webp => {
                    assert!(bytes.starts_with(b"RIFF"));
                    assert_eq!(&bytes[8..12], b"WEBP");
                }
            }
            let decoded = QImage::from_data(&bytes, Some(format.key())).expect("decodable image");
            assert_eq!(decoded.size(), image.size());
            assert!(decoded.pixel_color(0, 0).red() > 240);
            assert!(matches!(
                Prepared::new(temporary.path().into())?.save(&QImage::default(), format),
                Err(Error::Encode)
            ));
        }
        assert_eq!(fs::read_dir(temporary.path())?.count(), 3);
        Ok(())
    }

    #[test]
    fn repeated_capture_preserves_existing_png_and_cleans_up_failed_image()
    -> Result<(), Box<dyn std::error::Error>> {
        let temporary = tempfile::tempdir()?;
        let directory = temporary.path().join("画像 #100%");
        let mut image = QImage::from_width_height_and_format(4, 3, QImageFormat::Format_RGB32);
        image.fill(&QColor::from_rgb(255, 0, 0));
        let first = Prepared::new(directory.clone())?.save_at(
            &image,
            ScreenshotFormat::Png.into(),
            "20260914-123456-789",
        )?;
        let bytes = fs::read(&first)?;
        assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
        image.fill(&QColor::from_rgb(0, 255, 0));
        let second = Prepared::new(directory.clone())?.save_at(
            &image,
            ScreenshotFormat::Png.into(),
            "20260914-123456-789",
        )?;
        assert_ne!(first, second);
        assert_eq!(fs::read(first)?, bytes);
        assert_ne!(fs::read(second)?, bytes);
        assert!(matches!(
            Prepared::new(directory.clone())?.save(&QImage::default(), ScreenshotFormat::Png),
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
