//! appsrc owns the GStreamer streaming boundary; the receiver owns raw retention.
use super::*;
use crate::features::subscriptions::Subscriptions;
use gstreamer::{self as gst, prelude::*};
use gstreamer_app::{AppSrc, AppSrcCallbacks, AppStreamType};
use std::sync::atomic::AtomicU64;

pub(super) struct Feedback {
    pub failure: Mutex<Option<String>>,
    generation: AtomicU64,
    expired: AtomicU64,
}
const NO_EXPIRED_GENERATION: u64 = u64::MAX;
impl Default for Feedback {
    fn default() -> Self {
        Self {
            failure: Mutex::default(),
            generation: AtomicU64::new(0),
            expired: AtomicU64::new(NO_EXPIRED_GENERATION),
        }
    }
}
impl Feedback {
    pub(super) fn take_expired(&self) -> bool {
        self.expired.swap(NO_EXPIRED_GENERATION, Ordering::AcqRel)
            == self.generation.load(Ordering::Acquire)
    }
}

const SOURCE_QUEUE_BYTES: u64 = (READ_BYTES * 2) as u64;
const LIVE_EDGE_TOLERANCE_NS: u64 = 5_000_000_000;

pub(in crate::playback) struct Input {
    identity: u64,
    shared: Shared,
    worker: Worker,
    subscriptions: Subscriptions,
    interrupted: Arc<AtomicBool>,
    feedback: Arc<Feedback>,
    sources: Arc<Mutex<Vec<gst::glib::WeakRef<AppSrc>>>>,
}
impl Input {
    pub fn file(
        playbin: &gst::Element,
        path: &Path,
        service: u16,
        programs: bool,
    ) -> Result<Self, Error> {
        let (reader, worker) = file_reader(path, service, programs)?;
        Ok(Self::attach(playbin, reader, worker))
    }
    pub fn live(
        playbin: &gst::Element,
        uri: &str,
        service: u16,
        retention: Policy,
        programs: bool,
    ) -> Result<Self, Error> {
        let (reader, worker) = live_reader(uri, service, retention, programs)?;
        Ok(Self::attach(playbin, reader, worker))
    }
    fn attach(playbin: &gst::Element, reader: Reader, worker: Worker) -> Self {
        let shared = reader.shared.clone();
        let interrupted = Arc::new(AtomicBool::new(false));
        let feedback = Arc::new(Feedback::default());
        let subscriptions = Subscriptions::default();
        let registrations = subscriptions.clone();
        let reader = Arc::new(Mutex::new(reader));
        let stopped = interrupted.clone();
        let failed = feedback.clone();
        let sources = Arc::new(Mutex::new(Vec::new()));
        let installed_sources = sources.clone();
        let id = playbin.connect("source-setup", false, move |values| {
            if let Some(source) = values
                .get(1)
                .and_then(|value| value.get::<gst::Element>().ok())
                .and_then(|element| element.downcast::<AppSrc>().ok())
            {
                if let Ok(mut sources) = installed_sources.lock() {
                    sources
                        .retain(|source: &gst::glib::WeakRef<AppSrc>| source.upgrade().is_some());
                    sources.push(source.downgrade());
                }
                configure(
                    &source,
                    reader.clone(),
                    stopped.clone(),
                    failed.clone(),
                    &registrations,
                );
            }
            None
        });
        subscriptions.signal(playbin, id);
        static NEXT_SOURCE: AtomicU64 = AtomicU64::new(1);
        Self {
            identity: NEXT_SOURCE.fetch_add(1, Ordering::Relaxed),
            shared,
            worker,
            subscriptions,
            interrupted,
            feedback,
            sources,
        }
    }
    pub fn suspend(&self, suspended: bool) {
        self.interrupted.store(suspended, Ordering::Release);
    }
    pub fn reconfigure(
        &mut self,
        policy: Policy,
    ) -> Result<Option<super::super::timeline::RetentionChange>, Error> {
        let Shared::Live(shared) = &self.shared else {
            return Err(Error::Unindexed);
        };
        let mut store = shared.lock().map_err(|_| Error::Poisoned)?;
        Ok(store.prepare(policy)?.commit()?)
    }
    pub fn check(&self) -> Result<(), Error> {
        if let Some(error) = &*self.feedback.failure.lock().map_err(|_| Error::Poisoned)? {
            return Err(std::io::Error::other(error.clone()).into());
        }
        self.shared.window()?;
        Ok(())
    }
    pub fn identity(&self) -> u64 {
        self.identity
    }
    pub fn comment_source(
        &self,
        fallback: Option<crate::channels::BroadcastService>,
        channel: impl Fn(crate::channels::BroadcastService) -> Option<u16>,
        position: Option<u64>,
        reception: viewer_comments::cache::Reception,
    ) -> Option<viewer_comments::cache::Source> {
        use viewer_comments::cache::{ClockSpan, RecordingRange, Source};
        let window = self.shared.window().ok().flatten()?;
        let catalog = match &self.shared {
            Shared::File(shared) => shared.lock().ok()?.index.catalog(),
            Shared::Live(shared) => shared.lock().ok()?.index.catalog(),
        };
        let mapped_start = match &self.shared {
            Shared::Live(_) => window
                .start
                .saturating_sub(viewer_comments::danmaku::MAX_LIFETIME.as_nanos() as u64),
            Shared::File(_) => window.start,
        };
        let mapped = catalog
            .lock()
            .ok()?
            .broadcast_spans(mapped_start, window.end);
        let spans: Vec<_> = mapped
            .iter()
            .filter_map(|span| {
                let channel = channel(span.service.or(fallback)?)?;
                let (start, end) = span.clock.range();
                Some(ClockSpan {
                    key: span.clock.key(),
                    channel,
                    media_start_ms: i64::try_from(start / 1_000_000).ok()?,
                    media_end_ms: i64::try_from(end / 1_000_000).ok()?,
                    utc_start_ms: span.clock.utc_range().0,
                })
            })
            .collect();
        match &self.shared {
            Shared::File(_) => {
                let mut through = window.start;
                let complete = mapped.len() == spans.len()
                    && mapped.iter().all(|span| {
                        let (start, end) = span.clock.range();
                        let contiguous = start <= through;
                        through = through.max(end);
                        contiguous
                    })
                    && through >= window.end
                    && matches!(window.coverage, Coverage::Complete);
                Some(Source::Recording(if complete {
                    RecordingRange::Known(spans)
                } else {
                    RecordingRange::Discovering(spans)
                }))
            }
            Shared::Live(_) => Some(Source::Live {
                earliest_media_ms: i64::try_from(window.start / 1_000_000).ok()?,
                spans,
                reception,
                at_edge: position.is_none_or(|position| {
                    window.end.saturating_sub(position) <= LIVE_EDGE_TOLERANCE_NS
                }),
            }),
        }
    }
    pub fn metadata(&self, position_ns: u64) -> crate::transport::programs::catalog::View {
        match &self.shared {
            Shared::File(shared) => shared
                .lock()
                .map(|state| {
                    let mut view = state.index.view(position_ns);
                    if view.status == crate::transport::programs::catalog::Status::Pending {
                        view.status = match &state.status {
                            Status::Ended => {
                                crate::transport::programs::catalog::Status::Unavailable
                            }
                            Status::Failed(_) => {
                                crate::transport::programs::catalog::Status::Failed
                            }
                            Status::Receiving => view.status,
                        };
                    }
                    view
                })
                .unwrap_or_default(),
            Shared::Live(shared) => shared
                .lock()
                .map(|state| state.index.view(position_ns))
                .unwrap_or_default(),
        }
    }
    #[cfg(test)]
    pub(super) fn index_lifetime(&self) -> Option<std::sync::Weak<Mutex<FileIndex>>> {
        match &self.shared {
            Shared::File(index) => Some(Arc::downgrade(index)),
            Shared::Live(_) => None,
        }
    }
    pub fn duration_estimated(&self) -> bool {
        match &self.shared {
            Shared::File(shared) => shared.lock().is_ok_and(|state| {
                state.survey.is_some() && !matches!(state.status, Status::Ended)
            }),
            Shared::Live(_) => false,
        }
    }
    pub fn bytes_per_second(&self) -> Option<f64> {
        match &self.shared {
            Shared::Live(store) => store.lock().ok()?.index.bytes_per_second(),
            Shared::File(_) => None,
        }
    }
    pub fn live_timeline(
        &self,
        presenter: &mut super::super::live_timeline::Presenter,
        phase: super::super::timeline::Phase,
        position: Option<gst::ClockTime>,
        target: Option<gst::ClockTime>,
    ) -> Option<super::super::live_timeline::Snapshot> {
        let Shared::Live(shared) = &self.shared else {
            return None;
        };
        let mut store = shared.lock().ok()?;
        let end = store.index.end_ns().unwrap_or(0);
        let start = store
            .index
            .entries()
            .front()
            .map_or(end, |anchor| anchor.time_ns);
        let enabled = store.policy().storage() != Retention::Off;
        Some(presenter.project(
            &mut store.history,
            start,
            end,
            enabled,
            super::super::live_timeline::Reading {
                phase,
                position_ns: position.map(|time| time.nseconds()),
                target_ns: target.map(|time| time.nseconds()),
            },
        ))
    }
    pub fn live_seek_target(
        &self,
        milliseconds: f64,
    ) -> Result<super::super::live_timeline::SeekTarget, super::super::timeline::Error> {
        use super::super::timeline::Error;
        let Shared::Live(shared) = &self.shared else {
            return Err(Error::Unavailable);
        };
        let store = shared.lock().map_err(|_| Error::Unavailable)?;
        if store.policy().storage() == Retention::Off {
            return Err(Error::Unavailable);
        }
        let start = store
            .index
            .entries()
            .front()
            .ok_or(Error::Unavailable)?
            .time_ns;
        let end = store.index.end_ns().ok_or(Error::Unavailable)?;
        store.history.seek_target(start, end, milliseconds)
    }
    pub fn take_expired(&self) -> bool {
        self.feedback.take_expired()
    }
    pub fn live_window(&self) -> Option<super::super::timeline::LiveWindow> {
        use super::super::timeline::{LiveWindow, Range};
        if let Shared::File(_) = self.shared {
            return None;
        }
        let window = self.shared.window().ok()??;
        let range = Range::new(
            gst::ClockTime::from_nseconds(window.start),
            gst::ClockTime::from_nseconds(window.end),
        )?;
        Some(if window.seekable {
            LiveWindow::History(range)
        } else {
            LiveWindow::ForwardBuffer(range)
        })
    }
}
impl Drop for Input {
    fn drop(&mut self) {
        self.suspend(true);
        match &self.worker {
            Worker::File(flag) => flag.0.store(true, Ordering::Release),
            Worker::Live(task) => task.abort(),
        }
        // READY can retain playbin's old appsrc. Release its closures explicitly
        // so stopped readers cannot keep the raw store or disk directory alive.
        if let Ok(mut sources) = self.sources.lock() {
            for source in sources.drain(..).filter_map(|source| source.upgrade()) {
                source.set_callbacks(AppSrcCallbacks::builder().build());
            }
        }
        self.subscriptions.close();
    }
}

