use super::{Mode, Routing, route};
use gstreamer::{self as gst, glib};
use gstreamer_base::{self as gst_base, subclass::prelude::*};
use std::sync::OnceLock;

glib::wrapper! {
    pub struct DualMono(ObjectSubclass<imp::DualMono>) @extends gst_base::BaseTransform, gst::Element, gst::Object;
}
impl DualMono {
    pub fn new(routing: Routing) -> Self {
        let element: Self = glib::Object::new();
        // Constructed on this thread before publication to any pipeline.
        let _ = element.imp().routing.set(routing);
        element
    }
}

mod imp {
    use super::*;
    #[derive(Default)]
    pub struct DualMono {
        pub routing: OnceLock<Routing>,
    }
    #[glib::object_subclass]
    impl ObjectSubclass for DualMono {
        const NAME: &'static str = "FeatureLabDualMono";
        type Type = super::DualMono;
        type ParentType = gst_base::BaseTransform;
    }
    impl ObjectImpl for DualMono {}
    impl GstObjectImpl for DualMono {}
    impl ElementImpl for DualMono {
        fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
            static META: OnceLock<gst::subclass::ElementMetadata> = OnceLock::new();
            Some(META.get_or_init(|| {
                gst::subclass::ElementMetadata::new(
                    "Dual mono routing",
                    "Filter/Audio",
                    "Route confirmed broadcast dual mono",
                    "Mirakurun Viewer",
                )
            }))
        }
        fn pad_templates() -> &'static [gst::PadTemplate] {
            static TEMPLATES: OnceLock<Vec<gst::PadTemplate>> = OnceLock::new();
            TEMPLATES.get_or_init(|| {
                let caps = gst::Caps::builder("audio/x-raw")
                    .field("format", "F32LE")
                    .field("layout", "interleaved")
                    .field("rate", gst::IntRange::<i32>::new(1, i32::MAX))
                    .field("channels", gst::IntRange::<i32>::new(1, i32::MAX))
                    .build();
                [
                    ("sink", gst::PadDirection::Sink),
                    ("src", gst::PadDirection::Src),
                ]
                .into_iter()
                .map(|(name, direction)| {
                    // Static nonempty names, concrete directions and valid caps satisfy
                    // PadTemplate's contract; GObject class setup cannot return a Result.
                    gst::PadTemplate::new(name, direction, gst::PadPresence::Always, &caps)
                        .expect("valid static audio pad template")
                })
                .collect()
            })
        }
    }
    impl BaseTransformImpl for DualMono {
        const MODE: gst_base::subclass::base_transform::BaseTransformMode =
            gst_base::subclass::base_transform::BaseTransformMode::AlwaysInPlace;
        const PASSTHROUGH_ON_SAME_CAPS: bool = false;
        const TRANSFORM_IP_ON_PASSTHROUGH: bool = false;
        fn set_caps(
            &self,
            input: &gst::Caps,
            _output: &gst::Caps,
        ) -> Result<(), gst::LoggableError> {
            let stereo = input
                .structure(0)
                .and_then(|s| s.get::<i32>("channels").ok())
                == Some(2);
            if let Some(routing) = self.routing.get() {
                routing.renegotiate(stereo);
            }
            Ok(())
        }
        fn sink_event(&self, event: gst::Event) -> bool {
            if let Some(routing) = self.routing.get() {
                match event.view() {
                    gst::EventView::StreamStart(start)
                        if !routing.stream_start(start.stream_id()) =>
                    {
                        return false;
                    }
                    gst::EventView::FlushStart(_) => routing.flush(),
                    _ => {}
                }
            }
            self.parent_sink_event(event)
        }
        fn stop(&self) -> Result<(), gst::ErrorMessage> {
            if let Some(routing) = self.routing.get() {
                routing.reset();
            }
            self.parent_stop()
        }
        fn transform_ip(
            &self,
            buffer: &mut gst::BufferRef,
        ) -> Result<gst::FlowSuccess, gst::FlowError> {
            let mode = self
                .routing
                .get()
                .and_then(|routing| routing.format().mode)
                .unwrap_or(Mode::Both);
            if mode != Mode::Both {
                // BaseTransform supplies a writable buffer; mapping performs copy-on-write
                // for any still-shared memory. Metadata (PTS/duration) is untouched.
                let mut data = buffer.map_writable().map_err(|_| gst::FlowError::Error)?;
                route(data.as_mut_slice(), mode)?;
            }
            Ok(gst::FlowSuccess::Ok)
        }
    }
}
