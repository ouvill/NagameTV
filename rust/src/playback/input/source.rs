//! appsrc owns the GStreamer streaming boundary; the receiver owns raw retention.
use super::activity::{Activity, Change, Interest};
use super::*;
use crate::features::subscriptions::Subscriptions;
use gstreamer::{self as gst, prelude::*};
use gstreamer_app::{AppSrc, AppSrcCallbacks, AppStreamType};
use std::sync::atomic::AtomicU64;
#[cfg(test)]
mod wait_tests;

pub(super) struct Feedback {
    pub failure: Mutex<Option<String>>,
    generation: AtomicU64,
    expired: AtomicU64,
    progress: Mutex<ReadProgress>,
    interrupted: AtomicBool,
    activity: Activity,
}

/// Receive-edge waits belong to one seek generation. A wait alone says nothing
/// about data already queued in the decoder; filtered TS blocks are not waits.
#[derive(Clone, Copy)]
pub(in crate::playback) struct ReadProgress {
    generation: u64,
    waits: u64,
}
impl ReadProgress {
    pub(in crate::playback) fn waited_since(self, previous: Self) -> Option<bool> {
        (self.generation == previous.generation).then_some(self.waits != previous.waits)
    }
    #[cfg(test)]
    pub(in crate::playback) fn idle() -> Self {
        Self::for_test(0, 0)
    }
    #[cfg(test)]
    pub(in crate::playback) fn for_test(generation: u64, waits: u64) -> Self {
        Self { generation, waits }
    }
}
const NO_EXPIRED_GENERATION: u64 = u64::MAX;
impl Default for Feedback {
    fn default() -> Self {
        Self {
            failure: Mutex::default(),
            generation: AtomicU64::new(0),
            expired: AtomicU64::new(NO_EXPIRED_GENERATION),
            progress: Mutex::new(ReadProgress {
                generation: 0,
                waits: 0,
            }),
            interrupted: AtomicBool::new(false),
            activity: Activity::default(),
        }
    }
}
impl Feedback {
    fn begin_seek(&self) {
        let generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        if let Ok(mut progress) = self.progress.lock() {
            *progress = ReadProgress {
                generation,
                waits: 0,
            };
        }
        self.activity.notify(Change::Control);
    }
    fn awaited(&self, generation: u64) {
        if let Ok(mut progress) = self.progress.lock()
            && progress.generation == generation
        {
            progress.waits = progress.waits.wrapping_add(1);
        }
    }
    pub(super) fn read_progress(&self) -> Result<ReadProgress, Error> {
        self.progress
            .lock()
            .map(|value| *value)
            .map_err(|_| Error::Poisoned)
    }
    pub(super) fn take_expired(&self) -> bool {
        self.expired.swap(NO_EXPIRED_GENERATION, Ordering::AcqRel)
            == self.generation.load(Ordering::Acquire)
    }
}

// Keep the reader and all ways of interrupting its wait together. Callers
// cannot change a standalone cancellation flag without waking that reader.
#[derive(Clone)]
pub(super) struct Feeder {
    reader: Arc<Mutex<Reader>>,
    broadcast_change: Arc<Mutex<BroadcastChange>>,
    pub(super) feedback: Arc<Feedback>,
}

#[derive(Default)]
enum BroadcastChange {
    #[default]
    Unchanged,
    Attach(data_broadcast::Tap),
    Detach,
}

impl Feeder {
    pub fn new(reader: Reader) -> Result<Self, Error> {
        let activity = match &reader.shared {
            Shared::Live(store) => store.lock().map_err(|_| Error::Poisoned)?.activity(),
            Shared::File(_) => Activity::default(),
        };
        Ok(Self {
            reader: Arc::new(Mutex::new(reader)),
            broadcast_change: Arc::default(),
            feedback: Arc::new(Feedback {
                activity,
                ..Feedback::default()
            }),
        })
    }

    fn set_data_broadcast(&self, tap: Option<data_broadcast::Tap>) -> Result<(), Error> {
        // The reader can hold its mutex during a recording URL read. Queue
        // only the latest subscription so a UI toggle never waits on that I/O.
        *self.broadcast_change.lock().map_err(|_| Error::Poisoned)? = match tap {
            Some(tap) => BroadcastChange::Attach(tap),
            None => BroadcastChange::Detach,
        };
        self.feedback.activity.notify(Change::Control);
        Ok(())
    }

    pub fn suspend(&self, suspended: bool) {
        self.feedback
            .interrupted
            .store(suspended, Ordering::Release);
        self.feedback.activity.notify(Change::Control);
    }

