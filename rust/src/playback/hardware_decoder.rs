//! Hardware video decoders that cannot produce any format the output accepts.
use gstreamer::{self as gst, prelude::*};

/// A hardware decoder present when negotiation failed before any video
/// reached the sink. Only failure classification constructs it; disabling
/// consumes it.
#[derive(Debug)]
pub struct Rejected {
    factory: gst::ElementFactory,
}

impl Rejected {
    /// Raspberry Pi 4's v4l2slh265dec either fixes Broadcom's column-tiled
    /// DMA_DRM, which no element downstream accepts, or reports the format as
    /// UNKNOWN, depending on the downstream query. The bus may then carry only
    /// the demuxer's not-negotiated flow error, so neither the posting element
    /// nor the decoder's own caps identify the cause.
    pub(super) fn find(
        player: &gst::Element,
        video_output_negotiated: bool,
        source: &gst::glib::Error,
        debug: Option<&str>,
    ) -> Option<Self> {
        if video_output_negotiated || !negotiation_failed(source, debug) {
            return None;
        }
        let bin = player.downcast_ref::<gst::Bin>()?;
        let mut elements = bin.iterate_recurse();
        while let Ok(Some(element)) = elements.next() {
            let Some(factory) = element.factory() else {
                continue;
            };
            // A factory already removed from selection is never retried, which
            // bounds automatic reopening by the number of hardware decoders.
            if factory.has_type(
                gst::ElementFactoryType::DECODER
                    | gst::ElementFactoryType::MEDIA_VIDEO
                    | gst::ElementFactoryType::HARDWARE,
            ) && factory.rank() > gst::Rank::NONE
            {
                return Some(Self { factory });
            }
        }
        None
    }

    pub fn name(&self) -> String {
        self.factory.name().to_string()
    }

    /// Exclude the factory from automatic selection for this process. The
    /// device cannot change its output formats until the next launch.
    pub fn disable(self) {
        self.factory.set_rank(gst::Rank::NONE);
    }
}

fn negotiation_failed(source: &gst::glib::Error, debug: Option<&str>) -> bool {
    // GST_ELEMENT_FLOW_ERROR's debug text is the only bus record of the flow
    // return; the GError itself is a generic stream failure.
    source.matches(gst::CoreError::Negotiation)
        || (source.matches(gst::StreamError::Failed)
            && debug.is_some_and(|debug| debug.contains("not-negotiated")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gst::glib;
    use gst::subclass::prelude::*;

    glib::wrapper! {
        pub struct FakeDecoder(ObjectSubclass<imp::FakeDecoder>) @extends gst::Element, gst::Object;
    }

    mod imp {
        use super::*;
        use std::sync::OnceLock;

        #[derive(Default)]
        pub struct FakeDecoder;
        #[glib::object_subclass]
        impl ObjectSubclass for FakeDecoder {
            const NAME: &'static str = "NagameTestFakeHardwareDecoder";
            type Type = super::FakeDecoder;
            type ParentType = gst::Element;
        }
        impl ObjectImpl for FakeDecoder {
            fn constructed(&self) {
                self.parent_constructed();
                let template = Self::pad_templates()[0].clone();
                let pad = gst::Pad::from_template(&template);
                // A pad from this class's own template is always accepted.
                self.obj().add_pad(&pad).expect("add template pad");
            }
        }
        impl GstObjectImpl for FakeDecoder {}
        impl ElementImpl for FakeDecoder {
            fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
                static META: OnceLock<gst::subclass::ElementMetadata> = OnceLock::new();
                Some(META.get_or_init(|| {
                    gst::subclass::ElementMetadata::new(
                        "Fake hardware decoder",
                        "Codec/Decoder/Video/Hardware",
                        "Never negotiates its output",
                        "NagameTV",
                    )
                }))
            }
            fn pad_templates() -> &'static [gst::PadTemplate] {
                static TEMPLATES: OnceLock<Vec<gst::PadTemplate>> = OnceLock::new();
                TEMPLATES.get_or_init(|| {
                    vec![
                        gst::PadTemplate::new(
                            "src",
                            gst::PadDirection::Src,
                            gst::PadPresence::Always,
                            &gst::Caps::new_any(),
                        )
                        .expect("valid template"),
                    ]
                })
            }
        }
    }

    fn not_negotiated() -> (glib::Error, &'static str) {
        (
            glib::Error::new(gst::StreamError::Failed, "Internal data stream error."),
            "streaming stopped, reason not-negotiated (-4)",
        )
    }

    #[test]
    fn rejects_an_unnegotiated_hardware_decoder_once() -> Result<(), Box<dyn std::error::Error>> {
        gst::init()?;
        gst::Element::register(
            None,
            "nagametestfakehwdec",
            gst::Rank::PRIMARY,
            FakeDecoder::static_type(),
        )?;
        let pipeline = gst::Pipeline::new();
        let decoder = gst::ElementFactory::make("nagametestfakehwdec").build()?;
        pipeline.add(&decoder)?;
        let player = pipeline.upcast_ref::<gst::Element>();

        let (source, debug) = not_negotiated();
        assert!(
            Rejected::find(player, true, &source, Some(debug)).is_none(),
            "video that reached the sink was decodable"
        );
        let other = glib::Error::new(gst::ResourceError::NotFound, "Not found");
        assert!(Rejected::find(player, false, &other, None).is_none());
        let unrelated_flow =
            glib::Error::new(gst::StreamError::Failed, "Internal data stream error.");
        assert!(
            Rejected::find(player, false, &unrelated_flow, Some("reason error (-5)")).is_none()
        );

        let negotiation = glib::Error::new(gst::CoreError::Negotiation, "Unsupported pixel format");
        assert!(Rejected::find(player, false, &negotiation, None).is_some());
        let rejected = Rejected::find(player, false, &source, Some(debug)).ok_or("not rejected")?;
        assert_eq!(rejected.name(), "nagametestfakehwdec");
        rejected.disable();
        assert!(
            Rejected::find(player, false, &source, Some(debug)).is_none(),
            "a disabled factory must not trigger another reopen"
        );
        Ok(())
    }
}
