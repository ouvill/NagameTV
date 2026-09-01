use gst::prelude::*;
use gstreamer as gst;
use std::ffi::{c_char, c_void};
use std::ptr;
use std::sync::{Mutex, OnceLock};

static PRELOADED: OnceLock<Mutex<Option<Playback>>> = OnceLock::new();

pub fn preload() -> Result<(), String> {
    let playback = Playback::new()?;
    PRELOADED
        .set(Mutex::new(Some(playback)))
        .map_err(|_| "Playback was already preloaded".to_owned())
}

pub fn take_preloaded() -> Result<Playback, String> {
    PRELOADED
        .get()
        .and_then(|slot| slot.lock().ok()?.take())
        .ok_or_else(|| "The preloaded playback pipeline is unavailable".to_owned())
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
    fn from_environment() -> Result<Self, String> {
        let value = std::env::var("MIRAKURUN_DEINTERLACE").unwrap_or_else(|_| "yadif".into());
        Self::parse(&value)
    }

    fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "yadif" | "quality" => Ok(Self::Yadif),
            "linear" | "balanced" => Ok(Self::Linear),
            "off" | "disabled" => Ok(Self::Off),
            _ => Err(format!(
                "Invalid MIRAKURUN_DEINTERLACE value '{value}'; use yadif, linear, or off"
            )),
        }
    }
}

