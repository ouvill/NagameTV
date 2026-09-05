use gst::prelude::*;
use gstreamer as gst;
use serde::Serialize;

#[derive(Default, Serialize)]
pub struct VideoFormat {
    width: Option<i32>,
    height: Option<i32>,
    fps: Option<f64>,
    interlace: Option<String>,
    pixel_format: Option<String>,
    pixel_aspect_ratio: Option<String>,
}

impl VideoFormat {
    fn from_caps(caps: Option<&gst::CapsRef>) -> Self {
        let Some(s) = caps.and_then(|caps| caps.structure(0)) else {
            return Self::default();
        };
        Self {
            width: s.get("width").ok(),
            height: s.get("height").ok(),
            fps: s
                .get::<gst::Fraction>("framerate")
                .ok()
                .filter(|rate| rate.numer() > 0 && rate.denom() > 0)
                .map(|rate| f64::from(rate.numer()) / f64::from(rate.denom())),
            interlace: s.get::<&str>("interlace-mode").ok().map(str::to_owned),
            pixel_format: s.get::<&str>("format").ok().map(str::to_owned),
            pixel_aspect_ratio: s
                .get::<gst::Fraction>("pixel-aspect-ratio")
                .ok()
                .map(|ratio| format!("{}:{}", ratio.numer(), ratio.denom())),
        }
    }
}

#[derive(Serialize)]
pub struct VideoStats {
    state: String,
    input: VideoFormat,
    output: VideoFormat,
    deinterlacer: String,
    rendered: Option<u64>,
    dropped: Option<u64>,
    average_fps: Option<f64>,
    queue_buffers: u32,
    queue_bytes: u32,
    queue_ms: f64,
    gstreamer: String,
}

pub fn snapshot(
    player: &gst::Element,
    processor: &gst::Element,
    queue: &gst::Element,
    sink: &gst::Element,
    deinterlacer: &str,
) -> VideoStats {
    let state = player.current_state();
    let active = matches!(state, gst::State::Paused | gst::State::Playing);
    let caps = |element: &gst::Element| {
        if active {
            element
                .static_pad("sink")
                .and_then(|pad| pad.current_caps())
        } else {
            None
        }
    };
    let stats = active.then(|| sink.property::<gst::Structure>("stats"));
    VideoStats {
        state: format!("{state:?}"),
        input: VideoFormat::from_caps(caps(processor).as_deref()),
        output: VideoFormat::from_caps(caps(sink).as_deref()),
        deinterlacer: deinterlacer.to_owned(),
        rendered: stats.as_ref().and_then(|s| s.get("rendered").ok()),
        dropped: stats.as_ref().and_then(|s| s.get("dropped").ok()),
        average_fps: stats
            .as_ref()
            .and_then(|s| s.get::<f64>("average-rate").ok())
            .filter(|value| value.is_finite() && *value > 0.0),
        queue_buffers: queue.property("current-level-buffers"),
        queue_bytes: queue.property("current-level-bytes"),
        queue_ms: queue.property::<u64>("current-level-time") as f64 / 1_000_000.0,
        gstreamer: gst::version_string().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_broadcast_format_without_rounding_frame_rate() {
        gst::init().unwrap();
        let caps: gst::Caps = "video/x-raw,width=1440,height=1080,framerate=30000/1001,interlace-mode=interleaved,format=I420,pixel-aspect-ratio=4/3".parse().unwrap();
        let format = VideoFormat::from_caps(Some(&caps));
        assert_eq!(format.width, Some(1440));
        assert_eq!(format.interlace.as_deref(), Some("interleaved"));
        assert_eq!(format.pixel_aspect_ratio.as_deref(), Some("4:3"));
        assert!((format.fps.unwrap() - 29.97002997).abs() < 0.000001);
    }

    #[test]
    fn absent_or_variable_frame_rate_is_unknown() {
        gst::init().unwrap();
        assert!(VideoFormat::from_caps(None).width.is_none());
        let caps: gst::Caps = "video/x-raw,framerate=0/1".parse().unwrap();
        assert!(VideoFormat::from_caps(Some(&caps)).fps.is_none());
    }

    #[test]
    fn reads_running_pipeline_counters_and_caps() {
        gst::init().unwrap();
        // CPU-only fixture; no display, GPU, audio or network is used.
        let pipeline = gst::parse::launch("videotestsrc num-buffers=6 ! video/x-raw,format=I420,width=320,height=240,framerate=30/1 ! identity name=processor ! queue name=queue ! fakesink name=sink sync=false")
            .unwrap().downcast::<gst::Pipeline>().unwrap();
        pipeline.set_state(gst::State::Playing).unwrap();
        let message = pipeline.bus().unwrap().timed_pop_filtered(
            gst::ClockTime::from_seconds(5),
            &[gst::MessageType::Eos, gst::MessageType::Error],
        );
        let stats = snapshot(
            pipeline.upcast_ref(),
            &pipeline.by_name("processor").unwrap(),
            &pipeline.by_name("queue").unwrap(),
            &pipeline.by_name("sink").unwrap(),
            "Off",
        );
        pipeline.set_state(gst::State::Null).unwrap();
        assert!(matches!(
            message.as_ref().map(|m| m.view()),
            Some(gst::MessageView::Eos(_))
        ));
        assert_eq!(stats.rendered, Some(6));
        assert_eq!(stats.dropped, Some(0));
        assert_eq!(stats.output.width, Some(320));
        assert_eq!(stats.input.fps, Some(30.0));
    }

    #[test]
    fn stopped_pipeline_does_not_report_stale_frame_counts() {
        gst::init().unwrap();
        let pipeline = gst::Pipeline::new();
        let processor = gst::ElementFactory::make("identity").build().unwrap();
        let queue = gst::ElementFactory::make("queue").build().unwrap();
        let sink = gst::ElementFactory::make("fakesink").build().unwrap();
        let stats = snapshot(pipeline.upcast_ref(), &processor, &queue, &sink, "Off");
        assert!(stats.rendered.is_none());
        assert!(stats.output.width.is_none());
        assert_eq!(stats.queue_buffers, 0);
        assert!(serde_json::to_string(&stats).is_ok());
    }
}