    fn cancelled(&self, generation: u64) -> bool {
        self.feedback.interrupted.load(Ordering::Acquire)
            || generation != self.feedback.generation.load(Ordering::Acquire)
    }

    fn next(
        &self,
        generation: u64,
        requests: &Mutex<Option<u64>>,
    ) -> Result<Option<gst::Buffer>, Error> {
        // Filtering consumes at most READ_BYTES each iteration. Yield after a
        // bounded burst of real work, never spin on an unchanged empty input.
        const FILTERED_YIELD_BYTES: usize = READ_BYTES * 16;
        let mut filtered_bytes = 0;
        loop {
            let observed = self.feedback.activity.observe();
            if self.cancelled(generation) {
                return Err(Error::Cancelled);
            }
            let result = self
                .reader
                .lock()
                .map_err(|_| Error::Poisoned)
                .and_then(|mut reader| {
                    let change = std::mem::take(
                        &mut *self.broadcast_change.lock().map_err(|_| Error::Poisoned)?,
                    );
                    match change {
                        BroadcastChange::Unchanged => {}
                        BroadcastChange::Attach(tap) => reader.data_broadcast = Some(tap),
                        BroadcastChange::Detach => reader.data_broadcast = None,
                    }
                    if let Some(target) = requests.lock().map_err(|_| Error::Poisoned)?.take()
                        && !(target == 0 && reader.offset == reader.framing.offset())
                    {
                        reader.prepare(target)?.execute()?;
                    }
                    reader.next_with_cancel(|| self.cancelled(generation))
                });
            if self.cancelled(generation) {
                return Err(Error::Cancelled);
            }
            match result {
                Ok(Output::Data {
                    bytes,
                    time_ns,
                    discontinuity,
                }) => {
                    if bytes.is_empty() {
                        return Err(Error::NoReadProgress);
                    }
                    let mut buffer = gst::Buffer::from_mut_slice(bytes);
                    let writable = buffer.get_mut().expect("new buffer");
                    if discontinuity {
                        writable.set_flags(gst::BufferFlags::DISCONT);
                    }
                    writable.set_dts(gst::ClockTime::from_nseconds(time_ns));
                    writable.set_pts(gst::ClockTime::from_nseconds(time_ns));
                    return Ok(Some(buffer));
                }
                Ok(Output::Expired) | Err(Error::Expired) => {
                    self.feedback.expired.store(generation, Ordering::Release);
                    // New TS cannot repair an expired read position. Wait for
                    // a seek, terminal input state or cancellation, not arrivals.
                    observed.wait(Interest::Control);
                }
                Ok(Output::Awaiting) => {
                    self.feedback.awaited(generation);
                    observed.wait(Interest::Any);
                }
                Ok(Output::Filtered { consumed }) => {
                    filtered_bytes += consumed.get();
                    if filtered_bytes >= FILTERED_YIELD_BYTES {
                        std::thread::yield_now();
                        filtered_bytes = 0;
                    }
                }
                Ok(Output::End) => return Ok(None),
                Err(error) => return Err(error),
            }
        }
    }
}

const SOURCE_QUEUE_BYTES: u64 = (READ_BYTES * 2) as u64;
const LIVE_EDGE_TOLERANCE_NS: u64 = 5_000_000_000;
// Allow the bounded metadata scan to finish after playback has started.
// The planner still requires the following event to start within its padding.
const INITIAL_PROGRAM_SELECTION: Duration = Duration::from_secs(120);

