//! Bound decoder failure tolerance and software parallelism per player.
//! Compressed TS retention is separately owned by input::Store.
use gstreamer::{self as gst, prelude::*};
use gstreamer_video::{self as gst_video, prelude::*};

const SOFTWARE_THREADS: usize = 6;
const HEVC_THREADS: usize = 10;
// Leave room for damaged pictures before the next keyframe. GstVideoDecoder
// resets its counter on successful output; these are consecutive errors, not
// a lifetime limit or a playback timeout.
const MAX_CONSECUTIVE_ERRORS: i32 = 64;

/// Captured once at player construction, applied before each decoder starts.
/// Thread limits apply only to libav video; error limits also cover GPU video.
pub(super) struct DecoderPolicy {
    parallelism: usize,
}

impl DecoderPolicy {
    pub fn new() -> Self {
        Self {
            parallelism: std::thread::available_parallelism().map_or(1, usize::from),
        }
    }

    pub fn configure(&self, element: &gst::Element) {
        if let Some(decoder) = element.downcast_ref::<gst_video::VideoDecoder>() {
            let limit = decoder.max_errors();
            if !(0..=MAX_CONSECUTIVE_ERRORS).contains(&limit) {
                // The default is unlimited. NVDEC repeatedly failing to create
                // a decoder (e.g. CUDA_ERROR_NO_DEVICE) otherwise never posts a
                // bus error and playbin can remain in preroll indefinitely.
                // Keep any stricter limit selected by the decoder itself.
                decoder.set_max_errors(MAX_CONSECUTIVE_ERRORS);
            }
        }
        let Some(factory) = element.factory() else {
            return;
        };
        if factory.plugin_name().as_deref() != Some("libav")
            || !factory
                .has_type(gst::ElementFactoryType::DECODER | gst::ElementFactoryType::MEDIA_VIDEO)
            || element.find_property("max-threads").is_none()
        {
            return;
        }
        // VLC also caps libav's automatic worker count (6, HEVC 10). Each
        // frame worker can retain reference pictures and its own decode state.
        // Keep codec-selected threading and full-quality decoding unchanged.
        let threads = self.threads(factory.name().as_str());
        element.set_property("max-threads", threads);
        tracing::info!(decoder = %factory.name(), threads, "Software decoder budget");
    }

    fn threads(&self, factory: &str) -> i32 {
        let limit = if factory == "avdec_h265" {
            HEVC_THREADS
        } else {
            SOFTWARE_THREADS
        };
        self.parallelism.clamp(1, limit) as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoder_budget_scales_without_unbounded_frame_workers() {
        for (cpus, video, hevc) in [(1, 1, 1), (4, 4, 4), (8, 6, 8), (64, 6, 10)] {
            let policy = DecoderPolicy { parallelism: cpus };
            assert_eq!(policy.threads("avdec_mpeg2video"), video);
            assert_eq!(policy.threads("avdec_h264"), video);
            assert_eq!(policy.threads("avdec_h265"), hevc);
        }
    }

    #[test]
    fn thread_budget_applies_to_libav_video_only() -> Result<(), Box<dyn std::error::Error>> {
        gst::init()?;
        let policy = DecoderPolicy { parallelism: 64 };
        for name in ["avdec_mpeg2video", "avdec_h264", "avdec_h265"] {
            let decoder = gst::ElementFactory::make(name).build()?;
            policy.configure(&decoder);
            assert_eq!(decoder.property::<i32>("max-threads"), policy.threads(name));
        }
        for name in ["avdec_aac", "identity"] {
            let element = gst::ElementFactory::make(name).build()?;
            let before = element
                .find_property("max-threads")
                .map(|_| element.property::<i32>("max-threads"));
            policy.configure(&element);
            assert_eq!(
                before,
                element
                    .find_property("max-threads")
                    .map(|_| element.property::<i32>("max-threads"))
            );
        }
        Ok(())
    }

    #[test]
    fn repeated_decode_failure_reaches_the_bus_without_gpu_or_display()
    -> Result<(), Box<dyn std::error::Error>> {
        gst::init()?;
        let policy = DecoderPolicy { parallelism: 1 };
        // Exercise the same GstVideoDecoder error path used by NVDEC without
        // opening hardware or substituting a renderer in a playback test.
        for stricter_limit in [None, Some(0), Some(2)] {
            let pipeline = gst::Pipeline::new();
            let element = gst::ElementFactory::make("avdec_h264").build()?;
            let decoder = element.downcast_ref::<gst_video::VideoDecoder>().unwrap();
            if let Some(limit) = stricter_limit {
                decoder.set_max_errors(limit);
            }
            policy.configure(&element);
            pipeline.add(&element)?;
            let bus = pipeline.bus().unwrap();
            let report_failure = || {
                gst_video::video_decoder_error!(
                    decoder,
                    1,
                    gst::StreamError::Decode,
                    ("Failed to decode data"),
                    ["Injected repeated decoder initialization failure"]
                )
            };
            let tolerated = stricter_limit.unwrap_or(MAX_CONSECUTIVE_ERRORS);
            for _ in 0..tolerated {
                assert_eq!(report_failure(), Ok(gst::FlowSuccess::Ok));
                assert!(bus.pop_filtered(&[gst::MessageType::Error]).is_none());
            }
            assert_eq!(report_failure(), Err(gst::FlowError::Error));
            let message = bus
                .pop_filtered(&[gst::MessageType::Error])
                .ok_or("decoder failed without notifying the playback bus")?;
            let gst::MessageView::Error(error) = message.view() else {
                unreachable!();
            };
            assert!(error.error().matches(gst::StreamError::Decode));
            assert_eq!(error.src(), Some(element.upcast_ref()));
            assert!(error.debug().unwrap().contains("initialization failure"));
        }
        Ok(())
    }
}
