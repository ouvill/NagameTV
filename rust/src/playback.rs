use gstreamer::{self as gst, prelude::*};
use std::cell::RefCell;
use std::sync::{Mutex, OnceLock};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
static PRELOADED: OnceLock<Mutex<Option<Playback>>> = OnceLock::new();

pub fn preload() -> Result<()> {
    PRELOADED
        .set(Mutex::new(Some(Playback::new()?)))
        .map_err(|_| "Playback already initialized")?;
    Ok(())
}

pub fn take_preloaded() -> Option<Playback> {
    PRELOADED.get()?.lock().ok()?.take()
}

pub struct Playback {
    playbin: gst::Element,
    sink: gst::Element,
    attached: bool,
    requested_uri: RefCell<Option<String>>,
}

impl Playback {
    pub fn element(&self) -> &gst::Element {
        &self.playbin
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
        let deinterlace = gst::ElementFactory::make("deinterlace").build()?;
        deinterlace.set_property_from_str("method", "yadif");
        deinterlace.set_property_from_str("mode", "auto");
        deinterlace.set_property_from_str("fields", "all");
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
            gst::GhostPad::with_target(&input.static_pad("sink").ok_or("Missing video sink pad")?)?;
        pad.set_active(true)?;
        output.add_pad(&pad)?;
        let audio = gst::ElementFactory::make("pulsesink")
            .property("enable-last-sample", false)
            .build()?;
        let playbin = gst::ElementFactory::make("playbin3").build()?;
        playbin.set_property_from_str("flags", "video+audio+soft-volume+buffering+native-video");
        playbin.set_property("video-sink", &output);
        playbin.set_property("audio-sink", &audio);
        playbin.set_property("volume", 0.5_f64);
        playbin.connect("source-setup", false, |values| {
            if let Some(source) = values.get(1).and_then(|v| v.get::<gst::Element>().ok()) {
                if source.factory().is_some_and(|f| f.name() == "souphttpsrc") {
                    source.set_property("timeout", 15_u32);
                    source.set_property("retries", 0_i32);
                }
            }
            None
        });
        Ok(Self {
            playbin,
            sink,
            attached: false,
            requested_uri: RefCell::new(None),
        })
    }

    /// The GUI owns the live QQuickItem until shutdown has stopped the sink.
    pub unsafe fn attach(&mut self, item: usize) -> Result<()> {
        if item == 0 {
            return Err("Missing Qt video item".into());
        }
        self.sink
            .set_property("widget", item as *mut std::ffi::c_void);
        self.attached = true;
        Ok(())
    }

    /// Returns false when this stream is already connecting or playing.
    pub fn play(&self, server: &str, service: u64) -> Result<bool> {
        if !self.attached {
            return Err("Video output is not ready".into());
        }
        let uri = format!("{server}/api/services/{service}/stream");
        if self.requested_uri.borrow().as_ref() == Some(&uri) {
            return Ok(false);
        }
        self.stop()?;
        // Qt must supply the GL display before any other GL element starts.
        self.sink.set_state(gst::State::Ready)?;
        self.playbin.set_property("uri", &uri);
        self.playbin.set_state(gst::State::Playing)?;
        *self.requested_uri.borrow_mut() = Some(uri);
        eprintln!("Starting service {service}");
        Ok(true)
    }

    pub fn stop(&self) -> Result<()> {
        stop_stream(&self.playbin)?;
        *self.requested_uri.borrow_mut() = None;
        Ok(())
    }

    pub fn set_volume(&self, value: f64) {
        if value.is_finite() {
            self.playbin.set_property("volume", value.clamp(0.0, 1.0));
        }
    }

    pub fn poll(&self) -> Result<bool> {
        let bus = self.playbin.bus().ok_or("Missing GStreamer bus")?;
        let mut playing = false;
        let mut failure = None;
        while let Some(message) = bus.pop() {
            match message.view() {
                gst::MessageView::Error(e) => {
                    if failure.is_none() {
                        failure = Some(format!("{} ({:?})", e.error(), e.debug()));
                    }
                }
                gst::MessageView::Eos(_) => {
                    failure.get_or_insert("配信が終了しました".into());
                }
                gst::MessageView::StateChanged(s) if s.src() == Some(self.playbin.upcast_ref()) => {
                    playing |= s.current() == gst::State::Playing;
                }
                _ => {}
            }
        }
        if let Some(error) = failure {
            return Err(error.into());
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
        let _ = self.sink.set_state(gst::State::Null);
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
    let bus = playbin.bus().ok_or("Missing GStreamer bus")?;
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
