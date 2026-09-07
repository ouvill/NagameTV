mod audio_choices;
mod audio_components;
mod audio_default;
pub mod audio_output;
mod audio_routing;
pub mod audio_streams;
pub mod deinterlace;
pub mod failure;
pub mod stats;

use gstreamer::{self as gst, prelude::*};
use std::cell::RefCell;
use std::sync::{Mutex, OnceLock};

type Result<T> = std::result::Result<T, Error>;

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
    #[error("Video output is not ready")]
    OutputNotReady,
    #[error("Missing GStreamer bus")]
    MissingBus,
    #[error("配信が終了しました")]
    EndOfStream,
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

static PRELOADED: OnceLock<Mutex<Option<Playback>>> = OnceLock::new();

pub fn preload() -> Result<()> {
    PRELOADED
        .set(Mutex::new(Some(Playback::new()?)))
        .map_err(|_| Error::AlreadyInitialized)?;
    Ok(())
}

pub fn take_preloaded() -> Option<Playback> {
    PRELOADED.get()?.lock().ok()?.take()
}

pub struct Playback {
    playbin: gst::Element,
    sink: gst::Element,
    processor: gst::Element,
    queue: gst::Element,
    mode: deinterlace::Mode,
    attached: bool,
    audio_streams: RefCell<audio_streams::Streams>,
    routing: audio_routing::Routing,
    audio_intent: RefCell<Option<audio_choices::Intent>>,
    audio_default: RefCell<audio_default::Policy>,
    requested_uri: RefCell<Option<String>>,
}

impl Playback {
    pub fn element(&self) -> &gst::Element {
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
    pub fn position(&self) -> Option<gst::ClockTime> {
        self.sink.query_position::<gst::ClockTime>()
    }
    fn new() -> Result<Self> {
        gst::init()?;
        let sink = gst::ElementFactory::make("qml6glsink")
            .property("enable-last-sample", false)
            .build()?;
        let input = gst::ElementFactory::make("videoconvert").build()?;
        let mode = deinterlace::Mode::from_environment()?;
        let deinterlace = mode.build()?;
        eprintln!("Video processing: {}", mode.label());
        let queue = gst::ElementFactory::make("queue")
            .property("max-size-buffers", 8_u32)
            .property("max-size-bytes", 0_u32)
            .property("max-size-time", 0_u64)
            .build()?;
        let upload = gst::ElementFactory::make("glupload").build()?;
        let convert = gst::ElementFactory::make("glcolorconvert").build()?;
        let filter = gst::ElementFactory::make("capsfilter")
            .property(
                "caps",
                gst::Caps::builder("video/x-raw")
                    .features(["memory:GLMemory"])
                    .field("format", "RGBA")
                    .field("texture-target", "2D")
                    .build(),
            )
            .build()?;
        let output = gst::Bin::new();
        let elements = [
            &input,
            &deinterlace,
            &queue,
            &upload,
            &convert,
            &filter,
            &sink,
        ];
        output.add_many(elements)?;
        gst::Element::link_many(elements)?;
        let pad =
            gst::GhostPad::with_target(&input.static_pad("sink").ok_or(Error::MissingSinkPad)?)?;
        pad.set_active(true)?;
        output.add_pad(&pad)?;
        let audio = gst::ElementFactory::make("pulsesink")
            .property("enable-last-sample", false)
            .build()?;
        let routing = audio_routing::Routing::default();
        let audio_filter = routing.filter()?;
        let playbin = gst::ElementFactory::make("playbin3").build()?;
        playbin.set_property("audio-filter", &audio_filter);
        playbin.set_property_from_str("flags", "video+audio+soft-volume+buffering+native-video");
        playbin.set_property("video-sink", &output);
        playbin.set_property("audio-sink", &audio);
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
            processor: deinterlace,
            queue,
            mode,
            attached: false,
            audio_streams: RefCell::default(),
            routing,
            audio_intent: RefCell::default(),
            audio_default: RefCell::default(),
            requested_uri: RefCell::new(None),
        })
    }

    /// The GUI owns the live QQuickItem until shutdown has stopped the sink.
    pub unsafe fn attach(&mut self, item: usize) -> Result<()> {
        if item == 0 {
            return Err(Error::MissingVideoItem);
        }
        self.sink
            .set_property("widget", item as *mut std::ffi::c_void);
        self.attached = true;
        Ok(())
    }

    /// Returns false when this stream is already connecting or playing.
    pub fn play(
        &self,
        server: &str,
        service: u64,
        broadcast: Option<crate::channels::BroadcastService>,
    ) -> Result<bool> {
        if !self.attached {
            return Err(Error::OutputNotReady);
        }
        let uri = format!("{server}/api/services/{service}/stream");
        if self.requested_uri.borrow().as_ref() == Some(&uri) {
            return Ok(false);
        }
        self.stop()?;
        *self.audio_streams.borrow_mut() =
            audio_streams::Streams::for_service(broadcast.map(|service| service.service_id));
        // Qt must supply the GL display before any other GL element starts.
        self.sink.set_state(gst::State::Ready)?;
        self.playbin.set_property("uri", &uri);
        if let Err(error) = self.playbin.set_state(gst::State::Playing) {
            // A synchronous failure may already have a more specific HTTP error queued.
            // Read it before cleanup flushes the bus; never parse diagnostic prose.
            return Err(self.poll().err().unwrap_or(Error::StateChange(error)));
        }
        *self.requested_uri.borrow_mut() = Some(uri);
        eprintln!("Starting service {service}");
        Ok(true)
    }

    pub fn stop(&self) -> Result<()> {
        stop_stream(&self.playbin)?;
        *self.requested_uri.borrow_mut() = None;
        *self.audio_streams.borrow_mut() = audio_streams::Streams::default();
        self.routing.reset();
        *self.audio_intent.borrow_mut() = None;
        *self.audio_default.borrow_mut() = audio_default::Policy::default();
        Ok(())
    }

    pub fn set_audio_output(&self, output: audio_output::Output) {
        output.apply(&self.playbin);
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

    pub fn poll(&self) -> Result<bool> {
        let bus = self.playbin.bus().ok_or(Error::MissingBus)?;
        let mut playing = false;
        let mut failure = None;
        while let Some(message) = bus.pop() {
            if let Err(error) = self
                .audio_streams
                .borrow_mut()
                .observe(&self.playbin, &message)
            {
                eprintln!("Audio selection: {error}");
            }
            match message.view() {
                gst::MessageView::Error(e) => {
                    if failure.is_none() {
                        failure = Some(stream_error(e));
                    }
                }
                gst::MessageView::Eos(_) => {
                    failure.get_or_insert(Error::EndOfStream);
                }
                gst::MessageView::StateChanged(s) if s.src() == Some(self.playbin.upcast_ref()) => {
                    playing |= s.current() == gst::State::Playing;
                }
                _ => {}
            }
        }
        if let Some(error) = failure {
            return Err(error);
        }
        Ok(playing)
    }

    pub fn shutdown(&mut self) {
        if let Err(error) = self.stop() {
            eprintln!("Stop failed: {error}");
        }
        if let Err(error) = self.playbin.set_state(gst::State::Null) {
            eprintln!("Playback shutdown failed: {error}");
        }
        if let Err(error) = self.sink.set_state(gst::State::Null) {
            eprintln!("Video sink shutdown failed: {error}");
        }
        self.sink
            .set_property("widget", std::ptr::null_mut::<std::ffi::c_void>());
        self.attached = false;
    }
}

impl Drop for Playback {
    fn drop(&mut self) {
        self.shutdown();
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

#[cfg(test)]
mod tests {
    use super::*;

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
