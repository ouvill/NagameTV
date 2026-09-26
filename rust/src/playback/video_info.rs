//! Negotiated metadata and the latest frame's scan type, without retaining pixels.
use gstreamer::{self as gst, prelude::*};
use gstreamer_base::{BaseTransform, prelude::BaseTransformExt};
use gstreamer_video::{VideoBufferFlags, VideoInfo, VideoInterlaceMode, prelude::VideoBufferExt};
use serde::Serialize;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Scan {
    #[default]
    Unknown,
    Progressive,
    Interlaced,
}

impl Scan {
    fn from_buffer(info: &VideoInfo, buffer: &gst::BufferRef) -> Self {
        match info.interlace_mode() {
            VideoInterlaceMode::Progressive => Self::Progressive,
            VideoInterlaceMode::Interleaved
            | VideoInterlaceMode::Fields
            | VideoInterlaceMode::Alternate => Self::Interlaced,
            VideoInterlaceMode::Mixed => {
                // ONEFIELD cannot be treated as a complete progressive frame.
                // VideoBufferExt preserves flags absent from core BufferFlags.
                if buffer
                    .video_flags()
                    .intersects(VideoBufferFlags::INTERLACED | VideoBufferFlags::ONEFIELD)
                {
                    Self::Interlaced
                } else {
                    Self::Progressive
                }
            }
            // GStreamer can add new modes; do not declare them progressive.
            _ => Self::Unknown,
        }
    }
}

#[derive(Clone, Default)]
pub(super) enum Observation {
    #[default]
    Unavailable,
    Negotiated {
        info: VideoInfo,
        memory: String,
        scan: Scan,
    },
}

impl Observation {
    fn set_caps(&mut self, caps: &gst::CapsRef) {
        *self = match VideoInfo::from_caps(caps) {
            Ok(info) => Self::Negotiated {
                info,
                memory: caps
                    .features(0)
                    .expect("validated raw video caps")
                    .to_string(),
                scan: Scan::Unknown,
            },
            Err(error) => {
                tracing::warn!(
                    error = &error as &dyn std::error::Error,
                    "Failed to read negotiated video information"
                );
                Self::Unavailable
            }
        };
    }

    fn clear_frame(&mut self) {
        if let Self::Negotiated { scan, .. } = self {
            *scan = Scan::Unknown;
        }
    }

    fn observe(&mut self, buffer: &gst::BufferRef) -> Scan {
        match self {
            Self::Unavailable => Scan::Unknown,
            Self::Negotiated { info, scan, .. } => {
                *scan = Scan::from_buffer(info, buffer);
                *scan
            }
        }
    }
}

pub(super) struct Monitor(Arc<Mutex<Observation>>);

impl Monitor {
    pub fn observe(pad: &gst::Pad) -> Self {
        Self::install(pad, None)
    }

    pub fn bypass_progressive(pad: &gst::Pad, transform: &BaseTransform) -> Self {
        Self::install(pad, Some(transform.downgrade()))
    }

    fn install(pad: &gst::Pad, bypass: Option<gst::glib::WeakRef<BaseTransform>>) -> Self {
        let monitor = Self(Arc::default());
        let observed = monitor.0.clone();
        pad.add_probe(
            gst::PadProbeType::BUFFER
                | gst::PadProbeType::EVENT_DOWNSTREAM
                | gst::PadProbeType::EVENT_FLUSH,
            move |_, probe| {
                let mut observed = observed.lock().expect("video information lock poisoned");
                if let Some(event) = probe.event() {
                    match event.view() {
                        gst::EventView::Caps(event) => observed.set_caps(event.caps()),
                        gst::EventView::StreamStart(_) => *observed = Observation::Unavailable,
                        // Flushing seeks need not resend caps. Keep the format,
                        // but never report a frame from before the seek.
                        gst::EventView::FlushStart(_)
                        | gst::EventView::FlushStop(_)
                        | gst::EventView::Segment(_) => observed.clear_frame(),
                        _ => return gst::PadProbeReturn::Ok,
                    }
                    if let Some(transform) = bypass.as_ref().and_then(|weak| weak.upgrade()) {
                        // The previous stream must not affect caps negotiation.
                        transform.set_passthrough(false);
                    }
                }
                if let Some(buffer) = probe.buffer() {
                    let scan = observed.observe(buffer);
                    if let Some(transform) = bypass.as_ref().and_then(|weak| weak.upgrade()) {
                        transform.set_passthrough(scan == Scan::Progressive);
                    }
                }
                gst::PadProbeReturn::Ok
            },
        );
        monitor
    }

    pub fn snapshot(&self) -> Observation {
        self.0
            .lock()
            .expect("video information lock poisoned")
            .clone()
    }
}

