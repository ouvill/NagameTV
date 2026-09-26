//! Explicit memory contracts for CPU, NVDEC/OpenGL and Linux VA-API output.
use super::{
    Error, Result,
    deinterlace::Mode,
    video_info::{Monitor, Streams},
};
use gstreamer::{self as gst, prelude::*};

// Leave bounded read-ahead room to absorb brief upstream processing stalls.
const QUEUE_BUFFERS: u32 = 8;
const GL_MEMORY: &str = "memory:GLMemory";
const VA_MEMORY: &str = "memory:VAMemory";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Format {
    Nv12,
    Rgba,
    Nv12OrRgba,
}
impl Format {
    fn name(self) -> &'static str {
        match self {
            Self::Nv12 => "NV12",
            Self::Rgba => "RGBA",
            Self::Nv12OrRgba => "NV12 or RGBA (negotiated)",
        }
    }
    fn caps(self) -> gst::Caps {
        match self {
            Self::Nv12 | Self::Rgba => raw_caps(GL_MEMORY, Some(self.name())),
            Self::Nv12OrRgba => gst::Caps::builder("video/x-raw")
                .features([GL_MEMORY])
                .field("format", gst::List::new(["NV12", "RGBA"]))
                .field("texture-target", "2D")
                .build(),
        }
    }
}

fn raw_caps(memory: &str, format: Option<&str>) -> gst::Caps {
    let mut builder = gst::Caps::builder("video/x-raw").features([memory]);
    if let Some(format) = format {
        builder = builder.field("format", format);
    }
    if memory == GL_MEMORY {
        // Two-plane NV12 textures keep color conversion in Qt's material.
        // external-oes may incorporate driver YUV conversion, so use 2D here.
        builder = builder.field("texture-target", "2D");
    }
    builder.build()
}

fn va_import_caps() -> gst::Caps {
    let mut caps = Format::Nv12.caps();
    caps.make_mut().append(Format::Rgba.caps());
    caps
}

fn upload_caps() -> gst::Caps {
    // Keep CPU conversion before upload/read-ahead, so 8-bit broadcast video
    // still uses NV12. Native GL/DMABuf inputs retain their decoded format;
    // glcolorconvert chooses RGBA when direct NV12 conversion is unsupported.
    let mut caps = raw_caps("memory:SystemMemory", Some("NV12"));
    caps.make_mut().append(raw_caps(GL_MEMORY, None));
    caps.make_mut().append(raw_caps("memory:DMABuf", None));
    caps
}

fn filter(caps: gst::Caps) -> Result<gst::Element> {
    Ok(gst::ElementFactory::make("capsfilter")
        .property("caps", caps)
        .build()?)
}

/// Capability validation owns the exact sink it checked. Assembly consumes it;
/// callers cannot accidentally attach these caps to a different sink/plugin.
pub(super) struct Validated {
    sink: gst::Element,
    mode: Mode,
    format: Format,
}

pub(super) struct Output {
    pub bin: gst::Bin,
    pub processor: gst::Element,
    pub queue: gst::Element,
    pub streams: Streams,
}

