//! Byte-addressed input for demuxers. Reuses verified HTTP ranges and leaves
//! container timestamps, duration and TIME seeking to GStreamer.
use super::recording::{MediaFile, source::Reader};
use crate::features::subscriptions::Subscriptions;
use gstreamer::{self as gst, prelude::*};
use gstreamer_app::{AppSrc, AppSrcCallbacks, AppStreamType};
use std::{
    io::{Read, Seek, SeekFrom},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

const QUEUE_BYTES: u64 = 512 * 1024;
const DEFAULT_READ_BYTES: u32 = 64 * 1024;
const MAX_READ_BYTES: u32 = 32 * 1024 * 1024;
const NO_SEEK: u64 = u64::MAX;

pub(super) struct Input {
    subscriptions: Subscriptions,
    interrupted: Arc<AtomicBool>,
    sources: Arc<Mutex<Vec<gst::glib::WeakRef<AppSrc>>>>,
}
impl Input {
    pub fn new(playbin: &gst::Element, file: &MediaFile) -> Self {
        let subscriptions = Subscriptions::default();
        let interrupted = Arc::new(AtomicBool::new(false));
        let sources = Arc::new(Mutex::new(Vec::new()));
        let installed = sources.clone();
        let stopped = interrupted.clone();
        let file = file.clone();
        let registrations = subscriptions.clone();
        let id = playbin.connect("source-setup", false, move |values| {
            if let Some(source) = values
                .get(1)
                .and_then(|value| value.get::<gst::Element>().ok())
                .and_then(|element| element.downcast::<AppSrc>().ok())
            {
                if let Ok(mut sources) = installed.lock() {
                    sources.retain(|s: &gst::glib::WeakRef<AppSrc>| s.upgrade().is_some());
                    sources.push(source.downgrade());
                }
                configure(&source, &file, stopped.clone(), &registrations);
            }
            None
        });
        subscriptions.signal(playbin, id);
        Self {
            subscriptions,
            interrupted,
            sources,
        }
    }
    pub fn suspend(&self, suspended: bool) {
        self.interrupted.store(suspended, Ordering::Release);
    }
}
impl Drop for Input {
    fn drop(&mut self) {
        self.suspend(true);
        self.subscriptions.close();
        // READY may retain appsrc; release readers and authenticated clients now.
        if let Ok(mut sources) = self.sources.lock() {
            for source in sources.drain(..).filter_map(|s| s.upgrade()) {
                source.set_callbacks(AppSrcCallbacks::builder().build());
            }
        }
    }
}

fn configure(
    source: &AppSrc,
    file: &MediaFile,
    interrupted: Arc<AtomicBool>,
    registrations: &Subscriptions,
) {
    if let Some(pad) = source.static_pad("src") {
        // In pull mode the demuxer owns TIME segments. appsrc nevertheless emits
        // a default BYTES segment after a flushing seek; qtdemux interprets it as
        // a push-mode seek response and resets the requested interval to zero.
        // Suppress only that redundant segment; push-mode byte segments still
        // describe upstream byte seeks and must reach the demuxer.
        let id = pad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, |pad, info| {
            if pad.mode() == gst::PadMode::Pull && info.event().is_some_and(|event| {
                matches!(event.view(), gst::EventView::Segment(segment) if segment.segment().format() == gst::Format::Bytes)
            }) { gst::PadProbeReturn::Drop } else { gst::PadProbeReturn::Ok }
        });
        registrations.probe(&pad, id);
    }
    source.set_format(gst::Format::Bytes);
    source.set_stream_type(AppStreamType::RandomAccess);
    source.set_size(i64::try_from(file.size()).expect("inspection bounds resource size"));
    source.set_max_bytes(QUEUE_BYTES);
    // Let urisourcebin typefind the byte source, as it does for filesrc.
    // Supplying container caps here makes pull-mode typefind report the type
    // twice (upstream CAPS plus its own result), creating two parsebins.
    source.set_caps(None);
    let reader = Mutex::new(file.source().reader(interrupted.clone()));
    let target = Arc::new(AtomicU64::new(0));
    let seek_target = target.clone();
    let size = file.size();
    source.set_callbacks(
        AppSrcCallbacks::builder()
            .seek_data(move |_, offset| {
                if offset > size {
                    return false;
                }
                // The GUI can request a seek while a range read is pending. Never do
                // I/O or lock the reader here; the streaming task applies the request.
                seek_target.store(offset, Ordering::Release);
                true
            })
            .need_data(move |source, requested| {
                if interrupted.load(Ordering::Acquire) {
                    return;
                }
                let requested = if requested == u32::MAX {
                    DEFAULT_READ_BYTES
                } else {
                    requested
                };
                let result = (|| -> std::io::Result<Option<gst::Buffer>> {
                    if requested > MAX_READ_BYTES {
                        return Err(std::io::Error::other(
                            "Container requested an oversized read",
                        ));
                    }
                    let mut reader = reader
                        .lock()
                        .map_err(|_| std::io::Error::other("Media reader lock poisoned"))?;
                    let offset = target.swap(NO_SEEK, Ordering::AcqRel);
                    if offset != NO_SEEK {
                        reader.seek(SeekFrom::Start(offset))?;
                    }
                    read_buffer(&mut reader, requested, size, &interrupted)
                })();
                if interrupted.load(Ordering::Acquire) {
                    return;
                }
                match result {
                    Ok(Some(buffer)) => {
                        let _ = source.push_buffer(buffer);
                    }
                    Ok(None) => {
                        let _ = source.end_of_stream();
                    }
                    Err(error) => gst::element_error!(
                        source,
                        gst::ResourceError::Read,
                        ("Could not read the video file: {error}")
                    ),
                }
            })
            .build(),
    );
}

fn read_buffer(
    reader: &mut Reader,
    requested: u32,
    size: u64,
    interrupted: &AtomicBool,
) -> std::io::Result<Option<gst::Buffer>> {
    let offset = reader.stream_position()?;
    let length = (u64::from(requested)).min(size.saturating_sub(offset)) as usize;
    if length == 0 {
        return Ok(None);
    }
    let mut bytes = vec![0; length];
    let mut filled = 0;
    while filled < length {
        if interrupted.load(Ordering::Acquire) {
            return Err(std::io::ErrorKind::Interrupted.into());
        }
        let count = match reader.read(&mut bytes[filled..]) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if count == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        filled += count;
    }
    let mut buffer = gst::Buffer::from_mut_slice(bytes);
    let writable = buffer.get_mut().expect("new media buffer");
    writable.set_offset(offset);
    writable.set_offset_end(offset + length as u64);
    Ok(Some(buffer))
}

#[cfg(test)]
mod tests;
