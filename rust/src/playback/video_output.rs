//! Explicit memory contracts for CPU, NVDEC/OpenGL and Linux VA-API output.
use super::{Error, Result, deinterlace::Mode};
use gstreamer::{self as gst, prelude::*};
use gstreamer_base::prelude::BaseTransformExt;
use gstreamer_video::{VideoBufferFlags, prelude::VideoBufferExt};

// Leave bounded read-ahead room to absorb brief upstream processing stalls.
const QUEUE_BUFFERS: u32 = 8;
const GL_MEMORY: &str = "memory:GLMemory";
const VA_MEMORY: &str = "memory:VAMemory";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Format {
    Nv12,
    Rgba,
}
impl Format {
    fn name(self) -> &'static str {
        match self {
            Self::Nv12 => "NV12",
            Self::Rgba => "RGBA",
        }
    }
    fn caps(self) -> gst::Caps {
        raw_caps(GL_MEMORY, Some(self.name()))
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
        let queue = gst::ElementFactory::make("queue")
            .property("max-size-buffers", QUEUE_BUFFERS)
            .property("max-size-bytes", 0_u32)
            .property("max-size-time", 0_u64)
            .build()?;
        let upload = gst::ElementFactory::make("glupload").build()?;
        let convert = gst::ElementFactory::make("glcolorconvert").build()?;
        let mut elements = match mode {
            // videoconvert negotiates CPU I420->NV12 when needed; its ANY
            // feature template also passes GPU memory through unchanged.
            // Removing it breaks off-mode NV12 for software-decoded I420.
            Mode::Yadif | Mode::Linear | Mode::Off => vec![
                gst::ElementFactory::make("videoconvert").build()?,
                processor.clone(),
                queue.clone(),
                upload,
                convert,
            ],
            Mode::NvidiaGl => {
                bypass_progressive(&processor)?;
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
        })
    }
}

fn select_format(mode: Mode, requested: Option<Format>, caps: &gst::CapsRef) -> Result<Format> {
    let format = match (mode, requested) {
        (Mode::NvidiaGl, None) => Format::Rgba,
        (_, Some(format)) => format,
        (Mode::Yadif | Mode::Linear | Mode::Off | Mode::VaApi, None) => {
            if caps.can_intersect(&Format::Nv12.caps()) {
                Format::Nv12
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

fn bypass_progressive(element: &gst::Element) -> Result<()> {
    let transform = element
        .clone()
        .downcast::<gstreamer_base::BaseTransform>()
        .map_err(|_| Error::VideoOutput("gldeinterlace is not a video transform".into()))?;
    let weak = transform.downgrade();
    element
        .static_pad("sink")
        .ok_or(Error::MissingSinkPad)?
        .add_probe(
            gst::PadProbeType::BUFFER | gst::PadProbeType::EVENT_DOWNSTREAM,
            move |pad, probe| {
                // Do not let a previous progressive stream's passthrough state
                // affect the next stream's caps negotiation.
                if probe
                    .event()
                    .is_some_and(|event| matches!(event.view(), gst::EventView::Caps(_)))
                    && let Some(transform) = weak.upgrade()
                {
                    transform.set_passthrough(false);
                }
                if let (Some(transform), Some(buffer), Some(caps)) =
                    (weak.upgrade(), probe.buffer(), pad.current_caps())
                {
                    let interlaced = caps
                        .structure(0)
                        .and_then(|s| s.get::<&str>("interlace-mode").ok());
                    let process = needs_deinterlace(interlaced, buffer);
                    transform.set_passthrough(!process);
                }
                gst::PadProbeReturn::Ok
            },
        );
    Ok(())
}

fn needs_deinterlace(interlace: Option<&str>, buffer: &gst::BufferRef) -> bool {
    match interlace {
        Some("interleaved") => true,
        // Core BufferFlags discards the video-specific flag bits. Inspect the
        // video flags directly; this never maps GPU pixels to system memory.
        Some("mixed") => buffer.video_flags().contains(VideoBufferFlags::INTERLACED),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mixed_caps_use_video_flags_without_mapping_pixels() {
        gst::init().unwrap();
        let mut buffer = gst::Buffer::new();
        assert!(!needs_deinterlace(Some("mixed"), &buffer));
        assert!(needs_deinterlace(Some("interleaved"), &buffer));
        buffer
            .make_mut()
            .set_video_flags(VideoBufferFlags::INTERLACED | VideoBufferFlags::TFF);
        assert!(needs_deinterlace(Some("mixed"), &buffer));
        assert!(!needs_deinterlace(Some("progressive"), &buffer));
        buffer
            .make_mut()
            .unset_video_flags(VideoBufferFlags::INTERLACED);
        assert!(!needs_deinterlace(Some("mixed"), &buffer));
    }
    #[test]
    fn format_contract_handles_old_plugins_and_explicit_requests() {
        gst::init().unwrap();
        let old = Format::Rgba.caps();
        let mut current = old.clone();
        current.make_mut().append(Format::Nv12.caps());
        assert_eq!(
            select_format(Mode::Yadif, None, &current).unwrap(),
            Format::Nv12
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
    }
}
