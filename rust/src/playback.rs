mod session;
#[cfg(test)]
mod session_tests;

use crate::audio::{AudioRouting, AudioStreams};
use gst::prelude::*;
use gstreamer as gst;
use std::cell::RefCell;
use std::ffi::{c_char, c_void};
use std::ptr;
use std::sync::{Arc, Mutex, OnceLock};
use thiserror::Error;

use crate::subtitles::{SubtitleClock, SubtitleUpdate};
use crate::transport::TransportParser;

static PRELOADED: OnceLock<Mutex<Option<Playback>>> = OnceLock::new();

#[derive(Debug, Error)]
pub enum PlaybackError {
    #[error("Could not initialize GStreamer: {0}")]
    Initialization(#[source] gst::glib::Error),
    #[error("Could not create {element}: {source}")]
    ElementCreation {
        element: &'static str,
        #[source]
        source: gst::glib::BoolError,
    },
    #[error("Invalid MIRAKURUN_DEINTERLACE value '{value}'; use yadif, linear, or off")]
    InvalidDeinterlaceMode { value: String },
    #[error("The playbin flags property is not a flags type")]
    InvalidPlaybinFlagsType,
    #[error("The playback element is not a GStreamer bin")]
    InvalidPlaybinType,
    #[error("Could not configure playbin flag '{flag}' (enabled={enabled})")]
    PlaybinFlagConfiguration { flag: &'static str, enabled: bool },
    #[error("Could not assemble video output: {0}")]
    VideoOutputAssembly(#[source] gst::glib::BoolError),
    #[error("Could not link video output: {0}")]
    VideoOutputLink(#[source] gst::glib::BoolError),
    #[error("Video processor has no sink pad")]
    MissingVideoSinkPad,
    #[error("Could not expose video sink pad: {0}")]
    VideoSinkPadExposure(#[source] gst::glib::BoolError),
    #[error("Could not activate video sink pad: {0}")]
    VideoSinkPadActivation(#[source] gst::glib::BoolError),
    #[error("Could not add video sink pad: {0}")]
    VideoSinkPadAddition(#[source] gst::glib::BoolError),
    #[error("Playback was already preloaded")]
    AlreadyPreloaded,
    #[error("The preloaded playback lock is poisoned")]
    PreloadLockPoisoned,
    #[error("The preloaded playback pipeline is unavailable")]
    PreloadedUnavailable,
    #[error("The video item is null")]
    NullVideoItem,
    #[error("The Qt video item is not attached")]
    VideoItemNotAttached,
    #[error("The server URL must start with http:// or https://")]
    InvalidServerUrl,
    #[error("The service ID must be greater than zero")]
    InvalidServiceId,
    #[error("Could not {operation}: {source}")]
    StateChange {
        operation: &'static str,
        #[source]
        source: gst::StateChangeError,
    },
    #[error("The GStreamer message bus is unavailable")]
    BusUnavailable,
    #[error("The MPEG-TS extractor lock is poisoned")]
    ExtractorLockPoisoned,
    #[error("GStreamer playback error: {source} (HTTP: {http_status:?}, debug: {debug:?})")]
    Pipeline {
        #[source]
        source: gst::glib::Error,
        debug: Option<String>,
        http_status: Option<u32>,
        network_source: bool,
    },
    #[error("Could not configure audio routing: {0}")]
    AudioRouting(#[source] gst::glib::Error),
    #[error("The live stream ended unexpectedly. Try again to reconnect.")]
    StreamEnded,
}

impl PlaybackError {
    pub fn user_message(&self) -> &'static str {
        match self {
            Self::Pipeline {
                http_status: Some(503),
                ..
            } => {
                "No tuner is available. Tuners may be in use or unavailable. Wait a moment and try again, or choose another channel."
            }
            Self::Pipeline {
                http_status: Some(404),
                ..
            } => {
                "This channel was not found on Mirakurun. Refresh the channel list and choose a channel again."
            }
            Self::Pipeline {
                http_status: Some(401 | 403),
                ..
            } => "Mirakurun denied access to the stream. Check the server's access settings.",
            Self::Pipeline {
                http_status: Some(408 | 504),
                ..
            } => "The server did not respond in time. Check the connection and try again.",
            Self::Pipeline {
                http_status: Some(500..=599),
                ..
            } => "Mirakurun could not start the stream. Check the server and try again.",
            Self::Pipeline {
                http_status: Some(400..=499),
                ..
            } => {
                "Mirakurun rejected the stream request. Check the connection settings and channel."
            }
            Self::Pipeline {
                source,
                network_source: true,
                ..
            } if source.kind::<gst::ResourceError>().is_some() => {
                "Could not receive the stream from Mirakurun. Check the server and network connection, then try again."
            }
            Self::InvalidServerUrl => "Enter a server URL starting with http:// or https://",
            Self::InvalidServiceId => "Enter a valid Mirakurun service ID",
            Self::StreamEnded => "The live stream ended unexpectedly. Try again to reconnect.",
            _ => {
                "Could not play this channel. Try again or choose another channel. See the error details if the problem continues."
            }
        }
    }
}

pub fn preload() -> Result<(), PlaybackError> {
    let playback = Playback::new()?;
    PRELOADED
        .set(Mutex::new(Some(playback)))
        .map_err(|_| PlaybackError::AlreadyPreloaded)
}

pub fn take_preloaded() -> Result<Playback, PlaybackError> {
    let slot = PRELOADED.get().ok_or(PlaybackError::PreloadedUnavailable)?;
    slot.lock()
        .map_err(|_| PlaybackError::PreloadLockPoisoned)?
        .take()
        .ok_or(PlaybackError::PreloadedUnavailable)
}

#[link(name = "gobject-2.0")]
unsafe extern "C" {
    fn g_object_set(object: *mut c_void, first_property_name: *const c_char, ...);
}

pub struct Playback {
    playbin: gst::Element,
    video_sink: gst::Element,
    video_process: gst::Element,
    video_queue: gst::Element,
    deinterlace_mode: DeinterlaceMode,
    video_attached: bool,
    subtitles: SubtitleClock,
    audio: RefCell<AudioStreams>,
    extractor: Arc<Mutex<TransportParser>>,
    routing: AudioRouting,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DeinterlaceMode {
    Yadif,
    Linear,
    Off,
}

impl DeinterlaceMode {
    fn from_environment() -> Result<Self, PlaybackError> {
        let value = std::env::var("MIRAKURUN_DEINTERLACE").unwrap_or_else(|_| "yadif".into());
        Self::parse(&value)
    }

    fn parse(value: &str) -> Result<Self, PlaybackError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "yadif" | "quality" => Ok(Self::Yadif),
            "linear" | "balanced" => Ok(Self::Linear),
            "off" | "disabled" => Ok(Self::Off),
            _ => Err(PlaybackError::InvalidDeinterlaceMode {
                value: value.to_owned(),
            }),
        }
    }
}

impl Playback {
    pub fn new() -> Result<Self, PlaybackError> {
        gst::init().map_err(PlaybackError::Initialization)?;
        let video_sink = gst::ElementFactory::make("qml6glsink")
            .name("qt-video-sink")
            .build()
            .map_err(|source| PlaybackError::ElementCreation {
                element: "qml6glsink",
                source,
            })?;
        video_sink.set_property("enable-last-sample", false);

        let deinterlace_mode = DeinterlaceMode::from_environment()?;
        let video_process = match deinterlace_mode {
            DeinterlaceMode::Off => gst::ElementFactory::make("identity")
                .name("deinterlace-disabled")
                .build()
                .map_err(|source| PlaybackError::ElementCreation {
                    element: "video passthrough",
                    source,
                })?,
            mode => {
                let element = gst::ElementFactory::make("deinterlace")
                    .name("deinterlace")
                    .build()
                    .map_err(|source| PlaybackError::ElementCreation {
                        element: "deinterlace",
                        source,
                    })?;
                element.set_property_from_str(
                    "method",
                    match mode {
                        DeinterlaceMode::Yadif => "yadif",
                        DeinterlaceMode::Linear => "linear",
                        DeinterlaceMode::Off => unreachable!(),
                    },
                );
                element.set_property_from_str("mode", "auto");
                element.set_property_from_str("fields", "all");
                element.set_property_from_str("locking", "auto");
                element
            }
        };

        let video_queue = gst::ElementFactory::make("queue")
            .name("bounded-video-queue")
            .build()
            .map_err(|source| PlaybackError::ElementCreation {
                element: "video queue",
                source,
            })?;
        video_queue.set_property("max-size-buffers", 8_u32);
        video_queue.set_property("max-size-bytes", 0_u32);
        video_queue.set_property("max-size-time", 0_u64);

        let gl_upload = gst::ElementFactory::make("glupload")
            .build()
            .map_err(|source| PlaybackError::ElementCreation {
                element: "glupload",
                source,
            })?;
        let gl_convert = gst::ElementFactory::make("glcolorconvert")
            .build()
            .map_err(|source| PlaybackError::ElementCreation {
                element: "glcolorconvert",
                source,
            })?;
        let rgba_filter = gst::ElementFactory::make("capsfilter")
            .build()
            .map_err(|source| PlaybackError::ElementCreation {
                element: "RGBA filter",
                source,
            })?;
        rgba_filter.set_property(
            "caps",
            gst::Caps::builder("video/x-raw")
                .features(["memory:GLMemory"])
                .field("format", "RGBA")
                .field("texture-target", "2D")
                .build(),
        );

        // This bin owns format conversion when playbin's conversion is disabled.
        let input_convert = gst::ElementFactory::make("videoconvert")
            .name("video-input-convert")
            .build()
            .map_err(|source| PlaybackError::ElementCreation {
                element: "videoconvert",
                source,
            })?;
        let video_output = gst::Bin::with_name("qt-video-output");
        video_output
            .add_many([
                &input_convert,
                &video_process,
                &video_queue,
                &gl_upload,
                &gl_convert,
                &rgba_filter,
                &video_sink,
            ])
            .map_err(PlaybackError::VideoOutputAssembly)?;
        gst::Element::link_many([
            &input_convert,
            &video_process,
            &video_queue,
            &gl_upload,
            &gl_convert,
            &rgba_filter,
            &video_sink,
        ])
        .map_err(PlaybackError::VideoOutputLink)?;
        let sink_pad = input_convert
            .static_pad("sink")
            .ok_or(PlaybackError::MissingVideoSinkPad)?;
        let ghost_pad =
            gst::GhostPad::with_target(&sink_pad).map_err(PlaybackError::VideoSinkPadExposure)?;
        ghost_pad
            .set_active(true)
            .map_err(PlaybackError::VideoSinkPadActivation)?;
        video_output
            .add_pad(&ghost_pad)
            .map_err(PlaybackError::VideoSinkPadAddition)?;

        let playbin = gst::ElementFactory::make("playbin3")
            .name("mirakurun-player")
            .build()
            .map_err(|source| PlaybackError::ElementCreation {
                element: "playbin3",
                source,
            })?;
        // Our sink bin owns deinterlacing and conversion; do not process the
        // video through playbin's intermediate conversion/deinterlacing chain.
        let flags = playbin.property_value("flags");
        let flags = configure_video_flags(flags)?;
        playbin.set_property_from_value("flags", &flags);
        playbin.set_property("video-sink", &video_output);
        if std::env::var("MIRAKURUN_AUDIO_SINK").is_ok_and(|value| value == "fakesink") {
            let audio_sink = gst::ElementFactory::make("fakesink")
                .name("isolated-test-audio-sink")
                .build()
                .map_err(|source| PlaybackError::ElementCreation {
                    element: "test audio sink",
                    source,
                })?;
            audio_sink.set_property("enable-last-sample", false);
            audio_sink.set_property("sync", true);
            playbin.set_property("audio-sink", &audio_sink);
        } else if std::env::var_os("PULSE_SERVER").is_some() {
            // Workshop exposes host audio through a PulseAudio TCP endpoint, but no
            // PipeWire socket or ALSA device.  Letting autoaudiosink probe those
            // unavailable backends can make playbin3 rebuild its output while it is
            // being configured.  Select the known working transport directly.
            let audio_sink = gst::ElementFactory::make("pulsesink")
                .name("pulse-audio-sink")
                .build()
                .map_err(|source| PlaybackError::ElementCreation {
                    element: "PulseAudio sink",
                    source,
                })?;
            playbin.set_property("audio-sink", &audio_sink);
        }
        playbin.set_property("volume", 0.7_f64);
        let routing = AudioRouting::default();
        let audio_filter = routing.filter().map_err(PlaybackError::AudioRouting)?;
        playbin.set_property("audio-filter", audio_filter);
        let subtitles = SubtitleClock::default();
        let playbin_bin = playbin
            .downcast_ref::<gst::Bin>()
            .ok_or(PlaybackError::InvalidPlaybinType)?;
        subtitles.attach(playbin_bin);
        let subtitle_extractor = Arc::new(Mutex::new(TransportParser::new(true)));
        let extractor_for_source = subtitle_extractor.clone();
        let subtitles_for_source = subtitles.clone();
        playbin.connect("source-setup", false, move |values| {
            let Some(source) = values
                .get(1)
                .and_then(|value| value.get::<gst::Element>().ok())
            else {
                return None;
            };
            let factory = source
                .factory()
                .map(|factory| factory.name().to_string())
                .unwrap_or_else(|| source.type_().name().to_owned());
            if factory == "souphttpsrc" {
                // Surface tuner exhaustion immediately; retries are an explicit UI action.
                source.set_property("retries", 0_i32);
                source.set_property("timeout", 15_u32);
            }
            tracing::debug!(source = %factory, "Attaching MPEG-TS subtitle extractor");
            attach_subtitle_probe(
                &source,
                extractor_for_source.clone(),
                subtitles_for_source.clone(),
            );
            None
        });
        Ok(Self {
            playbin,
            video_sink,
            video_process,
            video_queue,
            deinterlace_mode,
            video_attached: false,
            subtitles,
            audio: RefCell::new(AudioStreams::default()),
            extractor: subtitle_extractor,
            routing,
        })
    }

    pub fn video_stats(&self) -> crate::video_stats::VideoStats {
        crate::video_stats::snapshot(
            &self.playbin,
            &self.video_process,
            &self.video_queue,
            &self.video_sink,
            match self.deinterlace_mode {
                DeinterlaceMode::Yadif => "YADIF (auto / all fields)",
                DeinterlaceMode::Linear => "Linear (auto / all fields)",
                DeinterlaceMode::Off => "Off",
            },
        )
    }

    pub fn attach_video_item(&mut self, widget: *mut c_void) -> Result<(), PlaybackError> {
        if widget.is_null() {
            return Err(PlaybackError::NullVideoItem);
        }
        unsafe {
            g_object_set(
                self.video_sink.as_ptr().cast(),
                c"widget".as_ptr(),
                widget,
                ptr::null::<c_char>(),
            );
        }
        self.video_sink.set_property("force-aspect-ratio", true);
        self.video_attached = true;
        Ok(())
    }

    pub fn play_service(
        &self,
        server: &str,
        service_id: u64,
        program: Option<crate::audio::AudioProgram>,
    ) -> Result<(), PlaybackError> {
        if !self.video_attached {
            return Err(PlaybackError::VideoItemNotAttached);
        }
        let url = service_stream_url(server, service_id)?;
        if self.playbin.current_state() == gst::State::Null {
            self.prepare_video_sink()?;
        }
        let bus = self.playbin.bus().ok_or(PlaybackError::BusUnavailable)?;
        self.prepare_stream(&bus, program)?;
        self.playbin.set_property("uri", url);
        if let Err(source) = self.playbin.set_state(gst::State::Playing) {
            // A synchronous state failure can already have the useful HTTP error queued.
            drain_bus_events(&bus, &self.playbin)?;
            return Err(PlaybackError::StateChange {
                operation: "start playback",
                source,
            });
        }
        Ok(())
    }

    fn prepare_video_sink(&self) -> Result<(), PlaybackError> {
        // qml6glsink must reach READY before the other GL elements so its Qt-backed
        // GstGLDisplay and GstGLContext are propagated through the pipeline.
        self.video_sink
            .set_state(gst::State::Ready)
            .map(|_| ())
            .map_err(|source| PlaybackError::StateChange {
                operation: "prepare the Qt video sink",
                source,
            })
    }

    pub fn stop(&self) -> Result<(), PlaybackError> {
        self.playbin
            .set_state(gst::State::Null)
            .map(|_| ())
            .map_err(|source| PlaybackError::StateChange {
                operation: "stop playback",
                source,
            })?;
        self.reset_stream_state()
    }

    pub fn set_subtitles_enabled(&self, enabled: bool) -> Result<(), PlaybackError> {
        let mut extractor = self
            .extractor
            .lock()
            .map_err(|_| PlaybackError::ExtractorLockPoisoned)?;
        extractor.set_subtitles_enabled(enabled);
        self.subtitles.set_enabled(enabled);
        Ok(())
    }

    pub fn pending_subtitles(&self) -> Option<usize> {
        self.subtitles.pending_count()
    }

    pub fn poll_subtitles(&self) -> SubtitleUpdate {
        // GstBaseSink's TIME position includes clock/segment/latency handling;
        // source arrival time and the decoder's ahead-of-playback position do not.
        let position = if self.playbin.current_state() == gst::State::Playing {
            self.video_sink.query_position::<gst::ClockTime>()
        } else {
            None
        };
        self.subtitles.poll(position)
    }

    pub fn set_volume(&self, volume: f64) {
        self.playbin
            .set_property("volume", volume.clamp(0.0, 100.0) / 100.0);
    }

    pub fn drain_events(&self) -> Result<PlaybackEvent, PlaybackError> {
        let Some(bus) = self.playbin.bus() else {
            return Err(PlaybackError::BusUnavailable);
        };
        drain_bus_events_with(&bus, &self.playbin, |message| {
            self.audio.borrow_mut().observe(&self.playbin, message);
        })
    }

    pub fn set_audio_program(&self, program: Option<crate::audio::AudioProgram>) {
        let mut audio = self.audio.borrow_mut();
        if audio.program != program {
            audio.program = program;
            audio.reset_choice();
            self.routing.set_mode(0);
        }
    }

    pub fn audio_state(&self) -> (Vec<crate::audio::AudioOption>, String) {
        let mut audio = self.audio.borrow_mut();
        if let Ok(extractor) = self.extractor.lock() {
            if audio.components != extractor.audio_components {
                audio.components = extractor.audio_components.clone();
                audio.reset_choice();
                self.routing.set_mode(0);
            }
        }
        let options = audio.options(self.routing.mode(), self.routing.channels());
        if audio.choice.is_none() {
            if let Some(default) = options
                .iter()
                .find(|option| option.default && option.enabled)
            {
                if audio.selected_index() == default.track as i32
                    || audio.select(&self.playbin, default.track as i32)
                {
                    audio.choice = Some(default.key.clone());
                }
            }
        }
        if let Some(choice) = &audio.choice {
            if let Some(option) = options.iter().find(|option| &option.key == choice) {
                if audio.selected_index() == option.track as i32 {
                    self.routing.set_mode(option.mode);
                }
            } else {
                audio.reset_choice();
                self.routing.set_mode(0);
            }
        }
        (
            audio.options(self.routing.mode(), self.routing.channels()),
            audio.error.clone(),
        )
    }

    pub fn select_audio_option(&self, key: &str) -> bool {
        let mut audio = self.audio.borrow_mut();
        let Some(option) = audio
            .options(self.routing.mode(), self.routing.channels())
            .into_iter()
            .find(|option| option.key == key && option.enabled)
        else {
            audio.error = "This audio track is no longer available. Choose a track again.".into();
            return false;
        };
        if audio.selected_index() != option.track as i32 {
            self.routing.set_mode(0);
            if !audio.select(&self.playbin, option.track as i32) {
                return false;
            }
        } else {
            self.routing.set_mode(option.mode);
        }
        audio.choice = Some(key.to_owned());
        audio.error.clear();
        true
    }
}

fn drain_bus_events(
    bus: &gst::Bus,
    playbin: &gst::Element,
) -> Result<PlaybackEvent, PlaybackError> {
    drain_bus_events_with(bus, playbin, |_| {})
}

fn drain_bus_events_with(
    bus: &gst::Bus,
    playbin: &gst::Element,
    mut observe: impl FnMut(&gst::MessageRef),
) -> Result<PlaybackEvent, PlaybackError> {
    let mut outcome = PlaybackEvent::None;
    let mut first_error = None;
    while let Some(message) = bus.pop() {
        observe(&message);
        use gst::MessageView;
        match message.view() {
            MessageView::Error(error) => {
                if first_error.is_none() {
                    first_error = Some(PlaybackError::Pipeline {
                        source: error.error(),
                        debug: error.debug().map(|debug| debug.to_string()),
                        http_status: error
                            .details()
                            .and_then(|details| details.get::<u32>("http-status-code").ok()),
                        network_source: error
                            .src()
                            .and_then(|src| src.downcast_ref::<gst::Element>())
                            .and_then(|element| element.factory())
                            .is_some_and(|factory| factory.name() == "souphttpsrc"),
                    });
                }
            }
            MessageView::Eos(..) => outcome = PlaybackEvent::Ended,
            MessageView::StateChanged(state)
                if state.src() == Some(playbin.upcast_ref())
                    && state.current() == gst::State::Playing
                    && outcome != PlaybackEvent::Ended =>
            {
                outcome = PlaybackEvent::Playing;
            }
            _ => {}
        }
    }
    match first_error {
        Some(error) => Err(error),
        None => Ok(outcome),
    }
}

fn configure_video_flags(mut flags: gst::glib::Value) -> Result<gst::glib::Value, PlaybackError> {
    let class = gst::glib::FlagsClass::with_type(flags.type_())
        .ok_or(PlaybackError::InvalidPlaybinFlagsType)?;
    for (flag, enabled) in [
        ("deinterlace", false),
        ("native-video", true),
        ("soft-colorbalance", false),
        // ARIB captions are already extracted from TS and rendered by QML.
        ("text", false),
    ] {
        flags = if enabled {
            class.set_by_nick(flags, flag)
        } else {
            class.unset_by_nick(flags, flag)
        }
        .map_err(|_| PlaybackError::PlaybinFlagConfiguration { flag, enabled })?;
    }
    Ok(flags)
}

fn attach_subtitle_probe(
    source: &gst::Element,
    extractor: Arc<Mutex<TransportParser>>,
    subtitles: SubtitleClock,
) {
    let Some(pad) = source.static_pad("src") else {
        tracing::warn!("MPEG-TS subtitle extractor source has no src pad");
        return;
    };
    pad.add_probe(gst::PadProbeType::BUFFER, move |_, info| {
        let Some(buffer) = info.buffer() else {
            return gst::PadProbeReturn::Ok;
        };
        let Ok(data) = buffer.map_readable() else {
            return gst::PadProbeReturn::Ok;
        };
        // Keep the extractor lock through publication. Disabling takes the same
        // lock before clearing the clock, so an old cue cannot arrive afterwards.
        if let Ok(mut extractor) = extractor.lock() {
            subtitles.push(extractor.push(data.as_slice()));
        }
        gst::PadProbeReturn::Ok
    });
}

impl Drop for Playback {
    fn drop(&mut self) {
        if let Err(error) = self.playbin.set_state(gst::State::Null) {
            tracing::warn!(%error, "Could not stop playback while dropping");
        }
        if let Err(error) = self.video_sink.set_state(gst::State::Null) {
            tracing::warn!(%error, "Could not stop the video sink while dropping");
        }
        if self.video_attached {
            unsafe {
                g_object_set(
                    self.video_sink.as_ptr().cast(),
                    c"widget".as_ptr(),
                    ptr::null_mut::<c_void>(),
                    ptr::null::<c_char>(),
                );
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaybackEvent {
    None,
    Playing,
    Ended,
}

fn service_stream_url(server: &str, service_id: u64) -> Result<String, PlaybackError> {
    let server = server.trim().trim_end_matches('/');
    if !(server.starts_with("http://") || server.starts_with("https://")) {
        return Err(PlaybackError::InvalidServerUrl);
    }
    if service_id == 0 {
        return Err(PlaybackError::InvalidServiceId);
    }
    Ok(format!("{server}/api/services/{service_id}/stream"))
}

#[cfg(test)]
mod tests {
    // In tests, unwrap/expect assert successful setup or an expected result.
    // Failures intentionally fail the test; they are not assumed impossible IO.
    use super::{DeinterlaceMode, PlaybackError, service_stream_url};
    use gst::prelude::*;
    use gstreamer as gst;

    // Bus-only tests: no playback, display, GPU, or audio device is started.
    fn queued_http_error(bus: &gst::Bus, status: u32) {
        bus.post(
            gst::message::Error::builder(gst::ResourceError::Read, "Service unavailable")
                .details(
                    gst::Structure::builder("details")
                        .field("http-status-code", status)
                        .build(),
                )
                .debug("HTTP stream request failed")
                .build(),
        )
        .unwrap();
    }

    #[test]
    fn preserves_http_cause_and_drains_followup_errors() {
        gst::init().unwrap();
        let bus = gst::Bus::new();
        let playbin = gst::ElementFactory::make("playbin3").build().unwrap();
        queued_http_error(&bus, 503);
        bus.post(gst::message::Error::new(
            gst::StreamError::Failed,
            "Internal data stream error",
        ))
        .unwrap();
        bus.post(
            gst::message::StateChanged::builder(
                gst::State::Paused,
                gst::State::Playing,
                gst::State::VoidPending,
            )
            .src(&playbin)
            .build(),
        )
        .unwrap();
        let error = super::drain_bus_events(&bus, &playbin).unwrap_err();
        assert!(matches!(
            &error,
            PlaybackError::Pipeline {
                http_status: Some(503),
                ..
            }
        ));
        assert!(error.user_message().starts_with("No tuner is available."));
        assert!(error.to_string().contains("HTTP stream request failed"));
        assert_eq!(
            super::drain_bus_events(&bus, &playbin).unwrap(),
            super::PlaybackEvent::None
        );
        // A fresh attempt can now report Playing without the previous error.
        bus.post(
            gst::message::StateChanged::builder(
                gst::State::Paused,
                gst::State::Playing,
                gst::State::VoidPending,
            )
            .src(&playbin)
            .build(),
        )
        .unwrap();
        assert_eq!(
            super::drain_bus_events(&bus, &playbin).unwrap(),
            super::PlaybackEvent::Playing
        );
    }

    #[test]
    fn distinguishes_http_failures() {
        gst::init().unwrap();
        let bus = gst::Bus::new();
        let playbin = gst::ElementFactory::make("playbin3").build().unwrap();
        for (status, message) in [
            (404, "This channel was not found"),
            (401, "Mirakurun denied access"),
            (403, "Mirakurun denied access"),
            (408, "The server did not respond in time"),
            (504, "The server did not respond in time"),
            (500, "Mirakurun could not start the stream"),
            (502, "Mirakurun could not start the stream"),
            (429, "Mirakurun rejected the stream request"),
        ] {
            queued_http_error(&bus, status);
            let error = super::drain_bus_events(&bus, &playbin).unwrap_err();
            assert!(
                error.user_message().starts_with(message),
                "HTTP {status}: {error}"
            );
        }
    }

    #[test]
    fn distinguishes_network_errors_from_output_errors() {
        gst::init().unwrap();
        let mut error = PlaybackError::Pipeline {
            source: gst::glib::Error::new(gst::ResourceError::OpenRead, "Connection refused"),
            debug: None,
            http_status: None,
            network_source: true,
        };
        assert!(
            error
                .user_message()
                .starts_with("Could not receive the stream")
        );
        if let PlaybackError::Pipeline { network_source, .. } = &mut error {
            *network_source = false;
        }
        assert!(
            error
                .user_message()
                .starts_with("Could not play this channel")
        );
    }

    #[test]
    fn eos_is_not_overwritten_by_a_queued_playing_message() {
        gst::init().unwrap();
        let bus = gst::Bus::new();
        let playbin = gst::ElementFactory::make("playbin3").build().unwrap();
        bus.post(gst::message::Eos::new()).unwrap();
        bus.post(
            gst::message::StateChanged::builder(
                gst::State::Paused,
                gst::State::Playing,
                gst::State::VoidPending,
            )
            .src(&playbin)
            .build(),
        )
        .unwrap();
        assert_eq!(
            super::drain_bus_events(&bus, &playbin).unwrap(),
            super::PlaybackEvent::Ended
        );
    }

    #[test]
    fn configures_playbin_video_flags() {
        gst::init().unwrap();
        // Inspect configuration only; do not start playback or access devices.
        let playbin = gst::ElementFactory::make("playbin3").build().unwrap();
        let original = playbin.property_value("flags");
        let class = gst::glib::FlagsClass::with_type(original.type_()).unwrap();
        let flags = super::configure_video_flags(original.clone()).unwrap();
        assert!(class.is_set_by_nick(&flags, "native-video"));
        for flag in ["deinterlace", "soft-colorbalance", "text"] {
            assert!(!class.is_set_by_nick(&flags, flag));
        }
        for flag in ["audio", "video", "buffering", "soft-volume"] {
            assert_eq!(
                class.is_set_by_nick(&flags, flag),
                class.is_set_by_nick(&original, flag)
            );
        }
        playbin.set_property_from_value("flags", &flags);
    }

    #[test]
    fn rejects_invalid_playbin_flags_without_panicking() {
        assert!(matches!(
            super::configure_video_flags(0_u32.to_value()),
            Err(PlaybackError::InvalidPlaybinFlagsType)
        ));
    }

    #[test]
    fn rejects_missing_playbin_flags_without_panicking() {
        gst::init().unwrap();
        assert!(matches!(
            super::configure_video_flags(gst::BufferFlags::empty().to_value()),
            Err(PlaybackError::PlaybinFlagConfiguration {
                flag: "deinterlace",
                enabled: false
            })
        ));
    }
    #[test]
    fn builds_service_url() {
        assert_eq!(
            service_stream_url("http://192.168.3.3:40772/", 3_203_246_080).unwrap(),
            "http://192.168.3.3:40772/api/services/3203246080/stream"
        );
    }
    #[test]
    fn validates_inputs() {
        assert!(matches!(
            service_stream_url("ftp://example.test", 1),
            Err(PlaybackError::InvalidServerUrl)
        ));
        assert!(matches!(
            service_stream_url("http://example.test", 0),
            Err(PlaybackError::InvalidServiceId)
        ));
        assert_eq!(
            DeinterlaceMode::parse("YADIF").unwrap(),
            DeinterlaceMode::Yadif
        );
        assert_eq!(
            DeinterlaceMode::parse("balanced").unwrap(),
            DeinterlaceMode::Linear
        );
        assert!(matches!(
            DeinterlaceMode::parse("unknown"),
            Err(PlaybackError::InvalidDeinterlaceMode { value }) if value == "unknown"
        ));
    }
}