pub(super) struct Streams {
    pub input: Monitor,
    pub output: Monitor,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_modes_and_mixed_frame_flags_do_not_map_pixels() {
        gst::init().unwrap();
        for (mode, flags, expected) in [
            (
                VideoInterlaceMode::Progressive,
                VideoBufferFlags::INTERLACED,
                Scan::Progressive,
            ),
            (
                VideoInterlaceMode::Interleaved,
                VideoBufferFlags::empty(),
                Scan::Interlaced,
            ),
            (
                VideoInterlaceMode::Fields,
                VideoBufferFlags::empty(),
                Scan::Interlaced,
            ),
            (
                VideoInterlaceMode::Alternate,
                VideoBufferFlags::BOTTOM_FIELD,
                Scan::Interlaced,
            ),
            (
                VideoInterlaceMode::Mixed,
                VideoBufferFlags::empty(),
                Scan::Progressive,
            ),
            (
                VideoInterlaceMode::Mixed,
                VideoBufferFlags::TFF,
                Scan::Progressive,
            ),
            (
                VideoInterlaceMode::Mixed,
                VideoBufferFlags::RFF,
                Scan::Progressive,
            ),
            (
                VideoInterlaceMode::Mixed,
                VideoBufferFlags::ONEFIELD,
                Scan::Interlaced,
            ),
            (
                VideoInterlaceMode::Mixed,
                VideoBufferFlags::INTERLACED,
                Scan::Interlaced,
            ),
        ] {
            let info = VideoInfo::builder(gstreamer_video::VideoFormat::I420, 320, 240)
                .interlace_mode(mode)
                .build()
                .unwrap();
            let mut buffer = gst::Buffer::new();
            buffer.make_mut().set_video_flags(flags);
            assert_eq!(
                Scan::from_buffer(&info, &buffer),
                expected,
                "{mode:?}, {flags:?}"
            );
        }
    }

    #[test]
    fn probe_resets_scan_and_bypass_on_caps_stream_changes_and_seeks()
    -> Result<(), Box<dyn std::error::Error>> {
        gst::init()?;
        // Exercise the GL bypass policy with an ordinary BaseTransform. No GL
        // element, display or GPU is created by this hardware-free probe test.
        let transform = gst::ElementFactory::make("identity")
            .build()?
            .downcast::<BaseTransform>()
            .expect("identity is a BaseTransform");
        let pad = gst::Pad::builder(gst::PadDirection::Sink)
            .chain_function(|_, _, _| Ok(gst::FlowSuccess::Ok))
            .event_function(|_, _, _| true)
            // Admit malformed caps for the fault injection below; a real
            // element normally rejects them before the event reaches a probe.
            .query_function(|_, _, query| match query.view_mut() {
                gst::QueryViewMut::AcceptCaps(query) => {
                    query.set_result(true);
                    true
                }
                _ => false,
            })
            .build();
        let monitor = Monitor::bypass_progressive(&pad, &transform);
        pad.set_active(true)?;
        assert!(pad.send_event(gst::event::StreamStart::new("first")));
        let caps: gst::Caps =
            "video/x-raw,format=I420,width=320,height=240,interlace-mode=mixed".parse()?;
        assert!(pad.send_event(gst::event::Caps::new(&caps)));
        let segment = gst::FormattedSegment::<gst::ClockTime>::new();
        assert!(pad.send_event(gst::event::Segment::new(&segment)));
        for flags in [
            VideoBufferFlags::empty(),
            VideoBufferFlags::INTERLACED,
            VideoBufferFlags::empty(),
            VideoBufferFlags::ONEFIELD,
        ] {
            let mut buffer = gst::Buffer::new();
            buffer.make_mut().set_video_flags(flags);
            pad.chain(buffer)?;
            let expected = if flags.is_empty() {
                Scan::Progressive
            } else {
                Scan::Interlaced
            };
            assert!(
                matches!(monitor.snapshot(), Observation::Negotiated { scan, .. } if scan == expected)
            );
            assert_eq!(transform.is_passthrough(), expected == Scan::Progressive);
        }
        assert!(pad.send_event(gst::event::FlushStart::new()));
        assert!(pad.send_event(gst::event::FlushStop::new(true)));
        assert!(matches!(
            monitor.snapshot(),
            Observation::Negotiated {
                scan: Scan::Unknown,
                ..
            }
        ));
        assert!(!transform.is_passthrough());
        // A flushing seek can resume without another caps event.
        assert!(pad.send_event(gst::event::Segment::new(&segment)));
        pad.chain(gst::Buffer::new())?;
        assert!(transform.is_passthrough());
        assert!(pad.send_event(gst::event::Caps::new(&caps)));
        assert!(matches!(
            monitor.snapshot(),
            Observation::Negotiated {
                scan: Scan::Unknown,
                ..
            }
        ));
        assert!(!transform.is_passthrough());
        pad.chain(gst::Buffer::new())?;
        // A malformed replacement must not preserve the old format or bypass.
        let invalid: gst::Caps = "video/x-raw,format=I420".parse()?;
        assert!(pad.send_event(gst::event::Caps::new(&invalid)));
        assert!(matches!(monitor.snapshot(), Observation::Unavailable));
        assert!(!transform.is_passthrough());
        assert!(pad.send_event(gst::event::Caps::new(&caps)));
        pad.chain(gst::Buffer::new())?;
        assert!(transform.is_passthrough());
        assert!(pad.send_event(gst::event::StreamStart::new("second")));
        assert!(matches!(monitor.snapshot(), Observation::Unavailable));
        assert!(!transform.is_passthrough());
        pad.set_active(false)?;
        Ok(())
    }
}