impl Playback {
    pub fn new() -> Result<Self, String> {
        gst::init().map_err(|error| format!("Could not initialize GStreamer: {error}"))?;
        let video_sink = gst::ElementFactory::make("qml6glsink")
            .name("qt-video-sink")
            .build()
            .map_err(|error| format!("Could not create qml6glsink: {error}"))?;
        video_sink.set_property("enable-last-sample", false);

        let deinterlace_mode = DeinterlaceMode::from_environment()?;
        let video_process = match deinterlace_mode {
            DeinterlaceMode::Off => gst::ElementFactory::make("identity")
                .name("deinterlace-disabled")
                .build()
                .map_err(|error| format!("Could not create video passthrough: {error}"))?,
            mode => {
                let element = gst::ElementFactory::make("deinterlace")
                    .name("deinterlace")
                    .build()
                    .map_err(|error| format!("Could not create deinterlace: {error}"))?;
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
            .map_err(|error| format!("Could not create video queue: {error}"))?;
        video_queue.set_property("max-size-buffers", 8_u32);
        video_queue.set_property("max-size-bytes", 0_u32);
        video_queue.set_property("max-size-time", 0_u64);

        let gl_upload = gst::ElementFactory::make("glupload")
            .build()
            .map_err(|error| format!("Could not create glupload: {error}"))?;
        let gl_convert = gst::ElementFactory::make("glcolorconvert")
            .build()
            .map_err(|error| format!("Could not create glcolorconvert: {error}"))?;
        let rgba_filter = gst::ElementFactory::make("capsfilter")
            .build()
            .map_err(|error| format!("Could not create RGBA filter: {error}"))?;
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
            .map_err(|error| format!("Could not assemble video output: {error}"))?;
        gst::Element::link_many([
            &video_process,
            &video_queue,
            &gl_upload,
            &gl_convert,
            &rgba_filter,
            &video_sink,
        ])
        .map_err(|error| format!("Could not link video output: {error}"))?;
        let sink_pad = video_process
            .static_pad("sink")
            .ok_or_else(|| "Video processor has no sink pad".to_owned())?;
        let ghost_pad = gst::GhostPad::with_target(&sink_pad)
            .map_err(|error| format!("Could not expose video sink pad: {error}"))?;
        ghost_pad
            .set_active(true)
            .map_err(|error| format!("Could not activate video sink pad: {error}"))?;
        video_output
            .add_pad(&ghost_pad)
            .map_err(|error| format!("Could not add video sink pad: {error}"))?;

        let playbin = gst::ElementFactory::make("playbin3")
            .name("mirakurun-player")
            .build()
            .map_err(|error| format!("Could not create playbin3: {error}"))?;
        playbin.set_property("video-sink", &video_output);
        if std::env::var("MIRAKURUN_AUDIO_SINK").is_ok_and(|value| value == "fakesink") {
            let audio_sink = gst::ElementFactory::make("fakesink")
                .name("isolated-test-audio-sink")
                .build()
                .map_err(|error| format!("Could not create test audio sink: {error}"))?;
            audio_sink.set_property("enable-last-sample", false);
            audio_sink.set_property("sync", true);
            playbin.set_property("audio-sink", &audio_sink);
        }
        playbin.set_property("volume", 0.7_f64);
        Ok(Self {
            playbin,
            video_sink,
            video_attached: false,
        })
    }

    pub fn attach_video_item(&mut self, widget: *mut c_void) -> Result<(), String> {
        if widget.is_null() {
            return Err("The video item is null".into());
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

    pub fn play_service(&self, server: &str, service_id: u64) -> Result<(), String> {
        if !self.video_attached {
            return Err("The Qt video item is not attached".into());
        }
        let url = service_stream_url(server, service_id)?;
        self.playbin
            .set_state(gst::State::Ready)
            .map_err(|error| format!("Could not reset pipeline: {error:?}"))?;
        self.playbin.set_property("uri", url);
        self.playbin
            .set_state(gst::State::Playing)
            .map_err(|error| format!("Could not start playback: {error:?}"))?;
        Ok(())
    }

    pub fn stop(&self) {
        let _ = self.playbin.set_state(gst::State::Null);
    }

    pub fn set_paused(&self, paused: bool) {
        let state = if paused {
            gst::State::Paused
        } else {
            gst::State::Playing
        };
        let _ = self.playbin.set_state(state);
    }

    pub fn set_volume(&self, volume: f64) {
        self.playbin
            .set_property("volume", volume.clamp(0.0, 100.0) / 100.0);
    }

    pub fn drain_events(&self) -> PlaybackEvent {
        let Some(bus) = self.playbin.bus() else {
            return PlaybackEvent::Error;
        };
        let mut outcome = PlaybackEvent::None;
        while let Some(message) = bus.pop() {
            use gst::MessageView;
            outcome = match message.view() {
                MessageView::Error(error) => {
                    eprintln!("GStreamer error: {} ({:?})", error.error(), error.debug());
                    PlaybackEvent::Error
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
        outcome
    }
}

impl Drop for Playback {
    fn drop(&mut self) {
        let _ = self.playbin.set_state(gst::State::Null);
        let _ = self.video_sink.set_state(gst::State::Null);
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
    Error,
}

fn service_stream_url(server: &str, service_id: u64) -> Result<String, String> {
    let server = server.trim().trim_end_matches('/');
    if !(server.starts_with("http://") || server.starts_with("https://")) {
        return Err("The server URL must start with http:// or https://".into());
    }
    if service_id == 0 {
        return Err("The service ID must be greater than zero".into());
    }
    Ok(format!("{server}/api/services/{service_id}/stream"))
}

#[cfg(test)]
mod tests {
    use super::{DeinterlaceMode, service_stream_url};
    #[test]
    fn builds_service_url() {
        assert_eq!(
            service_stream_url("http://192.168.3.3:40772/", 3_203_246_080).unwrap(),
            "http://192.168.3.3:40772/api/services/3203246080/stream"
        );
    }
    #[test]
    fn validates_inputs() {
        assert!(service_stream_url("ftp://example.test", 1).is_err());
        assert!(service_stream_url("http://example.test", 0).is_err());
        assert_eq!(
            DeinterlaceMode::parse("YADIF").unwrap(),
            DeinterlaceMode::Yadif
        );
        assert_eq!(
            DeinterlaceMode::parse("balanced").unwrap(),
            DeinterlaceMode::Linear
        );
        assert!(DeinterlaceMode::parse("unknown").is_err());
    }
}