pub(in crate::playback) struct Input {
    identity: u64,
    preview: super::preview::Source,
    shared: Shared,
    worker: Worker,
    subscriptions: Subscriptions,
    feeder: Feeder,
    sources: Arc<Mutex<Vec<gst::glib::WeakRef<AppSrc>>>>,
}
impl Input {
    pub fn set_data_broadcast(&mut self, tap: Option<data_broadcast::Tap>) -> Result<(), Error> {
        self.feeder.set_data_broadcast(tap)
    }
    pub fn recording(
        playbin: &gst::Element,
        recording: &crate::playback::recording::TransportStream,
        programs: bool,
    ) -> Result<Self, Error> {
        let (reader, worker) = file_reader_inspected(
            recording.source(),
            recording.service(),
            programs,
            recording.inspection(),
        )?;
        Self::attach(playbin, reader, worker, Some(recording.source().clone()))
    }
    #[cfg(test)]
    pub fn file(
        playbin: &gst::Element,
        path: &Path,
        service: u16,
        programs: bool,
    ) -> Result<Self, Error> {
        let (reader, worker) = file_reader(path, service, programs)?;
        Self::attach(
            playbin,
            reader,
            worker,
            Some(crate::playback::recording::source::Source::Local(
                path.to_owned(),
            )),
        )
    }
    pub fn live(
        playbin: &gst::Element,
        uri: &str,
        service: u16,
        retention: Policy,
        programs: bool,
    ) -> Result<Self, Error> {
        let (reader, worker) = live_reader(uri, service, retention, programs)?;
        Self::attach(playbin, reader, worker, None)
    }
    fn attach(
        playbin: &gst::Element,
        reader: Reader,
        worker: Worker,
        file: Option<crate::playback::recording::source::Source>,
    ) -> Result<Self, Error> {
        let preview = super::preview::Source::new(&reader, file)?;
        let shared = reader.shared.clone();
        let feeder = Feeder::new(reader)?;
        let subscriptions = Subscriptions::default();
        let registrations = subscriptions.clone();
        let installed_feeder = feeder.clone();
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
                configure(&source, installed_feeder.clone(), &registrations);
            }
            None
        });
        subscriptions.signal(playbin, id);
        Ok(Self {
            identity: crate::playback::next_source_identity(),
            preview,
            shared,
            worker,
            subscriptions,
            feeder,
            sources,
        })
    }
    pub fn suspend(&self, suspended: bool) {
        self.feeder.suspend(suspended);
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
        if let Some(error) = &*self
            .feeder
            .feedback
            .failure
            .lock()
            .map_err(|_| Error::Poisoned)?
        {
            return Err(std::io::Error::other(error.clone()).into());
        }
        self.shared.window()?;
        Ok(())
    }
    pub fn begin_pause(&mut self, position_ns: u64) -> Result<bool, Error> {
        match &self.shared {
            Shared::File(_) => Ok(false),
            Shared::Live(shared) => {
                let mut store = shared.lock().map_err(|_| Error::Poisoned)?;
                if store.policy().storage() == Retention::Off {
                    return Err(Error::Unindexed);
                }
                Ok(store.begin_pause(position_ns)?)
            }
        }
    }
    pub fn finish_pause(&mut self) -> Result<(), Error> {
        if let Shared::Live(shared) = &self.shared {
            shared.lock().map_err(|_| Error::Poisoned)?.finish_pause()?;
        }
        Ok(())
    }
    pub fn identity(&self) -> u64 {
        self.identity
    }
    pub fn preview_source(&self) -> super::preview::Source {
        self.preview.clone()
    }
    pub fn latency(&self) -> Result<Option<crate::playback::latency::Tracker>, Error> {
        match &self.shared {
            Shared::Live(store) => Ok(Some(
                store.lock().map_err(|_| Error::Poisoned)?.latency.clone(),
            )),
            Shared::File(_) => Ok(None),
        }
    }
    pub fn comment_source(
        &self,
        fallback: Option<crate::channels::BroadcastService>,
        channel: impl Fn(crate::channels::BroadcastService) -> Option<u16>,
        position: Option<u64>,
        reception: viewer_comments::cache::Reception,
    ) -> Option<viewer_comments::cache::Source> {
        use viewer_comments::cache::{ClockSpan, Program, ProgramId, Recording, Source};
        let window = self.shared.window().ok().flatten()?;
        let catalog = match &self.shared {
            Shared::File(shared) => shared.lock().ok()?.index.catalog(),
            Shared::Live(shared) => shared.lock().ok()?.index.catalog(),
        };
        if let Shared::File(shared) = &self.shared {
            let state = shared.lock().ok()?;
            if matches!(state.discovery, Discovery::Pending) {
                return Some(Source::Recording(Recording::Discovering));
            }
            let position = position.unwrap_or(window.start);
            let view = state.index.view(position);
            let program = |value: &crate::transport::programs::Program| {
                Program::new(
                    ProgramId {
                        network: value.network_id,
                        transport: value.transport_stream_id,
                        service: value.service_id,
                        event: value.event_id,
                    },
                    value.start_at?.div_euclid(1000),
                    i64::try_from(value.duration? / 1000).ok()?,
                )
            };
            return Some(Source::Recording(Recording::Observed {
                current: view.program.as_ref().and_then(program),
                next: view.next.as_ref().and_then(program),
                utc_seconds: view
                    .clock
                    .and_then(|clock| clock.utc(position))
                    .map(|utc| utc.div_euclid(1000)),
                at_start: position.saturating_sub(window.start)
                    < INITIAL_PROGRAM_SELECTION.as_nanos() as u64,
            }));
        }
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
            Shared::File(_) => unreachable!("recording metadata handled above"),
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
        let start = store.window_start().unwrap_or(end);
        let enabled = store.retaining();
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
        if !store.retaining() {
            return Err(Error::Unavailable);
        }
        let start = store.window_start().ok_or(Error::Unavailable)?;
        let end = store.index.end_ns().ok_or(Error::Unavailable)?;
        store.history.seek_target(start, end, milliseconds)
    }
    pub fn take_expired(&self) -> bool {
        self.feeder.feedback.take_expired()
    }
    pub fn read_progress(&self) -> Result<ReadProgress, Error> {
        self.feeder.feedback.read_progress()
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

pub(super) fn configure(source: &AppSrc, feeder: Feeder, registrations: &Subscriptions) {
    source.set_format(gst::Format::Time);
    source.set_stream_type(AppStreamType::Seekable);
    source.set_max_bytes(SOURCE_QUEUE_BYTES);
    source.set_caps(Some(
        &gst::Caps::builder("video/mpegts")
            .field("systemstream", true)
            .field("packetsize", TS_PACKET_SIZE as i32)
            .build(),
    ));
    let shared = feeder
        .reader
        .lock()
        .expect("new source reader")
        .shared
        .clone();
    let seek_shared = shared.clone();
    let requests = Arc::new(Mutex::new(None));
    let seeking = requests.clone();
    let sequence = Arc::new(Mutex::new(None::<gst::Seqnum>));
    let requested = sequence.clone();
    let epoch = feeder.feedback.clone();
    if let Some(pad) = source.static_pad("src") {
        let probe = pad.add_probe(
            gst::PadProbeType::EVENT_UPSTREAM
                | gst::PadProbeType::EVENT_DOWNSTREAM
                | gst::PadProbeType::EVENT_FLUSH
                | gst::PadProbeType::QUERY_UPSTREAM,
            move |_, info| {
                if let Some(event) = info.event_mut() {
                    match event.view() {
                        gst::EventView::Seek(_) => {
                            epoch.begin_seek();
                            if let Ok(mut sequence) = requested.lock() {
                                *sequence = Some(event.seqnum());
                            }
                        }
                        gst::EventView::FlushStart(_) => epoch.begin_seek(),
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

    let seeking_activity = feeder.feedback.activity.clone();
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
                    seeking_activity.notify(Change::Control);
                    Ok(())
                })();
                if let Err(error) = &result {
                    tracing::warn!(%error, "TS seek rejected");
                }
                // Rejection is returned to Controller; it must not destroy playback.

                result.is_ok()
            })
            .need_data(move |source, _| {
                let generation = feeder.feedback.generation.load(Ordering::Acquire);
                let result = feeder.next(generation, &requests);
                if feeder.cancelled(generation) {
                    return;
                }
                let delivered = match result {
                    Ok(Some(buffer)) => source.push_buffer(buffer),
                    Ok(None) => source.end_of_stream(),
                    // Normal cancellation during shutdown or a flushing seek.
                    Err(Error::Cancelled) => return,
                    Err(error) => {
                        tracing::error!(
                            error = &error as &dyn std::error::Error,
                            "Could not read TS input for playback"
                        );
                        if let Ok(mut failure) = feeder.feedback.failure.lock() {
                            *failure = Some(error.to_string());
                        }
                        source.end_of_stream()
                    }
                };
                if let Err(error) = delivered {
                    // GStreamer can flush or end between the generation check
                    // and delivery. These are normal seek/shutdown races.
                    if !matches!(error, gst::FlowError::Flushing | gst::FlowError::Eos) {
                        tracing::error!(%error, "Could not deliver TS input to appsrc");
                        if let Ok(mut failure) = feeder.feedback.failure.lock() {
                            *failure = Some(error.to_string());
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
    fn receive_wait_feedback_does_not_cross_a_seek() {
        let feedback = Feedback::default();
        let before = feedback.read_progress().unwrap();
        feedback.awaited(before.generation);
        let waiting = feedback.read_progress().unwrap();
        assert_eq!(waiting.waited_since(before), Some(true));
        feedback.begin_seek();
        let after = feedback.read_progress().unwrap();
        assert_eq!(after.waited_since(waiting), None);
        feedback.awaited(before.generation);
        assert_eq!(
            feedback.read_progress().unwrap().waited_since(after),
            Some(false)
        );
        feedback.awaited(after.generation);
        assert_eq!(
            feedback.read_progress().unwrap().waited_since(after),
            Some(true)
        );
    }
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