impl Validated {
    pub fn new(sink: gst::Element, mode: Mode) -> Result<Self> {
        if mode == Mode::VaApi
            && (!cfg!(target_os = "linux")
                || !std::env::var("QT_QPA_PLATFORM")
                    .is_ok_and(|platform| platform.starts_with("wayland")))
        {
            return Err(Error::VideoOutput("VA-API DMA_DRM output requires Linux Wayland with this qml6glsink; start with QT_QPA_PLATFORM=wayland and a working Wayland display".into()));
        }
        let requested = match std::env::var("NAGAMETV_VIDEO_FORMAT") {
            Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
                "auto" => None,
                "nv12" => Some(Format::Nv12),
                "rgba" => Some(Format::Rgba),
                _ => {
                    return Err(Error::VideoOutput(
                        "NAGAMETV_VIDEO_FORMAT must be auto, nv12 or rgba".into(),
                    ));
                }
            },
            Err(std::env::VarError::NotPresent) => None,
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err(Error::VideoOutput(
                    "NAGAMETV_VIDEO_FORMAT is not valid Unicode".into(),
                ));
            }
        };
        let caps = sink
            .static_pad("sink")
            .ok_or(Error::MissingSinkPad)?
            .pad_template_caps();
        let format = select_format(mode, requested, &caps)?;
        Ok(Self { sink, mode, format })
    }

    pub fn build(self) -> Result<Output> {
        let Self { sink, mode, format } = self;
        let processor = mode.build()?;
        let pad = processor.static_pad("sink").ok_or(Error::MissingSinkPad)?;
        let input = match mode {
            Mode::NvidiaGl => {
                let transform = processor
                    .downcast_ref::<gstreamer_base::BaseTransform>()
                    .ok_or_else(|| {
                        Error::VideoOutput("gldeinterlace is not a video transform".into())
                    })?;
                Monitor::bypass_progressive(&pad, transform)
            }
            Mode::Yadif | Mode::Linear | Mode::Off | Mode::VaApi => Monitor::observe(&pad),
        };
        let streams = Streams {
            input,
            output: Monitor::observe(&sink.static_pad("sink").ok_or(Error::MissingSinkPad)?),
        };
        let queue = gst::ElementFactory::make("queue")
            .property("max-size-buffers", QUEUE_BUFFERS)
            .property("max-size-bytes", 0_u32)
            .property("max-size-time", 0_u64)
            .property("silent", true)
            .build()?;
        let upload = gst::ElementFactory::make("glupload").build()?;
        let convert = gst::ElementFactory::make("glcolorconvert").build()?;
        let mut elements = match mode {
            // Deinterlace I420 before converting it into glupload's NV12 pool.
            // This keeps temporal filter history in ordinary decoded memory,
            // while retaining conversion/read-ahead upstream of GL upload.
            // Formats already matching the sink can pass through unchanged.
            // videoconvert's ANY feature template still passes GPU memory
            // through in off mode; CPU I420 needs conversion for NV12 output.
            Mode::Yadif | Mode::Linear | Mode::Off => {
                let mut cpu = vec![
                    // Negotiate unsupported input formats without converting I420
                    // prematurely: deinterlace itself cannot process every format.
                    gst::ElementFactory::make("videoconvert").build()?,
                    processor.clone(),
                    gst::ElementFactory::make("videoconvert").build()?,
                ];
                if format == Format::Nv12OrRgba {
                    cpu.push(filter(upload_caps())?);
                }
                cpu.extend([queue.clone(), upload, convert]);
                cpu
            }
            Mode::NvidiaGl => {
                vec![
                    filter(raw_caps(GL_MEMORY, Some("NV12")))?,
                    queue.clone(),
                    convert,
                    processor.clone(),
                ]
            }
            Mode::VaApi => {
                // VA-API is a Linux backend. vadeinterlace preserves VAMemory;
                // vapostproc exports it for EGL import without a CPU download.
                if !cfg!(target_os = "linux") {
                    return Err(Error::VideoOutput("VA-API output requires Linux".into()));
                }
                vec![
                    filter(raw_caps(VA_MEMORY, Some("NV12")))?,
                    processor.clone(),
                    gst::ElementFactory::make("vapostproc").build()?,
                    // DMA_DRM requires EGL import; unlike ordinary NV12
                    // DMABufs it cannot fall through glupload's CPU map path.
                    filter(raw_caps("memory:DMABuf", Some("DMA_DRM")))?,
                    queue.clone(),
                    upload,
                    // Keep EGL import in NV12/RGBA 2D textures. An unconstrained
                    // glcolorconvert also admits packed YUV (YUYV on Mesa),
                    // whose VA conversion can fail even if caps advertise it.
                    filter(va_import_caps())?,
                    convert,
                ]
            }
        };
        if mode == Mode::NvidiaGl && format == Format::Nv12 {
            // Explicit NV12 output is possible, but vfir still needs an RGBA
            // intermediate. Auto avoids this extra GPU conversion pass.
            elements.push(gst::ElementFactory::make("glcolorconvert").build()?);
        }
        elements.push(filter(format.caps())?);
        elements.push(sink);
        let bin = gst::Bin::new();
        bin.add_many(&elements)?;
        gst::Element::link_many(&elements)?;
        let pad = gst::GhostPad::with_target(
            &elements[0]
                .static_pad("sink")
                .ok_or(Error::MissingSinkPad)?,
        )?;
        pad.set_active(true)?;
        bin.add_pad(&pad)?;
        configure_decoders(mode)?;
        tracing::info!(
            "Video processing: {}; sink format: {}",
            mode.label(),
            format.name()
        );
        Ok(Output {
            bin,
            processor,
            queue,
            streams,
        })
    }
}

