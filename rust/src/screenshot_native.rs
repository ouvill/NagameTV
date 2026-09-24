//! Observe the actual buffer mapped by qml6glsink during Qt scene graph sync.
//! GstVideoMeta's public map callback is used; no private Qt/GStreamer C++ ABI
//! or decode-ahead last-sample is inspected. Pixels are downloaded only by a
//! screenshot worker, after a presented frame has been retained.
use super::{Error, overlay::Overlay};
use gstreamer::{
    self as gst,
    glib::{self, translate::*},
    prelude::*,
};
use gstreamer_video::{self as video, prelude::*};
use std::sync::{Arc, Mutex, OnceLock, Weak};

#[cxx::bridge(namespace = "viewer_screenshot")]
pub mod ffi {
    unsafe extern "C++" {
        include!("screenshot_observer.h");
        #[namespace = ""]
        type QQuickItem = crate::qt::ffi::QQuickItem;
        unsafe fn observePresentation(item: *mut QQuickItem, observer: Box<Observer>);
    }
    extern "Rust" {
        type Observer;
        fn before_sync(self: &Observer);
        fn after_sync(self: &Observer);
        fn presented(self: &Observer);
        fn invalidated(self: &Observer);
    }
}

#[derive(Clone)]
struct Frame {
    buffer: gst::Buffer,
    info: video::VideoInfo,
}
#[derive(Clone)]
pub struct Presented {
    frame: Frame,
    overlay: Arc<Overlay>,
}
impl Presented {
    pub fn bytes(&self) -> Result<usize, Error> {
        let (width, height) = self.output_size()?;
        (width as usize)
            .checked_mul(height as usize)
            .and_then(|p| p.checked_mul(8))
            .and_then(|size| size.checked_add(self.frame.info.size()))
            .and_then(|size| size.checked_add(self.overlay.media_caption_bytes()))
            .and_then(|size| {
                // A capture of NV12 additionally owns a temporary RGBA frame.
                if self.frame.info.format() == video::VideoFormat::Rgba {
                    return Some(size);
                }
                let pixels = (self.frame.info.width() as usize)
                    .checked_mul(self.frame.info.height() as usize)?;
                size.checked_add(pixels.checked_mul(4)?)
            })
            .ok_or(Error::Capacity)
    }
    fn output_size(&self) -> Result<(u32, u32), Error> {
        let info = &self.frame.info;
        let par = info.par();
        let width = (u64::from(info.width()) * par.numer() as u64 + par.denom() as u64 / 2)
            / par.denom() as u64;
        if width == 0 || width > i32::MAX as u64 {
            return Err(Error::Encode);
        }
        Ok((width as u32, info.height()))
    }
    pub fn image(self) -> Result<cxx_qt_lib::QImage, Error> {
        let (width, height) = self.output_size()?;
        let source =
            video::VideoFrameRef::from_buffer_ref_readable(&self.frame.buffer, &self.frame.info)
                .map_err(|_| Error::Encode)?;
        // Conversion happens only for an accepted capture, on the save worker.
        // GstVideoConverter applies negotiated range/matrix/chroma placement;
        // normal playback keeps both NV12 planes on the GPU.
        let converted = match source.format() {
            video::VideoFormat::Rgba => None,
            video::VideoFormat::Nv12 => Some(convert_to_rgba(&source)?),
            _ => return Err(Error::Encode),
        };
        let mapped = if let Some((buffer, info)) = &converted {
            video::VideoFrameRef::from_buffer_ref_readable(buffer, info)
                .map_err(|_| Error::Encode)?
        } else {
            source
        };
        let source_width = self.frame.info.width() as usize;
        let row_bytes = source_width.checked_mul(4).ok_or(Error::Encode)?;
        let stride = usize::try_from(mapped.plane_stride()[0]).map_err(|_| Error::Encode)?;
        let pixels = mapped.plane_data(0).map_err(|_| Error::Encode)?;
        let mut bytes = Vec::with_capacity(row_bytes * height as usize);
        for row in 0..height as usize {
            let start = row.checked_mul(stride).ok_or(Error::Encode)?;
            bytes.extend_from_slice(pixels.get(start..start + row_bytes).ok_or(Error::Encode)?);
        }
        drop(mapped);
        // SAFETY: tightly packed RGBA rows validated above, dimensions from the
        // same negotiated VideoInfo, owned Vec transferred to QImage's cleanup.
        let image = unsafe {
            cxx_qt_lib::QImage::from_raw_bytes(
                bytes,
                source_width as i32,
                height as i32,
                cxx_qt_lib::QImageFormat::Format_RGBA8888,
            )
        };
        super::overlay::compose(image, width, height, &self.overlay)
    }
    #[cfg(test)]
    fn pts(&self) -> Option<gst::ClockTime> {
        self.frame.buffer.pts()
    }
}

