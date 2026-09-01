use gst::prelude::*;
use gstreamer as gst;
use std::ffi::{CStr, c_char, c_void};
use std::ptr;

#[link(name = "gobject-2.0")]
unsafe extern "C" {
    fn g_object_set(object: *mut c_void, first_property_name: *const c_char, ...);
}

pub struct MirakurunPlayer {
    playbin: gst::Element,
    video_sink: gst::Element,
    video_attached: bool,
}

impl MirakurunPlayer {
    fn new() -> Result<Self, String> {
        gst::init().map_err(|error| format!("Could not initialize GStreamer: {error}"))?;

        // Creating this element before QML is loaded registers GstGLQt6VideoItem.
        // It also supplies Qt's GL display to the rest of the pipeline.
        let video_sink = gst::ElementFactory::make("qml6glsink")
            .name("qt-video-sink")
            .build()
            .map_err(|error| format!("Could not create qml6glsink: {error}"))?;
        video_sink.set_property("enable-last-sample", false);

        let gl_upload = gst::ElementFactory::make("glupload")
            .build()
            .map_err(|error| format!("Could not create glupload: {error}"))?;
        let gl_convert = gst::ElementFactory::make("glcolorconvert")
            .build()
            .map_err(|error| format!("Could not create glcolorconvert: {error}"))?;
        let rgba_filter = gst::ElementFactory::make("capsfilter")
            .build()
            .map_err(|error| format!("Could not create the RGBA filter: {error}"))?;
        let rgba_caps = gst::Caps::builder("video/x-raw")
            .features(["memory:GLMemory"])
            .field("format", "RGBA")
            .field("texture-target", "2D")
            .build();
        rgba_filter.set_property("caps", &rgba_caps);
        let video_output = gst::Bin::with_name("qt-video-output");
        video_output
            .add_many([&gl_upload, &gl_convert, &rgba_filter, &video_sink])
            .map_err(|error| format!("Could not assemble the video output: {error}"))?;
        gst::Element::link_many([&gl_upload, &gl_convert, &rgba_filter, &video_sink])
            .map_err(|error| format!("Could not link the video output: {error}"))?;
        let upload_sink_pad = gl_upload
            .static_pad("sink")
            .ok_or_else(|| "glupload has no sink pad".to_owned())?;
        let ghost_pad = gst::GhostPad::with_target(&upload_sink_pad)
            .map_err(|error| format!("Could not expose the video sink pad: {error}"))?;
        ghost_pad
            .set_active(true)
            .map_err(|error| format!("Could not activate the video sink pad: {error}"))?;
        video_output
            .add_pad(&ghost_pad)
            .map_err(|error| format!("Could not add the video sink pad: {error}"))?;

        let playbin = gst::ElementFactory::make("playbin3")
            .name("mirakurun-player")
            .build()
            .map_err(|error| format!("Could not create playbin3: {error}"))?;
        playbin.set_property("video-sink", &video_output);
        playbin.set_property("volume", 0.7_f64);

        Ok(Self {
            playbin,
            video_sink,
            video_attached: false,
        })
    }

    fn attach_video_item(&mut self, widget: *mut c_void) -> Result<(), String> {
        if widget.is_null() {
            return Err("The video item is null".to_owned());
        }
        unsafe {
            g_object_set(
                self.video_sink.as_ptr().cast(),
                c"widget".as_ptr(),
                widget,
                ptr::null::<c_char>(),
            );
        }
        self.video_attached = true;
        Ok(())
    }

    fn play_service(&self, server: &str, service_id: u64) -> Result<(), String> {
        if !self.video_attached {
            return Err("The Qt video item is not attached".to_owned());
        }
        let url = service_stream_url(server, service_id)?;
        self.playbin
            .set_state(gst::State::Ready)
            .map_err(|error| format!("Could not reset the pipeline: {error:?}"))?;
        self.playbin.set_property("uri", &url);
        self.playbin
            .set_state(gst::State::Playing)
            .map_err(|error| format!("Could not start playback: {error:?}"))?;
        Ok(())
    }

    fn stop(&self) {
        let _ = self.playbin.set_state(gst::State::Null);
    }

    fn set_paused(&self, paused: bool) {
        let state = if paused {
            gst::State::Paused
        } else {
            gst::State::Playing
        };
        let _ = self.playbin.set_state(state);
    }

    fn set_volume(&self, volume: f64) {
        self.playbin
            .set_property("volume", volume.clamp(0.0, 100.0) / 100.0);
    }

    fn drain_events(&self) -> i32 {
        let Some(bus) = self.playbin.bus() else {
            return 3;
        };
        let mut outcome = 0;
        while let Some(message) = bus.pop() {
            use gst::MessageView;
            outcome = match message.view() {
                MessageView::Error(error) => {
                    eprintln!(
                        "GStreamer error from {}: {} ({:?})",
                        error
                            .src()
                            .map(|source| source.path_string())
                            .unwrap_or_default(),
                        error.error(),
                        error.debug()
                    );
                    3
                }
                MessageView::Eos(..) => 2,
                MessageView::StateChanged(state) if state.current() == gst::State::Playing => 1,
                _ => outcome,
            };
        }
        outcome
    }
}