pub(super) fn configure(
    source: &AppSrc,
    reader: Arc<Mutex<Reader>>,
    interrupted: Arc<AtomicBool>,
    feedback: Arc<Feedback>,
    registrations: &Subscriptions,
) {
    source.set_format(gst::Format::Time);
    source.set_stream_type(AppStreamType::Seekable);
    source.set_max_bytes(SOURCE_QUEUE_BYTES);
    source.set_caps(Some(
        &gst::Caps::builder("video/mpegts")
            .field("systemstream", true)
            .field("packetsize", TS_PACKET_SIZE as i32)
            .build(),
    ));
    let shared = reader.lock().expect("new source reader").shared.clone();
    let seek_shared = shared.clone();
    let requests = Arc::new(Mutex::new(None));
    let seeking = requests.clone();
    let sequence = Arc::new(Mutex::new(None::<gst::Seqnum>));
    let requested = sequence.clone();
    let epoch = feedback.clone();
    if let Some(pad) = source.static_pad("src") {
        let probe = pad.add_probe(
            gst::PadProbeType::EVENT_UPSTREAM
                | gst::PadProbeType::EVENT_DOWNSTREAM
                | gst::PadProbeType::QUERY_UPSTREAM,
            move |_, info| {
                if let Some(event) = info.event_mut() {
                    match event.view() {
                        gst::EventView::Seek(_) => {
                            epoch.generation.fetch_add(1, Ordering::AcqRel);
                            if let Ok(mut sequence) = requested.lock() {
                                *sequence = Some(event.seqnum());
                            }
                        }
                        gst::EventView::Segment(_) => {
                            // appsrc creates a fresh segment seqnum. Preserve the
                            // originating seek identity through demux and preroll.
                            if let Some(sequence) = requested.lock().ok().and_then(|value| *value) {
                                event.make_mut().set_seqnum(sequence);
                            }
                        }
                        _ => {}
                    }
                }
                if let Some(query) = info.query_mut() {
                    let window = shared.window().ok().flatten();
                    match query.view_mut() {
                        gst::QueryViewMut::Seeking(query)
                            if query.format() == gst::Format::Time =>
                        {
                            query.set(
                                window.is_some_and(|window| window.seekable),
                                gst::ClockTime::from_nseconds(
                                    window.map_or(0, |value| value.start),
                                ),
                                window.map(|value| gst::ClockTime::from_nseconds(value.end)),
                            );
                            return gst::PadProbeReturn::Handled;
                        }
                        gst::QueryViewMut::Duration(query)
                            if query.format() == gst::Format::Time =>
                        {
                            query.set(
                                window
                                    .filter(|value| {
                                        matches!(
                                            value.coverage,
                                            Coverage::Complete | Coverage::Estimated
                                        )
                                    })
                                    .map(|value| gst::ClockTime::from_nseconds(value.end)),
                            );
                            return gst::PadProbeReturn::Handled;
                        }
                        _ => {}
                    }
                }
                gst::PadProbeReturn::Ok
            },
        );
        registrations.probe(&pad, probe);
    }

    source.set_callbacks(
        AppSrcCallbacks::builder()
            .seek_data(move |_, target| {
                let result = (|| -> Result<(), Error> {
                    if target != 0
                        && !seek_shared
                            .window()?
                            .is_some_and(|window| target >= window.start && target < window.end)
                    {
                        return Err(Error::Expired);
                    }
                    // No disk access and no reader lock on the GUI seek path.
                    *seeking.lock().map_err(|_| Error::Poisoned)? = Some(target);
                    Ok(())
                })();
                if let Err(error) = &result {
                    tracing::warn!(%error, "TS seek rejected");
                }
                // Rejection is returned to Controller; it must not destroy playback.

                result.is_ok()
            })
            .need_data(move |source, _| {
                let current = feedback.generation.load(Ordering::Acquire);
                while !interrupted.load(Ordering::Acquire)
                    && current == feedback.generation.load(Ordering::Acquire)
                {
                    let result =
                        reader
                            .lock()
                            .map_err(|_| Error::Poisoned)
                            .and_then(|mut reader| {
                                if let Some(target) =
                                    requests.lock().map_err(|_| Error::Poisoned)?.take()
                                    && !(target == 0 && reader.offset == reader.framing.offset())
                                {
                                    reader.prepare(target)?.execute()?;
                                }
                                reader.next_with_cancel(|| {
                                    interrupted.load(Ordering::Acquire)
                                        || current != feedback.generation.load(Ordering::Acquire)
                                })
                            });
                    if interrupted.load(Ordering::Acquire)
                        || current != feedback.generation.load(Ordering::Acquire)
                    {
                        return;
                    }
                    match result {
                        Ok(Output::Data {
                            bytes,
                            time_ns,
                            discontinuity,
                        }) => {
                            if bytes.is_empty() {
                                continue;
                            }
                            let mut buffer = gst::Buffer::from_mut_slice(bytes);
                            let writable = buffer.get_mut().expect("new buffer");
                            if discontinuity {
                                writable.set_flags(gst::BufferFlags::DISCONT);
                            }
                            writable.set_dts(gst::ClockTime::from_nseconds(time_ns));
                            writable.set_pts(gst::ClockTime::from_nseconds(time_ns));
                            if current == feedback.generation.load(Ordering::Acquire) {
                                let _ = source.push_buffer(buffer);
                            }
                            return;
                        }
                        Ok(Output::Expired) | Err(Error::Expired) => {
                            // A delayed seek may expire before the reader accepts
                            // it. Recover in this generation, never fail playback or
                            // carry an old reader's feedback into a new seek.
                            feedback.expired.store(current, Ordering::Release);
                            std::thread::sleep(INPUT_WAIT);
                        }
                        Ok(Output::Awaiting) => {
                            // An expired paused cursor waits for the UI controller to
                            // flush and move to the new window; never emit stale data.
                            std::thread::sleep(INPUT_WAIT);
                        }
                        Ok(Output::End) => {
                            let _ = source.end_of_stream();
                            return;
                        }
                        Err(error) => {
                            if let Ok(mut failure) = feedback.failure.lock() {
                                *failure = Some(error.to_string());
                            }
                            let _ = source.end_of_stream();
                            return;
                        }
                    }
                }
            })
            .build(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expired_feedback_cannot_cross_a_seek_generation() {
        let feedback = Feedback::default();
        assert!(!feedback.take_expired());
        let old = feedback.generation.load(Ordering::Acquire);
        feedback.expired.store(old, Ordering::Release);
        assert!(feedback.take_expired());
        assert!(!feedback.take_expired());
        let next = feedback.generation.fetch_add(1, Ordering::AcqRel) + 1;
        // The previous streaming callback can publish just after the GUI seeks.
        feedback.expired.store(old, Ordering::Release);
        assert!(!feedback.take_expired());
        feedback.expired.store(next, Ordering::Release);
        assert!(feedback.take_expired());
    }
}