fn convert_to_rgba(
    source: &video::VideoFrameRef<&gst::BufferRef>,
) -> Result<(gst::Buffer, video::VideoInfo), Error> {
    let input = source.info();
    let info = video::VideoInfo::builder(video::VideoFormat::Rgba, input.width(), input.height())
        .fps(input.fps())
        .par(input.par())
        .interlace_mode(input.interlace_mode())
        .build()
        .map_err(|_| Error::Encode)?;
    let converter = video::VideoConverter::new(input, &info, None).map_err(|_| Error::Encode)?;
    let mut buffer = gst::Buffer::with_size(info.size()).map_err(|_| Error::Capacity)?;
    {
        let mut output = video::VideoFrameRef::from_buffer_ref_writable(buffer.make_mut(), &info)
            .map_err(|_| Error::Encode)?;
        converter.frame_ref(source, &mut output);
    }
    Ok((buffer, info))
}

#[derive(Default)]
struct Frames {
    generation: u64,
    staged: Option<Arc<Overlay>>,
    phase: RenderPhase,
    mapped: Option<Frame>,
    presented: Option<Presented>,
}
#[derive(Default)]
enum RenderPhase {
    #[default]
    Idle,
    Syncing {
        thread: std::thread::ThreadId,
        overlay: Option<Arc<Overlay>>,
    },
    Rendering(Option<Presented>),
}
#[derive(Clone, Default)]
pub struct Presentation(Arc<Mutex<Frames>>);
pub struct Observer(Presentation);
impl Presentation {
    pub fn observer(&self) -> Box<Observer> {
        Box::new(Observer(self.clone()))
    }
    pub fn stage(&self, overlay: Overlay) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).staged = Some(Arc::new(overlay));
    }
    pub fn capture(&self) -> Result<Presented, Error> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .presented
            .clone()
            .ok_or(Error::Unavailable)
    }
    pub fn clear(&self) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let generation = state.generation.wrapping_add(1);
        let retired = std::mem::replace(
            &mut *state,
            Frames {
                generation,
                ..Default::default()
            },
        );
        // Returning a native buffer to its pool can call other GL threads.
        // Never hold the presentation mutex across that resource release.
        drop(state);
        drop(retired);
    }
    pub fn install(&self, pad: &gst::Pad) {
        let tracker = self.clone();
        pad.add_probe(
            gst::PadProbeType::BUFFER
                | gst::PadProbeType::EVENT_DOWNSTREAM
                | gst::PadProbeType::EVENT_FLUSH,
            move |pad, probe| {
                if let Some(event) = probe.event()
                    && matches!(
                        event.view(),
                        gst::EventView::FlushStart(_) | gst::EventView::StreamStart(_)
                    )
                {
                    tracker.clear();
                }
                if let Some(buffer) = probe.buffer_mut() {
                    let Some(info) = pad
                        .current_caps()
                        .and_then(|caps| video::VideoInfo::from_caps(&caps).ok())
                    else {
                        return gst::PadProbeReturn::Ok;
                    };
                    if !matches!(
                        info.format(),
                        video::VideoFormat::Rgba | video::VideoFormat::Nv12
                    ) {
                        return gst::PadProbeReturn::Ok;
                    }
                    // Never replace the map callback in a reusable pool buffer.
                    // A metadata-only copy retains its parent until Qt and all
                    // accepted captures release it, keeping the texture stable.
                    let mut observed = buffer.copy();
                    gst::ParentBufferMeta::add(observed.make_mut(), buffer);
                    *buffer = observed;
                    let buffer = buffer.make_mut();
                    if buffer.meta::<video::VideoMeta>().is_none()
                        && video::VideoMeta::add_full(
                            buffer,
                            video::VideoFrameFlags::empty(),
                            info.format(),
                            info.width(),
                            info.height(),
                            info.offset(),
                            info.stride(),
                        )
                        .is_err()
                    {
                        return gst::PadProbeReturn::Ok;
                    }
                    let generation = tracker
                        .0
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .generation;
                    // SAFETY: the probe exclusively owns the writable BufferRef;
                    // both metadata objects belong to that exact GstBuffer. The
                    // callback delegates mapping/unmapping to the original meta.
                    unsafe {
                        install_map_observer(buffer, &tracker, info, generation);
                    }
                }
                gst::PadProbeReturn::Ok
            },
        );
    }
}
enum Phase {
    BeforeSync,
    AfterSync,
    Presented,
}
impl Observer {
    fn before_sync(&self) {
        self.phase(Phase::BeforeSync);
    }
    fn after_sync(&self) {
        self.phase(Phase::AfterSync);
    }
    fn presented(&self) {
        self.phase(Phase::Presented);
    }
    fn invalidated(&self) {
        self.0.clear();
    }
    fn phase(&self, phase: Phase) {
        let mut state = self.0.0.lock().unwrap_or_else(|e| e.into_inner());
        let old_phase = std::mem::take(&mut state.phase);
        let mut retired = None;
        match phase {
            Phase::BeforeSync => {
                state.phase = RenderPhase::Syncing {
                    thread: std::thread::current().id(),
                    overlay: state.staged.clone(),
                };
            }
            Phase::AfterSync => {
                let overlay = match &old_phase {
                    RenderPhase::Syncing { overlay, .. } => overlay.clone(),
                    RenderPhase::Idle | RenderPhase::Rendering(_) => None,
                };
                state.phase = RenderPhase::Rendering(
                    state
                        .mapped
                        .clone()
                        .zip(overlay)
                        .map(|(frame, overlay)| Presented { frame, overlay }),
                );
            }
            Phase::Presented => {
                if let RenderPhase::Rendering(frame) = &old_phase {
                    retired = std::mem::replace(&mut state.presented, frame.clone());
                }
            }
        }
        drop(state);
        drop(old_phase);
        drop(retired);
    }
}

