use super::Input;
use gstreamer::{self as gst, prelude::*};
use gstreamer_app::{AppSink, AppSrc, AppSrcCallbacks};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const PREVIEW_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: gst::ClockTime = gst::ClockTime::from_mseconds(25);
const SOURCE_QUEUE_BYTES: u64 = 96 * 1024;

#[derive(Debug, thiserror::Error)]
pub(super) enum Error {
    #[error("Preview was cancelled")]
    Cancelled,
    #[error("Preview data is no longer available")]
    Unavailable,
    #[error("Preview decoding timed out")]
    TimedOut,
    #[error("Preview pipeline: {0}")]
    Operation(#[from] gst::glib::BoolError),
    #[error("Preview state change: {0}")]
    State(#[from] gst::StateChangeError),
    #[error("Preview decoder: {0}")]
    Pipeline(#[from] gst::glib::Error),
    #[error("Preview input: {0}")]
    Input(#[source] crate::playback::input::Error),
}

enum Attached {
    Transport(Arc<Mutex<Option<AppSrc>>>),
    Media(crate::playback::media::Input),
}
struct Decoder {
    playbin: gst::Element,
    attached: Option<Attached>,
    cancelled: Arc<AtomicBool>,
}
impl Drop for Decoder {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        if let Some(Attached::Media(input)) = &self.attached {
            input.suspend(true);
        }
        if let Err(error) = self.playbin.set_state(gst::State::Null) {
            tracing::warn!(
                error = &error as &dyn std::error::Error,
                "Stopping preview decoder failed"
            );
            // Retain the input together with the pipeline if native shutdown
            // failed: callbacks must not outlive their authenticated readers.
            std::mem::forget((self.playbin.clone(), self.attached.take()));
            return;
        }
        if let Some(Attached::Transport(source)) = &self.attached {
            match source.lock() {
                Ok(mut source) => {
                    if let Some(source) = source.take() {
                        source.set_callbacks(AppSrcCallbacks::builder().build());
                    }
                }
                Err(error) => tracing::warn!(%error, "Releasing preview input failed"),
            }
        }
    }
}

pub(super) fn capture(
    input: Input,
    target_ms: u64,
    cancelled: Arc<AtomicBool>,
) -> Result<String, Error> {
    if cancelled.load(Ordering::Acquire) {
        return Err(Error::Cancelled);
    }
    let playbin = gst::ElementFactory::make("playbin").build()?;
    // Thumbnails deliberately use a CPU decoder and an in-memory image sink.
    // This pipeline never opens a display/audio device or falls back from a GPU.
    playbin.set_property_from_str("flags", "video+force-sw-decoders");
    let policy = crate::playback::resources::DecoderPolicy::preview();
    playbin.connect("element-setup", false, move |values| {
        policy.configure(&values[1].get::<gst::Element>().expect("playbin element"));
        None
    });
    let sink = gst::parse::bin_from_description(
        "videoconvert ! videoscale add-borders=true ! video/x-raw,format=RGB,width=240,height=135,pixel-aspect-ratio=1/1 ! pngenc ! appsink name=preview sync=false max-buffers=1 wait-on-eos=false",
        true,
    )?;
    let appsink = sink
        .by_name("preview")
        .expect("declared preview sink")
        .downcast::<AppSink>()
        .expect("appsink type");
    playbin.set_property("video-sink", &sink);
    let media = matches!(&input, Input::Media(_));
    let attached = match input {
        Input::Media(file) => Attached::Media(crate::playback::media::Input::new(&playbin, &file)),
        Input::Transport(source) => {
            let cursor = source
                .open(target_ms.saturating_mul(1_000_000), cancelled.clone())
                .map_err(|error| match error {
                    crate::playback::input::Error::Cancelled => Error::Cancelled,
                    crate::playback::input::Error::Expired
                    | crate::playback::input::Error::Unindexed => Error::Unavailable,
                    error => Error::Input(error),
                })?;
            let cursor = Arc::new(Mutex::new(cursor));
            let installed = Arc::new(Mutex::new(None));
            let installed_source = installed.clone();
            let cancellation = cancelled.clone();
            playbin.connect("source-setup", false, move |values| {
                let source = values[1].get::<AppSrc>().expect("appsrc URI source");
                source.set_format(gst::Format::Bytes);
                source.set_max_bytes(SOURCE_QUEUE_BYTES);
                source.set_caps(Some(
                    &gst::Caps::builder("video/mpegts")
                        .field("systemstream", true)
                        .field("packetsize", crate::transport::wire::TS_PACKET_SIZE as i32)
                        .build(),
                ));
                let cursor = cursor.clone();
                let cancellation = cancellation.clone();
                source.set_callbacks(
                    AppSrcCallbacks::builder()
                        .need_data(move |source, _| {
                            if cancellation.load(Ordering::Acquire) {
                                return;
                            }
                            let result = cursor
                                .lock()
                                .map_err(|_| crate::playback::input::Error::Poisoned)
                                .and_then(|mut cursor| cursor.next());
                            let result = match result {
                                Ok(Some(bytes)) => {
                                    source.push_buffer(gst::Buffer::from_mut_slice(bytes))
                                }
                                Ok(None) | Err(crate::playback::input::Error::Expired) => {
                                    source.end_of_stream()
                                }
                                Err(crate::playback::input::Error::Cancelled) => return,
                                Err(error) => {
                                    gst::element_error!(
                                        source,
                                        gst::ResourceError::Read,
                                        ("Preview input: {error}")
                                    );
                                    return;
                                }
                            };
                            // Flushing/EOS are expected when the still has been captured or cancelled.
                            if let Err(error) = result
                                && !matches!(error, gst::FlowError::Flushing | gst::FlowError::Eos)
                            {
                                tracing::warn!(
                                    error = &error as &dyn std::error::Error,
                                    "Feeding seek preview failed"
                                );
                            }
                        })
                        .build(),
                );
                match installed_source.lock() {
                    Ok(mut installed) => *installed = Some(source),
                    Err(error) => tracing::warn!(%error, "Retaining preview source failed"),
                }
                None
            });
            Attached::Transport(installed)
        }
    };
    let decoder = Decoder {
        playbin,
        attached: Some(attached),
        cancelled,
    };
    decoder.playbin.set_property("uri", "appsrc://");
    decoder.playbin.set_state(gst::State::Paused)?;
    let deadline = Instant::now() + PREVIEW_TIMEOUT;
    let mut sample = preroll(&decoder, &appsink, deadline)?;
    if media && target_ms > 0 {
        decoder.playbin.seek_simple(
            gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
            gst::ClockTime::from_mseconds(target_ms),
        )?;
        sample = preroll(&decoder, &appsink, deadline)?;
    }
    let bytes = sample.buffer().ok_or(Error::Unavailable)?.map_readable()?;
    Ok(format!(
        "data:image/png;base64,{}",
        gst::glib::base64_encode(bytes.as_slice())
    ))
}

fn preroll(decoder: &Decoder, sink: &AppSink, deadline: Instant) -> Result<gst::Sample, Error> {
    let bus = decoder.playbin.bus().expect("playbin bus");
    loop {
        if decoder.cancelled.load(Ordering::Acquire) {
            return Err(Error::Cancelled);
        }
        if Instant::now() >= deadline {
            return Err(Error::TimedOut);
        }
        if let Some(sample) = sink.try_pull_preroll(POLL_INTERVAL) {
            return Ok(sample);
        }
        if let Some(message) = bus.pop_filtered(&[gst::MessageType::Error, gst::MessageType::Eos]) {
            return match message.view() {
                gst::MessageView::Error(error) => Err(Error::Pipeline(error.error())),
                _ => Err(Error::Unavailable),
            };
        }
    }
}
