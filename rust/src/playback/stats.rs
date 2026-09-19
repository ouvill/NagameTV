//! On-demand snapshots. No frame retention, probes, timers or historical samples.
use gst::prelude::*;
use gstreamer as gst;
use gstreamer_base::{BaseSink, prelude::BaseSinkExt};
use serde::Serialize;

/// Sink rendering calls, not proof that Qt presented a new image on screen.
/// Native counters can reset on stream changes or flushes; these are not
/// cumulative totals for the file or playback session.
#[derive(Debug, PartialEq, Eq)]
pub struct FrameCounters {
    pub rendered: u64,
    pub dropped: u64,
}

fn sink_stats(sink: &gst::Element) -> Option<gst::Structure> {
    if !matches!(
        sink.current_state(),
        gst::State::Paused | gst::State::Playing
    ) {
        return None;
    }
    Some(sink.downcast_ref::<BaseSink>()?.stats())
}

/// Sample native counters without retaining buffers or collecting display metadata.
/// READY/NULL and unsupported sinks are unknown, rather than a synthetic zero.
pub fn frame_counters(sink: &gst::Element) -> Option<FrameCounters> {
    let stats = sink_stats(sink)?;
    Some(FrameCounters {
        rendered: stats.get("rendered").ok()?,
        dropped: stats.get("dropped").ok()?,
    })
}

#[derive(Default, Serialize)]
pub struct VideoFormat {
    width: Option<i32>,
    height: Option<i32>,
    fps: Option<f64>,
    interlace: Option<String>,
    pixel_format: Option<String>,
    memory: Option<String>,
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
            memory: caps
                .and_then(|caps| caps.features(0))
                .map(|features| features.to_string()),
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
    decoders: Vec<String>,
    processor_passthrough: Option<bool>,
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
    let stats = active.then(|| sink_stats(sink)).flatten();
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
        decoders: if active {
            decoder_names(player)
        } else {
            Vec::new()
        },
        processor_passthrough: active
            .then(|| {
                processor
                    .downcast_ref::<gstreamer_base::BaseTransform>()
                    .map(gstreamer_base::prelude::BaseTransformExt::is_passthrough)
            })
            .flatten(),
    }
}

fn decoder_names(player: &gst::Element) -> Vec<String> {
    let Some(bin) = player.downcast_ref::<gst::Bin>() else {
        return Vec::new();
    };
    let mut names = Vec::new();
    let mut elements = bin.iterate_recurse();
    while let Ok(Some(element)) = elements.next() {
        if let Some(factory) = element.factory()
            && factory
                .has_type(gst::ElementFactoryType::DECODER | gst::ElementFactoryType::MEDIA_VIDEO)
        {
            names.push(factory.name().to_string());
        }
    }
    names.sort();
    names.dedup();
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playback::deinterlace::Mode;

    // Ensure native streaming tasks stop even when a Result/assertion exits a test.
    struct Running(gst::Pipeline);
    impl Drop for Running {
        fn drop(&mut self) {
            let _ = self.0.set_state(gst::State::Null);
        }
    }

    #[test]
    fn reads_exact_format_and_handles_unknown_rate() -> Result<(), Box<dyn std::error::Error>> {
        gst::init()?;
        let caps: gst::Caps = "video/x-raw,width=1440,height=1080,framerate=30000/1001,interlace-mode=interleaved,format=I420,pixel-aspect-ratio=4/3".parse()?;
        let format = VideoFormat::from_caps(Some(&caps));
        assert_eq!(format.width, Some(1440));
        assert_eq!(format.pixel_aspect_ratio.as_deref(), Some("4:3"));
        assert!((format.fps.ok_or("missing fps")? - 29.97002997).abs() < 0.000001);
        let unknown: gst::Caps = "video/x-raw,framerate=0/1".parse()?;
        assert!(VideoFormat::from_caps(Some(&unknown)).fps.is_none());
        assert!(VideoFormat::from_caps(None).width.is_none());
        Ok(())
    }

    #[test]
    fn each_mode_processes_frames_and_stopped_snapshot_clears_counts()
    -> Result<(), Box<dyn std::error::Error>> {
        gst::init()?;
        // CPU-only video into a fakesink: no display, GPU, audio or network.
        for mode in [Mode::Yadif, Mode::Linear, Mode::Off] {
            let pipeline = Running(gst::Pipeline::new());
            let source = gst::ElementFactory::make("videotestsrc")
                .property("num-buffers", 6_i32)
                .build()?;
            let caps: gst::Caps = "video/x-raw,format=I420,width=320,height=240,framerate=30/1,interlace-mode=interleaved".parse()?;
            let filter = gst::ElementFactory::make("capsfilter")
                .property("caps", &caps)
                .build()?;
            let processor = mode.build()?;
            let queue = gst::ElementFactory::make("queue").build()?;
            let sink = gst::ElementFactory::make("fakesink")
                .property("sync", false)
                .build()?;
            let elements = [&source, &filter, &processor, &queue, &sink];
            pipeline.0.add_many(elements)?;
            gst::Element::link_many(elements)?;
            pipeline.0.set_state(gst::State::Playing)?;
            let message = pipeline.0.bus().ok_or("missing bus")?.timed_pop_filtered(
                gst::ClockTime::from_seconds(5),
                &[gst::MessageType::Eos, gst::MessageType::Error],
            );
            assert!(
                matches!(
                    message.as_ref().map(|m| m.view()),
                    Some(gst::MessageView::Eos(_))
                ),
                "{message:?}"
            );
            let running = snapshot(
                pipeline.0.upcast_ref(),
                &processor,
                &queue,
                &sink,
                mode.label(),
            );
            assert_eq!(running.input.fps, Some(30.0));
            assert_eq!(
                running.output.fps,
                Some(if mode == Mode::Off { 30.0 } else { 60.0 })
            );
            assert!(running.rendered.is_some_and(|n| n > 0));
            let completed = frame_counters(&sink).ok_or("missing native counters")?;
            assert_eq!(Some(completed.rendered), running.rendered);
            assert_eq!(completed.dropped, 0);
            // EOS does not leave PLAYING. Counters remain unchanged when no new
            // frames arrive, even though a state-only observation says PLAYING.
            assert_eq!(frame_counters(&sink), Some(completed));
            assert_eq!(running.dropped, Some(0));
            assert_eq!(running.output.width, Some(320));
            pipeline.0.set_state(gst::State::Ready)?;
            let stopped = snapshot(
                pipeline.0.upcast_ref(),
                &processor,
                &queue,
                &sink,
                mode.label(),
            );
            assert!(stopped.rendered.is_none());
            assert!(frame_counters(&sink).is_none());
            assert!(stopped.output.width.is_none());
            assert!(serde_json::to_string(&stopped).is_ok());
        }
        Ok(())
    }
}
