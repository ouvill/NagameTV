//! One cancellable range reader; replacements wait for the old worker to finish.
use super::{Error, List, mp4};
use crate::playback::recording::MediaFile;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
};

pub(crate) struct Source {
    identity: u64,
    mp4: Option<MediaFile>,
}
impl Source {
    pub(in crate::playback) fn media(identity: u64, file: MediaFile) -> Self {
        let mp4 = file.is_mp4().then_some(file);
        Self { identity, mp4 }
    }
    pub(in crate::playback) fn transport(identity: u64) -> Self {
        Self {
            identity,
            mp4: None,
        }
    }
}
struct Job {
    cancelled: Arc<AtomicBool>,
    thread: JoinHandle<Result<List, Error>>,
}
impl Job {
    fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
    fn finish(self) -> Option<List> {
        let cancelled = self.cancelled.load(Ordering::Acquire);
        match self.thread.join() {
            Ok(result) if cancelled => {
                // Replaced sources and shutdown intentionally discard all late results.
                drop(result);
                None
            }
            Ok(Ok(list)) => Some(list),
            Ok(Err(Error::Cancelled)) => None,
            Ok(Err(error)) => {
                tracing::warn!(
                    operation = "read recording chapters",
                    error = &error as &dyn std::error::Error,
                    "Chapter discovery unavailable"
                );
                None
            }
            Err(_) => {
                tracing::warn!(
                    operation = "read recording chapters",
                    "Chapter discovery worker panicked"
                );
                None
            }
        }
    }
}
#[derive(Default)]
enum Work {
    #[default]
    Idle,
    Reading(Job),
    Retiring(Job),
}
#[derive(Default)]
pub(crate) struct Controller {
    identity: Option<u64>,
    pending: Option<MediaFile>,
    work: Work,
    fallback: List,
    selected: List,
    duration: Option<u64>,
    visible: bool,
    display: List,
    current: Option<usize>,
    revision: u64,
}
impl Controller {
    #[cfg(feature = "native_tests")]
    pub(crate) fn seed_for_check(&mut self, toc: &gstreamer::TocRef) {
        self.update(
            Some(Source::transport(u64::MAX)),
            Some(List::from_toc(toc).expect("test TOC")),
            true,
            Some(0),
            None,
        );
    }
    #[cfg(feature = "native_tests")]
    pub(crate) fn range_for_check(&mut self, seekable: bool, position: Option<u64>) {
        self.update(
            Some(Source::transport(u64::MAX)),
            None,
            seekable,
            position,
            None,
        );
    }
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }
    pub(crate) fn json(&self) -> &str {
        self.display.json()
    }
    pub(crate) fn current(&self) -> i32 {
        self.current.map_or(-1, |index| index as i32)
    }
    pub(crate) fn target(&self, offset: i32, position: Option<u64>) -> Option<u64> {
        self.display.target(offset, position?)
    }
    pub(crate) fn update(
        &mut self,
        source: Option<Source>,
        toc: Option<List>,
        seekable: bool,
        position: Option<u64>,
        duration: Option<u64>,
    ) {
        let identity = source.as_ref().map(|source| source.identity);
        let source_changed = self.identity != identity;
        if source_changed {
            self.work = match std::mem::take(&mut self.work) {
                Work::Reading(job) | Work::Retiring(job) => {
                    job.cancel();
                    Work::Retiring(job)
                }
                Work::Idle => Work::Idle,
            };
            self.pending = source.and_then(|source| source.mp4);
            self.fallback = List::default();
            self.identity = identity;
        }
        self.poll();
        let before = self.display.clone();
        let before_current = self.current;
        let selected = toc
            .filter(|list| !list.entries.is_empty())
            .unwrap_or_else(|| {
                if source_changed || self.selected.entries.is_empty() {
                    self.fallback.clone()
                } else {
                    self.selected.clone()
                }
            });
        // Seek flushes can temporarily lose range/metadata answers. Retained
        // presentation must not authorize transport; Player checks the current range.
        let visible = identity.is_some() && (seekable || (!source_changed && self.visible));
        let duration = duration.or(if source_changed { None } else { self.duration });
        if source_changed
            || self.selected != selected
            || self.duration != duration
            || self.visible != visible
        {
            self.display = if visible {
                selected.within(duration)
            } else {
                List::default()
            };
            self.selected = selected;
            self.duration = duration;
            self.visible = visible;
        }
        self.current = match position {
            Some(position) => self.display.current(position),
            None if !source_changed && before == self.display => before_current,
            None => None,
        };
        if before != self.display || before_current != self.current {
            self.revision = self.revision.wrapping_add(1);
        }
    }
    fn poll(&mut self) {
        self.work = match std::mem::take(&mut self.work) {
            Work::Reading(job) if job.thread.is_finished() => {
                if let Some(list) = job.finish() {
                    self.fallback = list;
                }
                Work::Idle
            }
            Work::Retiring(job) if job.thread.is_finished() => {
                job.finish();
                Work::Idle
            }
            state => state,
        };
        if matches!(self.work, Work::Idle)
            && let Some(file) = self.pending.take()
        {
            let cancelled = Arc::new(AtomicBool::new(false));
            let cancellation = cancelled.clone();
            match std::thread::Builder::new()
                .name("recording-chapters".into())
                .spawn(move || {
                    let mut reader = file.source().reader(cancellation.clone());
                    mp4::discover(&mut reader, file.size(), &cancellation)
                }) {
                Ok(thread) => self.work = Work::Reading(Job { cancelled, thread }),
                Err(error) => tracing::warn!(
                    operation = "read recording chapters",
                    error = &error as &dyn std::error::Error,
                    "Could not start chapter discovery"
                ),
            }
        }
    }
    #[cfg(test)]
    pub(crate) fn busy(&self) -> bool {
        !matches!(self.work, Work::Idle) || self.pending.is_some()
    }
}
impl Drop for Controller {
    fn drop(&mut self) {
        if let Work::Reading(job) | Work::Retiring(job) = std::mem::take(&mut self.work) {
            job.cancel();
            job.finish();
        }
    }
}
