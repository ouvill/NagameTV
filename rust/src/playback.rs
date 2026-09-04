use gst::prelude::*;
use gstreamer as gst;
use std::ffi::{c_char, c_void};
use std::ptr;
use std::sync::{Mutex, OnceLock};
use thiserror::Error;

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
    #[error("GStreamer playback error: {source} (debug: {debug:?})")]
    Pipeline {
        #[source]
        source: gst::glib::Error,
        debug: Option<String>,
    },
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
    video_attached: bool,
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

        let video_output = gst::Bin::with_name("qt-video-output");
        video_output
            .add_many([
                &video_process,
                &video_queue,
                &gl_upload,
                &gl_convert,
                &rgba_filter,
                &video_sink,
            ])
            .map_err(PlaybackError::VideoOutputAssembly)?;
        gst::Element::link_many([
            &video_process,
            &video_queue,
            &gl_upload,
            &gl_convert,
            &rgba_filter,
            &video_sink,
        ])
        .map_err(PlaybackError::VideoOutputLink)?;
        let sink_pad = video_process
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
        Ok(Self {
            playbin,
            video_sink,
            video_attached: false,
        })
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

    pub fn play_service(&self, server: &str, service_id: u64) -> Result<(), PlaybackError> {
        if !self.video_attached {
            return Err(PlaybackError::VideoItemNotAttached);
        }
        let url = service_stream_url(server, service_id)?;
        if self.playbin.current_state() == gst::State::Null {
            self.prepare_video_sink()?;
        }
        self.playbin
            .set_state(gst::State::Ready)
            .map_err(|source| PlaybackError::StateChange {
                operation: "reset pipeline",
                source,
            })?;
        self.playbin.set_property("uri", url);
        self.playbin
            .set_state(gst::State::Playing)
            .map_err(|source| PlaybackError::StateChange {
                operation: "start playback",
                source,
            })?;
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
            })
    }

    pub fn set_paused(&self, paused: bool) -> Result<(), PlaybackError> {
        let state = if paused {
            gst::State::Paused
        } else {
            gst::State::Playing
        };
        self.playbin
            .set_state(state)
            .map(|_| ())
            .map_err(|source| PlaybackError::StateChange {
                operation: if paused {
                    "pause playback"
                } else {
                    "resume playback"
                },
                source,
            })
    }

    pub fn set_volume(&self, volume: f64) {
        self.playbin
            .set_property("volume", volume.clamp(0.0, 100.0) / 100.0);
    }

    pub fn drain_events(&self) -> Result<PlaybackEvent, PlaybackError> {
        let Some(bus) = self.playbin.bus() else {
            return Err(PlaybackError::BusUnavailable);
        };
        let mut outcome = PlaybackEvent::None;
        while let Some(message) = bus.pop() {
            use gst::MessageView;
            outcome = match message.view() {
                MessageView::Error(error) => {
                    return Err(PlaybackError::Pipeline {
                        source: error.error(),
                        debug: error.debug().map(|debug| debug.to_string()),
                    });
                }
                MessageView::Eos(..) => PlaybackEvent::Ended,
                MessageView::StateChanged(state)
                    if state.src() == Some(self.playbin.upcast_ref())
                        && state.current() == gst::State::Playing =>
                {
                    PlaybackEvent::Playing
                }
                _ => outcome,
            };
        }
        Ok(outcome)
    }
}

impl Drop for Playback {
    fn drop(&mut self) {
        if let Err(error) = self.playbin.set_state(gst::State::Null) {
            eprintln!("Could not stop playback while dropping: {error}");
        }
        if let Err(error) = self.video_sink.set_state(gst::State::Null) {
            eprintln!("Could not stop the video sink while dropping: {error}");
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
    use super::{DeinterlaceMode, PlaybackError, service_stream_url};
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
