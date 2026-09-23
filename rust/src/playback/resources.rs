//! Bound software decoder parallelism per player.
//! Compressed TS retention is separately owned by input::Store.
use gstreamer::{self as gst, prelude::*};

const SOFTWARE_THREADS: usize = 6;
const HEVC_THREADS: usize = 10;

/// Captured once at player construction, applied before each decoder starts.
/// Factory and property checks leave GPU and audio decoders untouched.
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
    fn policy_applies_to_libav_video_only() -> Result<(), Box<dyn std::error::Error>> {
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
}