fn select_format(mode: Mode, requested: Option<Format>, caps: &gst::CapsRef) -> Result<Format> {
    let format = match (mode, requested) {
        (Mode::NvidiaGl, None) => Format::Rgba,
        (_, Some(format)) => format,
        (Mode::Yadif | Mode::Linear | Mode::Off | Mode::VaApi, None) => {
            if caps.can_intersect(&Format::Nv12.caps()) {
                if caps.can_intersect(&Format::Rgba.caps()) {
                    // NV12 can pass through, but P010 GL textures cannot be
                    // converted directly to NV12. Let glcolorconvert negotiate
                    // RGBA for those inputs without downloading GPU memory.
                    Format::Nv12OrRgba
                } else {
                    Format::Nv12
                }
            } else {
                Format::Rgba
            }
        }
    };
    if !caps.can_intersect(&format.caps()) {
        return Err(Error::VideoOutput(format!(
            "qml6glsink does not support {} GL textures; update its GStreamer plugin",
            format.name()
        )));
    }
    Ok(format)
}

/// playbin3 has no per-instance decoder factory selection API. At startup only,
/// restrict its process-local video decoder registry for explicit GPU modes.
/// Audio factories retain their ranks. Memory caps above also prohibit hidden
/// hardware downloads/software decoding. CPU modes preserve user GST ranks.
fn configure_decoders(mode: Mode) -> Result<()> {
    let plugin = match mode {
        Mode::Yadif | Mode::Linear | Mode::Off => return Ok(()),
        Mode::NvidiaGl => "nvcodec",
        Mode::VaApi => "va",
    };
    let factories = gst::ElementFactory::factories_with_type(
        gst::ElementFactoryType::DECODER | gst::ElementFactoryType::MEDIA_VIDEO,
        gst::Rank::NONE,
    );
    if !factories
        .iter()
        .any(|factory| factory.plugin_name().as_deref() == Some(plugin))
    {
        return Err(Error::VideoOutput(format!(
            "No {plugin} video decoder is available; check the GPU device, driver and GStreamer plugin. Select yadif explicitly to use CPU deinterlacing."
        )));
    }
    for factory in factories {
        factory.set_rank(if factory.plugin_name().as_deref() == Some(plugin) {
            gst::Rank::PRIMARY
        } else {
            gst::Rank::NONE
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn format_contract_handles_old_plugins_and_explicit_requests() {
        gst::init().unwrap();
        let old = Format::Rgba.caps();
        let mut current = old.clone();
        current.make_mut().append(Format::Nv12.caps());
        assert_eq!(
            select_format(Mode::Yadif, None, &current).unwrap(),
            Format::Nv12OrRgba
        );
        assert_eq!(
            select_format(Mode::Yadif, None, &old).unwrap(),
            Format::Rgba
        );
        assert!(select_format(Mode::Yadif, Some(Format::Nv12), &old).is_err());
        assert_eq!(
            select_format(Mode::NvidiaGl, None, &current).unwrap(),
            Format::Rgba
        );
        assert_eq!(
            select_format(Mode::NvidiaGl, Some(Format::Nv12), &current).unwrap(),
            Format::Nv12
        );
        for memory in ["memory:SystemMemory", "memory:CUDAMemory", VA_MEMORY] {
            assert!(
                !Format::Nv12
                    .caps()
                    .can_intersect(&raw_caps(memory, Some("NV12")))
            );
        }
        let va_import = va_import_caps();
        assert!(va_import.can_intersect(&Format::Nv12.caps()));
        assert!(va_import.can_intersect(&Format::Rgba.caps()));
        assert!(!va_import.can_intersect(&raw_caps(GL_MEMORY, Some("YUY2"))));
        assert!(!va_import.can_intersect(&raw_caps("memory:SystemMemory", Some("NV12"))));
        let upload = upload_caps();
        assert!(upload.can_intersect(&raw_caps("memory:SystemMemory", Some("NV12"))));
        assert!(!upload.can_intersect(&raw_caps("memory:SystemMemory", Some("P010_10LE"))));
        assert!(upload.can_intersect(&raw_caps(GL_MEMORY, Some("P010_10LE"))));
        assert!(upload.can_intersect(&raw_caps(GL_MEMORY, Some("NV12"))));
        assert!(upload.can_intersect(&raw_caps("memory:DMABuf", Some("DMA_DRM"))));
    }
}