impl Drop for MirakurunPlayer {
    fn drop(&mut self) {
        let _ = self.playbin.set_state(gst::State::Null);
        let _ = self.video_sink.set_state(gst::State::Null);
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

fn service_stream_url(server: &str, service_id: u64) -> Result<String, String> {
    let server = server.trim().trim_end_matches('/');
    if !(server.starts_with("http://") || server.starts_with("https://")) {
        return Err("The server URL must start with http:// or https://".to_owned());
    }
    if service_id == 0 {
        return Err("The service ID must be greater than zero".to_owned());
    }
    Ok(format!("{server}/api/services/{service_id}/stream"))
}

unsafe fn input_string<'a>(value: *const c_char) -> Result<&'a str, String> {
    unsafe {
        if value.is_null() {
            return Err("Received a null string".to_owned());
        }
        CStr::from_ptr(value)
            .to_str()
            .map_err(|_| "The string is not valid UTF-8".to_owned())
    }
}

unsafe fn write_error(buffer: *mut c_char, capacity: usize, message: &str) {
    unsafe {
        if buffer.is_null() || capacity == 0 {
            return;
        }
        let bytes = message.as_bytes();
        let length = bytes.len().min(capacity - 1);
        ptr::copy_nonoverlapping(bytes.as_ptr(), buffer.cast(), length);
        *buffer.add(length) = 0;
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// `error` must be writable for `capacity` bytes when it is non-null.
pub unsafe extern "C" fn mirakurun_player_create(
    error: *mut c_char,
    capacity: usize,
) -> *mut MirakurunPlayer {
    unsafe {
        match MirakurunPlayer::new() {
            Ok(player) => Box::into_raw(Box::new(player)),
            Err(message) => {
                write_error(error, capacity, &message);
                ptr::null_mut()
            }
        }
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// `player` must be null or a pointer returned by `mirakurun_player_create` that has not been freed.
pub unsafe extern "C" fn mirakurun_player_destroy(player: *mut MirakurunPlayer) {
    unsafe {
        if !player.is_null() {
            drop(Box::from_raw(player));
        }
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// `player` and `widget` must remain valid for the call; `error` follows the create contract.
pub unsafe extern "C" fn mirakurun_player_attach_video_item(
    player: *mut MirakurunPlayer,
    widget: *mut c_void,
    error: *mut c_char,
    capacity: usize,
) -> bool {
    unsafe {
        let result = player
            .as_mut()
            .ok_or_else(|| "The player is null".to_owned())
            .and_then(|player| player.attach_video_item(widget));
        match result {
            Ok(()) => true,
            Err(message) => {
                write_error(error, capacity, &message);
                false
            }
        }
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// All pointers must be valid for the call; `server` must point to a NUL-terminated UTF-8 string.
pub unsafe extern "C" fn mirakurun_player_play_service(
    player: *mut MirakurunPlayer,
    server: *const c_char,
    service_id: u64,
    error: *mut c_char,
    capacity: usize,
) -> bool {
    unsafe {
        let result = player
            .as_ref()
            .ok_or_else(|| "The player is null".to_owned())
            .and_then(|player| player.play_service(input_string(server)?, service_id));
        match result {
            Ok(()) => true,
            Err(message) => {
                write_error(error, capacity, &message);
                false
            }
        }
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// `player` must be null or point to a live player.
pub unsafe extern "C" fn mirakurun_player_stop(player: *mut MirakurunPlayer) {
    unsafe {
        if let Some(player) = player.as_ref() {
            player.stop();
        }
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// `player` must be null or point to a live player.
pub unsafe extern "C" fn mirakurun_player_set_pause(player: *mut MirakurunPlayer, paused: bool) {
    unsafe {
        if let Some(player) = player.as_ref() {
            player.set_paused(paused);
        }
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// `player` must be null or point to a live player.
pub unsafe extern "C" fn mirakurun_player_set_volume(player: *mut MirakurunPlayer, volume: f64) {
    unsafe {
        if let Some(player) = player.as_ref() {
            player.set_volume(volume);
        }
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// `player` must be null or point to a live player.
pub unsafe extern "C" fn mirakurun_player_drain_events(player: *mut MirakurunPlayer) -> i32 {
    unsafe { player.as_ref().map_or(3, MirakurunPlayer::drain_events) }
}

#[cfg(test)]
mod tests {
    use super::service_stream_url;

    #[test]
    fn builds_mirakurun_service_url() {
        assert_eq!(
            service_stream_url("http://192.168.3.3:40772/", 3_203_246_080).unwrap(),
            "http://192.168.3.3:40772/api/services/3203246080/stream"
        );
    }

    #[test]
    fn rejects_unsafe_scheme_and_zero_id() {
        assert!(service_stream_url("ftp://example.test", 1).is_err());
        assert!(service_stream_url("http://example.test", 0).is_err());
    }
}
