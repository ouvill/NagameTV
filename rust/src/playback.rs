mod audio_choices;
mod audio_components;
mod audio_default;
pub mod audio_output;
mod audio_routing;
mod audio_sink;
pub mod audio_streams;
mod clock;
pub mod deinterlace;
pub mod failure;
pub mod input;
pub(crate) mod live_timeline;
pub mod recording;
pub mod speed;
pub mod stats;
pub mod timeline;
#[cfg(feature = "video_item_tests")]
pub(crate) mod video_item_checks;
mod video_output;
pub mod warnings;

use gstreamer::{self as gst, prelude::*};
use std::cell::{Cell, RefCell};
use std::sync::{Arc, Mutex, OnceLock, Weak};

type Result<T> = std::result::Result<T, Error>;

pub enum Event {
    Idle,
    Playing,
    Ended(gst::Seqnum),
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{primary}（停止処理も失敗: {cleanup}）")]
    Cleanup {
        #[source]
        primary: Box<Error>,
        cleanup: Box<Error>,
    },
    #[error("{0}")]
    Deinterlace(#[from] deinterlace::Error),
    #[error("{0}")]
    MediaSubtitle(#[from] crate::media_subtitles::Error),
    #[error("Video output: {0}")]
    VideoOutput(String),
    #[error("{0}")]
    AudioSink(#[from] audio_sink::Error),
    #[error("{0}")]
    Clock(#[from] clock::Error),
    #[error("GStreamer initialization failed: {0}")]
    Initialization(#[from] gst::glib::Error),
    #[error("GStreamer operation failed: {0}")]
    Operation(#[from] gst::glib::BoolError),
    #[error("GStreamer state change failed: {0}")]
    StateChange(#[from] gst::StateChangeError),
    #[error("Playback already initialized")]
    AlreadyInitialized,
    #[error("Playback unavailable")]
    Unavailable,
    #[error("Missing video sink pad")]
    MissingSinkPad,
    #[error("Missing Qt video item")]
    MissingVideoItem,
    #[error("Video output requires a GStreamer video item on the GUI thread")]
    InvalidVideoItem,
    #[error("Video output is already attached")]
    OutputAlreadyAttached,
    #[error("Video output has been shut down")]
    OutputShutDown,
    #[error("Video output is not ready")]
    OutputNotReady,
    #[error("Missing GStreamer bus")]
    MissingBus,
    #[error("Missing video or audio decoder: {0}")]
    MissingDecoder(String),
    #[error("配信が終了しました")]
    EndOfStream,
    #[error("{0}")]
    Recording(#[from] recording::Error),
    #[error("{0}")]
    Input(#[from] input::Error),
    #[error("{0}")]
    Transport(#[from] timeline::Error),
    #[error("{source} ({debug:?})")]
    LiveResumeRejected {
        source: gst::glib::Error,
        debug: Option<String>,
    },
    #[error("{source} (HTTP: {http_status:?}, debug: {debug:?})")]
    Stream {
        http_status: Option<u32>,
        network_source: bool,
        source: gst::glib::Error,
        debug: Option<String>,
    },
}

impl Error {
    pub fn is_live_resume_rejected(&self) -> bool {
        matches!(self, Self::LiveResumeRejected { .. })
    }
}

fn stream_error(message: &gst::message::Error) -> Error {
    let source = message.error();
    let debug = message.debug().map(|s| s.to_string());
    if source.matches(gst::ResourceError::Seek)
        && message
            .src()
            .and_then(|s| s.downcast_ref::<gst::Element>())
            .and_then(|s| s.factory())
            .is_some_and(|f| f.name() == "souphttpsrc")
    {
        Error::LiveResumeRejected { source, debug }
    } else {
        Error::Stream {
            http_status: message
                .details()
                .and_then(|details| details.get::<u32>("http-status-code").ok()),
            network_source: message
                .src()
                .and_then(|src| src.downcast_ref::<gst::Element>())
                .and_then(|element| element.factory())
                .is_some_and(|factory| factory.name() == "souphttpsrc"),
            source,
            debug,
        }
    }
}

// GStreamer's documented missing-plugin message carries the unhandled caps.
// ARIB/private data and subtitle decoders are optional; missing A/V is a failure,
// including when playbin could otherwise continue with only the remaining track.
fn missing_decoder(message: &gst::MessageRef) -> Option<Error> {
    let structure = message.structure()?;
    if structure.name() != "missing-plugin" || structure.get::<&str>("type").ok()? != "decoder" {
        return None;
    }
    let caps = structure.get::<gst::Caps>("detail").ok()?;
    let name = caps.structure(0)?.name();
    (name.starts_with("video/") || name.starts_with("audio/")).then(|| {
        Error::MissingDecoder(
            structure
                .get::<String>("name")
                .unwrap_or_else(|_| caps.to_string()),
        )
    })
}

static PRELOADED: OnceLock<Weak<Mutex<Option<Playback>>>> = OnceLock::new();
mod media;
mod resources;
mod session;
pub use session::{Session, SubtitleStart};

/// Own the unclaimed playback until QML takes it, including failed UI creation.
/// Declare after QGuiApplication and before QQmlApplicationEngine so native
/// playback destruction always precedes application destruction.
#[must_use = "Keep the preload owner alive until the QML engine is destroyed"]
pub struct Preloaded(Arc<Mutex<Option<Playback>>>);

pub fn preload() -> Result<Preloaded> {
    let preloaded = Preloaded(Arc::new(Mutex::new(Some(Playback::new()?))));
    PRELOADED
        .set(Arc::downgrade(&preloaded.0))
        .map_err(|_| Error::AlreadyInitialized)?;
    Ok(preloaded)
}

pub fn take_preloaded() -> Option<Playback> {
    let slot = PRELOADED.get()?.upgrade()?;
    slot.lock().ok()?.take()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VideoOutputState {
    Unattached,
    Attached,
    Closing,
    Closed,
}

pub struct Playback {
    playbin: gst::Element,
    sink: gst::Element,
    processor: gst::Element,
    queue: gst::Element,
    mode: deinterlace::Mode,
    video_output: VideoOutputState,
    audio_streams: RefCell<audio_streams::Streams>,
    routing: audio_routing::Routing,
    tempo: Option<gst::Element>,
    audio_intent: RefCell<Option<audio_choices::Intent>>,
    audio_default: RefCell<audio_default::Policy>,
    requested_uri: RefCell<Option<String>>,
    warnings: Cell<warnings::Counts>,
    program_number: Arc<std::sync::atomic::AtomicI32>,
    presentation: crate::screenshots::native::Presentation,
}

impl Playback {
    pub fn presentation(&self) -> &crate::screenshots::native::Presentation {
        &self.presentation
    }
    fn element(&self) -> &gst::Element {
        &self.playbin
    }
    pub fn video_stats(&self) -> stats::VideoStats {
        stats::snapshot(
            &self.playbin,
            &self.processor,
            &self.queue,
            &self.sink,
            self.mode.label(),
        )
    }
    pub fn video_aspect_ratio(&self) -> Option<f64> {
        if !matches!(
            self.sink.current_state(),
            gst::State::Paused | gst::State::Playing
        ) {
            return None;
        }
        let caps = self.sink.static_pad("sink")?.current_caps()?;
        let info = gstreamer_video::VideoInfo::from_caps(&caps).ok()?;
        let par = info.par();
        let ratio = f64::from(info.width()) * f64::from(par.numer())
            / (f64::from(info.height()) * f64::from(par.denom()));
        (ratio.is_finite() && ratio > 0.0).then_some(ratio)
    }
    pub fn position(&self) -> Option<gst::ClockTime> {
        self.sink.query_position::<gst::ClockTime>()
    }
    pub fn video_frame_counters(&self) -> Option<stats::FrameCounters> {
        stats::frame_counters(&self.sink)
    }
    pub fn warning_counts(&self) -> warnings::Counts {
        self.warnings.get()
    }
    fn new() -> Result<Self> {
        gst::init()?;
        let sink = gst::ElementFactory::make("qml6glsink")
            .property("enable-last-sample", false)
            .build()?;
        let presentation = crate::screenshots::native::Presentation::default();
        presentation.install(&sink.static_pad("sink").ok_or(Error::MissingSinkPad)?);
        let mode = deinterlace::Mode::from_environment()?;
        let video_output::Output {
            bin: output,
            processor,
            queue,
        } = video_output::Validated::new(sink.clone(), mode)?.build()?;
        let audio = audio_sink::Output::from_environment()?.build()?;
        let routing = audio_routing::Routing::default();
        let tempo = gst::ElementFactory::find("scaletempo")
            .map(|factory| factory.create().build())
            .transpose()?;
        let audio_filter = routing.filter_with_tempo(tempo.as_ref())?;
        let playbin = clock::Policy::from_environment()?.build_playbin()?;
        let program_number = Arc::new(std::sync::atomic::AtomicI32::new(-1));
        let program = program_number.clone();
        let decoder_policy = resources::DecoderPolicy::new();
        playbin.connect("element-setup", false, move |values| {
            if let Some(element) = values
                .get(1)
                .and_then(|value| value.get::<gst::Element>().ok())
            {
                decoder_policy.configure(&element);
                if element
                    .factory()
                    .is_some_and(|factory| factory.name() == "tsdemux")
                {
                    element.set_property(
                        "program-number",
                        program.load(std::sync::atomic::Ordering::Relaxed),
                    );
                }
            }
            None
        });
        playbin.set_property("audio-filter", &audio_filter);
        playbin.set_property_from_str("flags", "video+audio+soft-volume+buffering+native-video");
        playbin.set_property("video-sink", &output);
        if let Some(audio) = audio {
            playbin.set_property("audio-sink", &audio);
        }
        playbin.set_property("volume", 0.5_f64);
        playbin.connect("source-setup", false, |values| {
            if let Some(source) = values.get(1).and_then(|v| v.get::<gst::Element>().ok())
                && source.factory().is_some_and(|f| f.name() == "souphttpsrc")
            {
                source.set_property("timeout", 15_u32);
                source.set_property("retries", 0_i32);
            }
            None
        });
        Ok(Self {
            playbin,
            sink,
            processor,
            queue,
            mode,
            video_output: VideoOutputState::Unattached,
            audio_streams: RefCell::default(),
            routing,
            tempo,
            audio_intent: RefCell::default(),
            audio_default: RefCell::default(),
            requested_uri: RefCell::new(None),
            warnings: Cell::default(),
            program_number,
            presentation,
        })
    }

    /// Validate and attach a QML video item to qml6glsink.
    ///
    /// # Safety
    /// Call on the GUI thread with null or a live GUI-thread QQuickItem. Keep it
    /// alive through this call and, on success, until shutdown stops the sink.
    /// Qt meta-objects must identify their real native classes, and the video
    /// type must belong to the installed GStreamer plugin. No ownership transfers.
    unsafe fn attach(&mut self, item: *mut crate::qt::ffi::QQuickItem) -> Result<()> {
        // qml6glsink's widget setter shares its interface pointer with the
        // streaming thread without locking. Never replace an active binding.
        match self.video_output {
            VideoOutputState::Attached => return Err(Error::OutputAlreadyAttached),
            VideoOutputState::Closing | VideoOutputState::Closed => {
                return Err(Error::OutputShutDown);
            }
            VideoOutputState::Unattached => {}
        }
        if item.is_null() {
            return Err(Error::MissingVideoItem);
        }
        // SAFETY: The caller guarantees a live item. The helper checks thread
        // affinity and meta-casts to the exact base type required by qml6glsink.
        let widget = unsafe { crate::qt::ffi::qml6_video_item_pointer(item) };
        if widget.is_null() {
            return Err(Error::InvalidVideoItem);
        }
        // The checked pointer is used immediately, on the same GUI thread,
        // without processing events or retaining a Rust pointer to the item.
        self.sink
            .set_property("widget", widget.cast::<std::ffi::c_void>());
        // SAFETY: the same live GUI item validated above. Connections own an
        // Arc-backed observer and are disconnected with the item's lifetime.
        unsafe {
            crate::screenshots::native::ffi::observePresentation(
                item,
                self.presentation.observer(),
            );
        }
        self.video_output = VideoOutputState::Attached;
        Ok(())
    }

    /// Returns false when this stream is already connecting or playing.
    fn play(&self, uri: &str, service: Option<u16>) -> Result<bool> {
        if self.video_output != VideoOutputState::Attached {
            return Err(Error::OutputNotReady);
        }
        if self.requested_uri.borrow().as_deref() == Some(uri) {
            return Ok(false);
        }
        self.stop()?;
        *self.audio_streams.borrow_mut() = audio_streams::Streams::for_service(service);
        self.program_number.store(
            service.map(i32::from).unwrap_or(-1),
            std::sync::atomic::Ordering::Relaxed,
        );
        // Qt must supply the GL display before any other GL element starts.
        self.sink.set_state(gst::State::Ready)?;
        // video-sink is still only a playsink property at this point: its bin
        // may not yet be parented into playbin. Forward Qt's display explicitly
        // so a hardware decoder cannot create and propagate a different one
        // before playsink links the output bin (the CPU path masked this).
        let display = self.sink.context("gst.gl.GLDisplay").ok_or_else(|| {
            Error::VideoOutput("qml6glsink did not provide Qt's GL display context".into())
        })?;
        self.playbin.set_context(&display);
        if self.mode == deinterlace::Mode::VaApi {
            // playsink may temporarily detach the output bin on stream changes.
            // Prepare VA before decodebin can create a second display and
            // replace the display underneath the existing VPP filters/pools.
            self.processor.set_state(gst::State::Ready)?;
            // VA answers context queries but does not retain its self-created
            // context in GstElement's context list like qml6glsink does.
            let mut query = gst::query::Context::new("gst.va.display.handle");
            let display = self
                .processor
                .query(&mut query)
                .then(|| query.context_owned())
                .flatten()
                .ok_or_else(|| {
                    Error::VideoOutput("vadeinterlace did not provide a VA display context".into())
                })?;
            self.playbin.set_context(&display);
        }
        self.playbin.set_property("uri", uri);
        if let Err(error) = self.playbin.set_state(gst::State::Playing) {
            // A synchronous failure may already have a more specific HTTP error queued.
            // Read it before cleanup flushes the bus; never parse diagnostic prose.
            return Err(self.poll().err().unwrap_or(Error::StateChange(error)));
        }
        *self.requested_uri.borrow_mut() = Some(uri.to_owned());
        tracing::info!(?service, "Starting playback");
        Ok(true)
    }

    fn stop(&self) -> Result<()> {
        match self.video_output {
            VideoOutputState::Closing => return Err(Error::OutputShutDown),
            VideoOutputState::Closed => {}
            _ => stop_stream(&self.playbin)?,
        }
        *self.requested_uri.borrow_mut() = None;
        self.presentation.clear();
        *self.audio_streams.borrow_mut() = audio_streams::Streams::default();
        self.routing.reset();
        *self.audio_intent.borrow_mut() = None;
        *self.audio_default.borrow_mut() = audio_default::Policy::default();
        Ok(())
    }

    pub fn set_audio_output(&self, output: audio_output::Output) {
        output.apply(&self.playbin);
    }

    pub fn subtitle_tracks(&self) -> Vec<crate::media_subtitles::Track> {
        self.audio_streams.borrow().text_tracks()
    }
    pub fn select_subtitle(
        &self,
        id: &str,
    ) -> std::result::Result<(), crate::media_subtitles::Error> {
        self.audio_streams
            .borrow_mut()
            .select_text(&self.playbin, id)
    }
    pub fn subtitle_canvas(&self) -> (i32, i32) {
        let info = self
            .sink
            .static_pad("sink")
            .and_then(|pad| pad.current_caps())
            .and_then(|caps| gstreamer_video::VideoInfo::from_caps(&caps).ok());
        let Some(info) = info else {
            return (1280, 720);
        };
        let ratio = info.width() as f64 * info.par().numer() as f64
            / info.par().denom() as f64
            / info.height().max(1) as f64;
        if ratio >= 1.0 {
            (1280, (1280.0 / ratio).round().clamp(1.0, 1920.0) as i32)
        } else {
            ((1280.0 * ratio).round().clamp(1.0, 1920.0) as i32, 1280)
        }
    }
    pub fn audio_failure(&self) -> Option<audio_streams::Error> {
        self.audio_streams.borrow().failure()
    }

    pub fn audio_tracks(&self) -> Vec<audio_streams::Track> {
        self.audio_streams.borrow().tracks()
    }

    pub fn select_audio(&self, id: &str) -> std::result::Result<(), audio_streams::Error> {
        self.audio_streams.borrow_mut().select(&self.playbin, id)
    }

    pub fn poll(&self) -> Result<Event> {
        let bus = self.playbin.bus().ok_or(Error::MissingBus)?;
        let mut event = Event::Idle;
        let mut failure = None;
        while let Some(message) = bus.pop() {
            if let Err(error) = self
                .audio_streams
                .borrow_mut()
                .observe(&self.playbin, &message)
            {
                tracing::error!("Audio selection: {error}");
            }
            match message.view() {
                gst::MessageView::Element(_) => {
                    if failure.is_none() {
                        failure = missing_decoder(&message);
                    }
                }
                gst::MessageView::NewClock(message) => {
                    if let Some(clock) = message.clock() {
                        tracing::info!("Playback clock selected: {}", clock.name());
                    }
                }
                gst::MessageView::Warning(warning) => {
                    let mut counts = self.warnings.get();
                    counts.observe(warning);
                    self.warnings.set(counts);
                }
                gst::MessageView::Error(e) => {
                    if failure.is_none() {
                        failure = Some(stream_error(e));
                    }
                }
                gst::MessageView::Eos(message) => {
                    event = Event::Ended(message.seqnum());
                }
                gst::MessageView::StateChanged(s)
                    if s.src() == Some(self.playbin.upcast_ref())
                        && s.current() == gst::State::Playing
                        && !matches!(event, Event::Ended(_)) =>
                {
                    event = Event::Playing;
                }
                _ => {}
            }
        }
        if let Some(error) = failure {
            return Err(error);
        }
        Ok(event)
    }

    fn shutdown(&mut self) -> Result<()> {
        self.shutdown_with(|element| {
            element.set_state(gst::State::Null)?;
            let (result, current, pending) = element.state(gst::ClockTime::ZERO);
            result?;
            if current != gst::State::Null || pending != gst::State::VoidPending {
                return Err(gst::StateChangeError);
            }
            Ok(())
        })
    }

    // Kept separate so integration checks can inject failed native transitions
    // without depending on a broken driver or device. Production always uses
    // GStreamer's synchronous downward transition to NULL.
    fn shutdown_with(
        &mut self,
        mut stop: impl FnMut(&gst::Element) -> std::result::Result<(), gst::StateChangeError>,
    ) -> Result<()> {
        if self.video_output == VideoOutputState::Closed {
            return Ok(());
        }
        self.video_output = VideoOutputState::Closing;
        // Stop upstream streaming before touching the sink's widget. A failed
        // bin transition may have stopped only some children; preserve the
        // binding and allow the caller to retry instead of assuming completion.
        stop(&self.playbin)?;
        if self.mode == deinterlace::Mode::VaApi {
            // VA preparation can enter READY before playbin owns the output.
            stop(&self.processor)?;
        }
        // The sink can have entered READY independently of playbin in play().
        stop(&self.sink)?;
        self.sink
            .set_property("widget", std::ptr::null_mut::<std::ffi::c_void>());
        self.video_output = VideoOutputState::Closed;
        *self.requested_uri.borrow_mut() = None;
        Ok(())
    }

    pub(crate) fn shutdown_before_drop(&mut self) {
        if let Err(error) = self.shutdown() {
            tracing::error!("Cannot safely destroy native playback after shutdown failed: {error}");
            // A destructor cannot veto QML destruction or return an error.
            // Do not unwind over CXX or release a potentially live native graph.
            // Normal window closing returns the error and keeps the item alive.
            std::process::abort();
        }
    }
}

impl Drop for Playback {
    fn drop(&mut self) {
        self.shutdown_before_drop();
    }
}

// Reusing playbin must not reset streamsynchronizer's pad numbering while
// playsink can still own its request pads. NULL is only for final destruction.
fn stop_stream(playbin: &gst::Element) -> Result<()> {
    let bus = playbin.bus().ok_or(Error::MissingBus)?;
    bus.set_flushing(true);
    let result = if playbin.current_state() == gst::State::Null {
        Ok(gst::StateChangeSuccess::Success)
    } else {
        playbin.set_state(gst::State::Ready)
    };
    bus.set_flushing(false);
    result?;
    Ok(())
}

/// Unique across both transport and container sessions; cache ownership cannot collide.
fn next_source_identity() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_av_decoders_fail_while_optional_subtitle_data_does_not() {
        gst::init().unwrap();
        for (caps, required) in [
            ("video/x-h265", true),
            ("audio/x-opus", true),
            ("subpicture/x-dvb", false),
            ("private/section", false),
        ] {
            let message = gst::message::Element::new(
                gst::Structure::builder("missing-plugin")
                    .field("type", "decoder")
                    .field("detail", gst::Caps::builder(caps).build())
                    .field("name", "Test decoder")
                    .build(),
            );
            assert_eq!(missing_decoder(&message).is_some(), required);
        }
    }

    #[test]
    fn recovery_is_limited_to_http_source_seek_errors() {
        gst::init().unwrap();
        let source = gst::ElementFactory::make("souphttpsrc").build().unwrap();
        for (code, recoverable) in [
            (gst::ResourceError::Seek, true),
            (gst::ResourceError::NotFound, false),
            (gst::ResourceError::Read, false),
        ] {
            let message = gst::message::Error::builder(code, "test")
                .src(&source)
                .build();
            let gst::MessageView::Error(error) = message.view() else {
                panic!("expected error")
            };
            assert_eq!(stream_error(error).is_live_resume_rejected(), recoverable);
        }
        let message =
            gst::message::Error::builder(gst::ResourceError::Seek, "other source").build();
        let gst::MessageView::Error(error) = message.view() else {
            panic!("expected error")
        };
        assert!(!stream_error(error).is_live_resume_rejected());
    }

    // Uses only native stream-synchronization pads: no display, GPU, or audio.
    #[test]
    fn stopping_preserves_unique_names_for_retained_stream_pads() {
        gst::init().unwrap();
        let pipeline = gst::Pipeline::new();
        let sync = gst::ElementFactory::make("streamsynchronizer")
            .build()
            .unwrap();
        pipeline.add(&sync).unwrap();
        pipeline.set_state(gst::State::Ready).unwrap();
        let retained = sync.request_pad_simple("sink_%u").unwrap();
        // playsink may still hold a request pad across a channel change.
        stop_stream(pipeline.upcast_ref()).unwrap();
        let next = sync.request_pad_simple("sink_%u").unwrap();
        assert_ne!(retained.name(), next.name());
        assert_eq!(sync.sink_pads().len(), 2);
        sync.release_request_pad(&retained);
        sync.release_request_pad(&next);
        pipeline.set_state(gst::State::Null).unwrap();
    }
}