type MapFn = unsafe extern "C" fn(
    *mut video::ffi::GstVideoMeta,
    u32,
    *mut gst::ffi::GstMapInfo,
    *mut glib::ffi::gpointer,
    *mut i32,
    gst::ffi::GstMapFlags,
) -> glib::ffi::gboolean;
struct MapObserver {
    original: MapFn,
    tracker: Weak<Mutex<Frames>>,
    info: video::VideoInfo,
    generation: u64,
}
#[derive(Clone, glib::Boxed)]
#[boxed_type(name = "ViewerScreenshotMapContext")]
struct MapContext(Arc<MapObserver>);
const MAP_META: &str = "ViewerScreenshotMapMeta";
const CONTEXT_FIELD: &str = "context";
fn register_map_meta() {
    static REGISTER: OnceLock<()> = OnceLock::new();
    REGISTER.get_or_init(|| gst::meta::CustomMeta::register(MAP_META, &[]));
}
unsafe fn install_map_observer(
    buffer: &mut gst::BufferRef,
    tracker: &Presentation,
    info: video::VideoInfo,
    generation: u64,
) {
    register_map_meta();
    unsafe {
        let meta = video::ffi::gst_buffer_get_video_meta(buffer.as_mut_ptr());
        if meta.is_null() {
            return;
        }
        let Some(mut original) = (*meta).map else {
            return;
        };
        if std::ptr::fn_addr_eq(original, observe_map as MapFn) {
            let Ok(existing) = gst::meta::CustomMeta::from_buffer(buffer, MAP_META) else {
                return;
            };
            let Ok(context) = existing.structure().get::<MapContext>(CONTEXT_FIELD) else {
                return;
            };
            original = context.0.original;
        }
        let context = MapContext(Arc::new(MapObserver {
            original,
            tracker: Arc::downgrade(&tracker.0),
            info,
            generation,
        }));
        // Unlike miniobject qdata, custom meta and its boxed Arc are copied
        // together with GstVideoMeta's map callback when buffers are copied.
        let mut context_meta = if gst::meta::CustomMeta::from_buffer(buffer, MAP_META).is_ok() {
            gst::meta::CustomMeta::from_mut_buffer(buffer, MAP_META)
        } else {
            gst::meta::CustomMeta::add(buffer, MAP_META)
        }
        .expect("registered map metadata");
        context_meta.mut_structure().set(CONTEXT_FIELD, context);
        (*meta).map = Some(observe_map);
    }
}
unsafe extern "C" fn observe_map(
    meta: *mut video::ffi::GstVideoMeta,
    plane: u32,
    map: *mut gst::ffi::GstMapInfo,
    data: *mut glib::ffi::gpointer,
    stride: *mut i32,
    flags: gst::ffi::GstMapFlags,
) -> glib::ffi::gboolean {
    // GstVideoMeta invokes map with a live meta/buffer and valid output slots.
    // No Rust panic may unwind into GStreamer. Poisoned locks are recovered.
    unsafe {
        let buffer = (*meta).buffer;
        let buffer_ref = gst::BufferRef::from_ptr(buffer);
        let Ok(context_meta) = gst::meta::CustomMeta::from_buffer(buffer_ref, MAP_META) else {
            return glib::ffi::GFALSE;
        };
        let Ok(context) = context_meta.structure().get::<MapContext>(CONTEXT_FIELD) else {
            return glib::ffi::GFALSE;
        };
        let observer = &context.0;
        let result = (observer.original)(meta, plane, map, data, stride, flags);
        if result == glib::ffi::GFALSE || plane != 0 {
            return result;
        }
        if let Some(tracker) = observer.tracker.upgrade() {
            let mut state = tracker.lock().unwrap_or_else(|e| e.into_inner());
            if state.generation == observer.generation
                && matches!(&state.phase, RenderPhase::Syncing { thread, .. } if *thread == std::thread::current().id())
            {
                let retired = state.mapped.replace(Frame {
                    buffer: from_glib_none(buffer),
                    info: observer.info.clone(),
                });
                drop(state);
                drop(retired);
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn buffer(tracker: &Presentation, pts: u64) -> (gst::Buffer, video::VideoInfo) {
        gst::init().unwrap();
        let info = video::VideoInfo::builder(video::VideoFormat::Rgba, 12, 8)
            .par((4, 3))
            .build()
            .unwrap();
        let mut buffer = gst::Buffer::with_size(info.size()).unwrap();
        let generation = tracker.0.lock().unwrap().generation;
        {
            let buffer = buffer.get_mut().unwrap();
            buffer.set_pts(gst::ClockTime::from_mseconds(pts));
            video::VideoMeta::add_full(
                buffer,
                video::VideoFrameFlags::empty(),
                info.format(),
                info.width(),
                info.height(),
                info.offset(),
                info.stride(),
            )
            .unwrap();
            // SAFETY: this test owns the writable CPU buffer and its video meta.
            unsafe {
                install_map_observer(buffer, tracker, info.clone(), generation);
            }
        }
        (buffer, info)
    }
    fn sync(observer: &Observer, buffer: &gst::Buffer, info: &video::VideoInfo) {
        observer.before_sync();
        let frame = video::VideoFrameRef::from_buffer_ref_readable(buffer, info).unwrap();
        drop(frame);
        observer.after_sync();
    }
    #[test]
    fn copied_or_reused_video_metadata_keeps_one_mapping_callback() {
        let tracker = Presentation::default();
        let observer = tracker.observer();
        tracker.stage(Overlay::empty(160.0, 90.0));
        let (mut first, info) = buffer(&tracker, 100);
        // A pool may reuse a buffer, and downstream may copy its metadata.
        // Neither case may recursively wrap our callback or lose its context.
        unsafe {
            install_map_observer(first.make_mut(), &tracker, info.clone(), 0);
        }
        let copy = first.copy();
        drop(first);
        sync(&observer, &copy, &info);
        observer.presented();
        assert_eq!(
            tracker.capture().unwrap().pts(),
            Some(gst::ClockTime::from_mseconds(100))
        );
    }
    #[test]
    fn nv12_capture_converts_limited_range_and_preserves_presented_frame() {
        gst::init().unwrap();
        // Width 6 deliberately exercises padded Y/UV strides.
        let info = video::VideoInfo::from_caps(&"video/x-raw,format=NV12,width=6,height=4,framerate=25/1,colorimetry=bt709,interlace-mode=progressive".parse::<gst::Caps>().unwrap()).unwrap();
        let tracker = Presentation::default();
        let observer = tracker.observer();
        tracker.stage(Overlay::empty(6.0, 4.0));
        let mut buffer = gst::Buffer::with_size(info.size()).unwrap();
        {
            let buffer = buffer.make_mut();
            let mut frame = video::VideoFrameRef::from_buffer_ref_writable(buffer, &info).unwrap();
            let stride = frame.plane_stride()[0] as usize;
            let y = frame.plane_data_mut(0).unwrap();
            for row in 0..4 {
                y[row * stride..row * stride + 6].fill(if row < 2 { 16 } else { 235 });
            }
            frame.plane_data_mut(1).unwrap().fill(128);
            drop(frame);
            video::VideoMeta::add_full(
                buffer,
                video::VideoFrameFlags::empty(),
                info.format(),
                info.width(),
                info.height(),
                info.offset(),
                info.stride(),
            )
            .unwrap();
            // SAFETY: exclusively owned CPU test buffer; no GPU access.
            unsafe {
                install_map_observer(buffer, &tracker, info.clone(), 0);
            }
        }
        sync(&observer, &buffer, &info);
        assert!(tracker.capture().is_err());
        observer.presented();
        let captured = tracker.capture().unwrap();
        tracker.clear();
        let image = captured.image().unwrap();
        assert_eq!((image.width(), image.height()), (6, 4));
        for (row, expected) in [(0, 0), (3, 255)] {
            let pixel = image.pixel_color(2, row);
            for value in [pixel.red(), pixel.green(), pixel.blue()] {
                assert!((value - expected).abs() <= 2, "{row}: {value}");
            }
        }
    }
    #[test]
    fn nv12_capture_uses_negotiated_color_matrix() {
        gst::init().unwrap();
        // Encodings of red differ between the SD and HD YUV matrices.
        for (matrix, y, u, v) in [("bt601", 81, 90, 240), ("bt709", 63, 102, 240)] {
            let caps: gst::Caps = format!("video/x-raw,format=NV12,width=6,height=4,framerate=25/1,colorimetry={matrix},interlace-mode=progressive").parse().unwrap();
            let info = video::VideoInfo::from_caps(&caps).unwrap();
            let mut buffer = gst::Buffer::with_size(info.size()).unwrap();
            {
                let mut frame =
                    video::VideoFrameRef::from_buffer_ref_writable(buffer.make_mut(), &info)
                        .unwrap();
                frame.plane_data_mut(0).unwrap().fill(y);
                frame
                    .plane_data_mut(1)
                    .unwrap()
                    .as_chunks_mut::<2>()
                    .0
                    .fill([u, v]);
            }
            let source = video::VideoFrameRef::from_buffer_ref_readable(&buffer, &info).unwrap();
            let (rgba, rgba_info) = convert_to_rgba(&source).unwrap();
            let pixels = video::VideoFrameRef::from_buffer_ref_readable(&rgba, &rgba_info).unwrap();
            let pixel = &pixels.plane_data(0).unwrap()[..4];
            assert!(
                pixel[0] >= 250 && pixel[1] <= 5 && pixel[2] <= 5 && pixel[3] == 255,
                "{matrix}: {pixel:?}"
            );
        }
    }
    #[test]
    fn only_presented_frames_are_captured_and_old_generations_cannot_return() {
        let tracker = Presentation::default();
        let observer = tracker.observer();
        tracker.stage(Overlay::empty(160.0, 90.0));
        let (first, info) = buffer(&tracker, 100);
        let (ahead, _) = buffer(&tracker, 200);
        sync(&observer, &first, &info);
        assert!(
            tracker.capture().is_err(),
            "synchronized is not yet presented"
        );
        observer.presented();
        let captured = tracker.capture().unwrap();
        assert_eq!(captured.pts(), Some(gst::ClockTime::from_mseconds(100)));
        assert_eq!(
            captured.output_size().unwrap(),
            (16, 8),
            "square-pixel aspect correction"
        );
        sync(&observer, &ahead, &info);
        assert_eq!(tracker.capture().unwrap().pts(), captured.pts());
        observer.presented();
        assert_eq!(
            tracker.capture().unwrap().pts(),
            Some(gst::ClockTime::from_mseconds(200))
        );
        assert_eq!(
            captured.pts(),
            Some(gst::ClockTime::from_mseconds(100)),
            "accepted capture stays fixed"
        );
        tracker.clear();
        tracker.stage(Overlay::empty(160.0, 90.0));
        sync(&observer, &first, &info);
        observer.presented();
        assert!(
            tracker.capture().is_err(),
            "stale mapped buffer must not cross a flush"
        );
    }
}
