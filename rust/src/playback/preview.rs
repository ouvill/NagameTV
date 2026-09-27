//! On-demand stills, independent of the playback cursor and Qt. One worker and
//! one replacement request bound decoder/I/O use during rapid pointer movement.
mod decoder;
use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
};

const BUCKET_MS: u64 = 2_000;
const CACHE_ENTRIES: usize = 24;

#[derive(Clone)]
pub struct Source {
    identity: u64,
    input: Input,
}
#[derive(Clone)]
enum Input {
    Transport(super::input::preview::Source),
    Media(super::recording::MediaFile),
}
impl Source {
    pub(super) fn transport(identity: u64, source: super::input::preview::Source) -> Self {
        Self {
            identity,
            input: Input::Transport(source),
        }
    }
    pub(super) fn media(identity: u64, file: super::recording::MediaFile) -> Self {
        Self {
            identity,
            input: Input::Media(file),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Key {
    identity: u64,
    milliseconds: u64,
}
impl Key {
    fn new(identity: u64, milliseconds: f64) -> Option<Self> {
        (milliseconds.is_finite() && milliseconds >= 0.0 && milliseconds < u64::MAX as f64).then(
            || Self {
                identity,
                milliseconds: milliseconds as u64 / BUCKET_MS * BUCKET_MS,
            },
        )
    }
}
struct Request {
    key: Key,
    source: Source,
    target_ms: u64,
}
struct Job {
    cancelled: Arc<AtomicBool>,
    thread: JoinHandle<Result<String, decoder::Error>>,
    key: Key,
}
#[derive(Default)]
enum Work {
    #[default]
    Idle,
    Running(Job),
    Cancelling(Job),
}

#[derive(Default)]
pub struct Controller {
    identity: Option<u64>,
    desired: Option<Key>,
    pending: Option<Request>,
    work: Work,
    cache: VecDeque<(Key, String)>,
    image: String,
    revision: u64,
}
impl Controller {
    pub fn image(&self) -> &str {
        &self.image
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    fn publish(&mut self, image: String) {
        if self.image != image {
            self.image = image;
            self.revision = self.revision.wrapping_add(1);
        }
    }

    pub fn synchronize(&mut self, identity: Option<u64>) {
        if self.identity != identity {
            self.clear();
            self.cache.clear();
            self.identity = identity;
        }
    }
    pub fn clear(&mut self) {
        self.desired = None;
        self.pending = None;
        self.publish(String::new());
        self.work = match std::mem::take(&mut self.work) {
            Work::Running(job) | Work::Cancelling(job) => {
                job.cancelled.store(true, Ordering::Release);
                Work::Cancelling(job)
            }
            Work::Idle => Work::Idle,
        };
    }
    pub fn request(&mut self, source: Source, milliseconds: f64) {
        self.synchronize(Some(source.identity));
        let Some(key) = Key::new(source.identity, milliseconds) else {
            self.clear();
            return;
        };
        if self.desired == Some(key) {
            return;
        }
        self.clear();
        self.desired = Some(key);
        if let Some((_, image)) = self.cache.iter().find(|(cached, _)| *cached == key) {
            self.publish(image.clone());
        } else {
            self.pending = Some(Request {
                key,
                source,
                target_ms: milliseconds as u64,
            });
        }
        self.poll();
    }
    pub fn poll(&mut self) {
        let work = std::mem::take(&mut self.work);
        self.work = match work {
            Work::Running(job) | Work::Cancelling(job) if !job.thread.is_finished() => {
                if job.cancelled.load(Ordering::Acquire) {
                    Work::Cancelling(job)
                } else {
                    Work::Running(job)
                }
            }
            Work::Running(job) | Work::Cancelling(job) => {
                match job.thread.join() {
                    Ok(Ok(image)) if self.desired == Some(job.key) => {
                        self.publish(image.clone());
                        self.cache.push_back((job.key, image));
                        while self.cache.len() > CACHE_ENTRIES {
                            self.cache.pop_front();
                        }
                    }
                    // Superseded requests and expired live data are normal cancellations.
                    Ok(Ok(_))
                    | Ok(Err(decoder::Error::Cancelled | decoder::Error::Unavailable)) => {}
                    Ok(Err(error)) => tracing::warn!(
                        error = &error as &dyn std::error::Error,
                        "Seek preview failed"
                    ),
                    Err(_) => tracing::warn!("Seek preview worker panicked"),
                }
                Work::Idle
            }
            Work::Idle => Work::Idle,
        };
        if matches!(self.work, Work::Idle)
            && let Some(request) = self.pending.take()
        {
            let cancelled = Arc::new(AtomicBool::new(false));
            let cancellation = cancelled.clone();
            match std::thread::Builder::new()
                .name("seek-preview".into())
                .spawn(move || {
                    decoder::capture(request.source.input, request.target_ms, cancellation)
                }) {
                Ok(thread) => {
                    self.work = Work::Running(Job {
                        cancelled,
                        thread,
                        key: request.key,
                    })
                }
                Err(error) => tracing::warn!(
                    error = &error as &dyn std::error::Error,
                    "Could not start seek preview"
                ),
            }
        }
    }
}
impl Drop for Controller {
    fn drop(&mut self) {
        self.clear();
        if let Work::Running(job) | Work::Cancelling(job) = std::mem::take(&mut self.work) {
            match job.thread.join() {
                Ok(Ok(_)) | Ok(Err(decoder::Error::Cancelled | decoder::Error::Unavailable)) => {}
                Ok(Err(error)) => tracing::warn!(
                    error = &error as &dyn std::error::Error,
                    "Stopping seek preview failed"
                ),
                Err(_) => tracing::warn!("Seek preview worker panicked during shutdown"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    pub(super) fn wait(controller: &mut Controller) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !matches!(controller.work, Work::Idle) || controller.pending.is_some() {
            assert!(Instant::now() < deadline, "preview completion deadline");
            controller.poll();
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn media_preview_decodes_without_display_or_audio() -> Result<(), Box<dyn std::error::Error>> {
        gstreamer::init()?;
        for fixture in ["media-h264.mp4", "media-h264.mkv"] {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../tests/fixtures")
                .join(fixture);
            let super::super::recording::Recording::Media(file) =
                super::super::recording::Recording::open(&path)?
            else {
                panic!("media fixture")
            };
            let mut controller = Controller::default();
            controller.request(Source::media(1, file.clone()), 0.0);
            wait(&mut controller);
            let first = controller.image().to_owned();
            controller.request(Source::media(1, file), 3000.0);
            wait(&mut controller);
            assert_ne!(
                controller.image(),
                first,
                "media seek must produce the target frame"
            );
            assert!(
                controller
                    .image()
                    .starts_with("data:image/png;base64,iVBOR"),
                "{fixture} preview missing"
            );
            controller.synchronize(Some(2));
            assert!(controller.image().is_empty());
            assert!(controller.cache.is_empty());
        }
        Ok(())
    }

    #[test]
    fn cancellation_retains_worker_until_finished_and_rejects_late_image() {
        let (send, receive) = std::sync::mpsc::channel();
        let key = Key::new(1, 5000.0).unwrap();
        let mut controller = Controller::default();
        controller.synchronize(Some(1));
        controller.desired = Some(key);
        controller.work = Work::Running(Job {
            key,
            cancelled: Arc::new(AtomicBool::new(false)),
            thread: std::thread::spawn(move || {
                receive.recv().unwrap();
                Ok("old image".into())
            }),
        });
        controller.clear();
        controller.clear();
        controller.synchronize(Some(2));
        assert!(matches!(controller.work, Work::Cancelling(_)));
        send.send(()).unwrap();
        wait(&mut controller);
        assert!(controller.image().is_empty());
        assert!(controller.cache.is_empty());
    }

    #[test]
    fn invalid_positions_cannot_construct_requests() {
        for position in [f64::NAN, f64::INFINITY, -1.0, u64::MAX as f64] {
            assert!(Key::new(1, position).is_none());
        }
        assert_eq!(Key::new(1, 2001.0), Key::new(1, 3999.0));
        assert_ne!(Key::new(1, 2001.0), Key::new(2, 2001.0));
    }
}
