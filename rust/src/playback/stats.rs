//! On-demand snapshots of native counters and observed video metadata.
use super::{
    deinterlace::Mode,
    video_info::{Observation, Scan, Streams},
};
use gst::prelude::*;
use gstreamer as gst;
use gstreamer_base::{BaseSink, prelude::BaseSinkExt};
use gstreamer_video::VideoInterlaceMode;
use serde::Serialize;
use std::collections::BTreeMap;

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
    width: Option<u32>,
    height: Option<u32>,
    fps: Option<f64>,
    #[serde(serialize_with = "serialize_interlace")]
    interlace: Option<VideoInterlaceMode>,
    pixel_format: Option<String>,
    memory: Option<String>,
    pixel_aspect_ratio: Option<String>,
    scan: Scan,
}

impl VideoFormat {
    fn from_observation(observation: Observation) -> Self {
        let Observation::Negotiated { info, memory, scan } = observation else {
            return Self::default();
        };
        let rate = info.fps();
        let ratio = info.par();
        Self {
            width: Some(info.width()),
            height: Some(info.height()),
            fps: (rate.numer() > 0 && rate.denom() > 0)
                .then(|| f64::from(rate.numer()) / f64::from(rate.denom())),
            interlace: Some(info.interlace_mode()),
            pixel_format: Some(info.format().to_string()),
            memory: Some(memory),
            pixel_aspect_ratio: Some(format!("{}:{}", ratio.numer(), ratio.denom())),
            scan,
        }
    }
}

fn serialize_interlace<S: serde::Serializer>(
    mode: &Option<VideoInterlaceMode>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    mode.map(|mode| mode.to_string()).serialize(serializer)
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum DeinterlaceStatus {
    Disabled,
    Unknown,
    Passthrough,
    Active,
}

impl DeinterlaceStatus {
    fn new(
        mode: Mode,
        input: &VideoFormat,
        output: &VideoFormat,
        passthrough: Option<bool>,
    ) -> Self {
        if mode == Mode::Off {
            return Self::Disabled;
        }
        if input.scan == Scan::Unknown || output.scan == Scan::Unknown {
            return Self::Unknown;
        }
        match mode {
            Mode::Off => Self::Disabled,
            Mode::Yadif | Mode::Linear => {
                if input.scan == Scan::Interlaced
                    && output.interlace == Some(VideoInterlaceMode::Progressive)
                {
                    Self::Active
                } else {
                    Self::Passthrough
                }
            }
            // VA can keep filtering mixed caps even for progressive frames.
            // Report the native filter state, not whether filtering is needed.
            Mode::NvidiaGl | Mode::VaApi => match passthrough {
                Some(false) => Self::Active,
                Some(true) => Self::Passthrough,
                None => Self::Unknown,
            },
        }
    }
}

#[derive(Serialize)]
pub struct VideoStats {
    state: String,
    input: VideoFormat,
    output: VideoFormat,
    deinterlacer: String,
    deinterlace_status: DeinterlaceStatus,
    rendered: Option<u64>,
    dropped: Option<u64>,
    average_fps: Option<f64>,
    queue_buffers: u32,
    queue_bytes: u32,
    queue_ms: f64,
    render_delay_ms: f64,
    gstreamer: String,
    decoders: Vec<String>,
    decoder_thread_limits: BTreeMap<String, i32>,
    processor_passthrough: Option<bool>,
}

pub(super) fn snapshot(
    player: &gst::Element,
    processor: &gst::Element,
    queue: &gst::Element,
    sink: &gst::Element,
    mode: Mode,
    streams: &Streams,
) -> VideoStats {
    let state = player.current_state();
    let active = matches!(state, gst::State::Paused | gst::State::Playing);
    let format = |monitor: &super::video_info::Monitor| {
        if active {
            VideoFormat::from_observation(monitor.snapshot())
        } else {
            VideoFormat::default()
        }
    };
    let stats = active.then(|| sink_stats(sink)).flatten();
    let (decoders, decoder_thread_limits) = if active {
        decoder_details(player)
    } else {
        Default::default()
    };
    let input = format(&streams.input);
    let output = format(&streams.output);
    let processor_passthrough = active
        .then(|| {
            processor
                .downcast_ref::<gstreamer_base::BaseTransform>()
                .map(gstreamer_base::prelude::BaseTransformExt::is_passthrough)
        })
        .flatten();
    VideoStats {
        state: format!("{state:?}"),
        deinterlace_status: DeinterlaceStatus::new(mode, &input, &output, processor_passthrough),
        input,
        output,
        deinterlacer: mode.label().to_owned(),
        rendered: stats.as_ref().and_then(|s| s.get("rendered").ok()),
        dropped: stats.as_ref().and_then(|s| s.get("dropped").ok()),
        average_fps: stats
            .as_ref()
            .and_then(|s| s.get::<f64>("average-rate").ok())
            .filter(|value| value.is_finite() && *value > 0.0),
        queue_buffers: queue.property("current-level-buffers"),
        queue_bytes: queue.property("current-level-bytes"),
        queue_ms: queue.property::<u64>("current-level-time") as f64 / 1_000_000.0,
        render_delay_ms: sink.property::<u64>("render-delay") as f64 / 1_000_000.0,
        gstreamer: gst::version_string().to_string(),
        decoders,
        decoder_thread_limits,
        processor_passthrough,
    }
}

fn decoder_details(player: &gst::Element) -> (Vec<String>, BTreeMap<String, i32>) {
    let Some(bin) = player.downcast_ref::<gst::Bin>() else {
        return Default::default();
    };
    let mut names = Vec::new();
    let mut threads = BTreeMap::new();
    let mut elements = bin.iterate_recurse();
    while let Ok(Some(element)) = elements.next() {
        if let Some(factory) = element.factory()
            && factory
                .has_type(gst::ElementFactoryType::DECODER | gst::ElementFactoryType::MEDIA_VIDEO)
        {
            names.push(factory.name().to_string());
            if factory.plugin_name().as_deref() == Some("libav")
                && element.find_property("max-threads").is_some()
            {
                threads.insert(factory.name().to_string(), element.property("max-threads"));
            }
        }
    }
    names.sort();
    names.dedup();
    (names, threads)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playback::deinterlace::Mode;

    // Ensure native streaming tasks stop even when a Result/assertion exits a test.
    struct Running(gst::Pipeline);
    impl Drop for Running {
        fn drop(&mut self) {
            if let Err(error) = self.0.set_state(gst::State::Null) {
                tracing::error!(
                    error = &error as &dyn std::error::Error,
                    "Failed to stop video statistics test pipeline"
                );
            }
        }
    }

    fn format_from_caps(caps: &gst::CapsRef) -> VideoFormat {
        VideoFormat::from_observation(Observation::Negotiated {
            info: gstreamer_video::VideoInfo::from_caps(caps).unwrap(),
            memory: caps.features(0).unwrap().to_string(),
            scan: Scan::Unknown,
        })
    }

    #[test]
    fn reads_exact_format_and_handles_unknown_rate() -> Result<(), Box<dyn std::error::Error>> {
        gst::init()?;
        let caps: gst::Caps = "video/x-raw,width=1440,height=1080,framerate=30000/1001,interlace-mode=interleaved,format=I420,pixel-aspect-ratio=4/3".parse()?;
        let format = format_from_caps(&caps);
        assert_eq!(format.width, Some(1440));
        assert_eq!(format.pixel_aspect_ratio.as_deref(), Some("4:3"));
        assert!((format.fps.ok_or("missing fps")? - 29.97002997).abs() < 0.000001);
        assert_eq!(format.interlace, Some(VideoInterlaceMode::Interleaved));
        let unknown: gst::Caps =
            "video/x-raw,format=I420,width=320,height=240,framerate=0/1".parse()?;
        let defaults = format_from_caps(&unknown);
        assert!(defaults.fps.is_none());
        assert_eq!(defaults.interlace, Some(VideoInterlaceMode::Progressive));
        assert_eq!(defaults.pixel_aspect_ratio.as_deref(), Some("1:1"));
        assert!(
            VideoFormat::from_observation(Observation::Unavailable)
                .width
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn gpu_filter_activity_is_not_inferred_from_a_progressive_mixed_frame() {
        gst::init().unwrap();
        let caps: gst::Caps = "video/x-raw,format=NV12,width=320,height=240,interlace-mode=mixed"
            .parse()
            .unwrap();
        let mut input = format_from_caps(&caps);
        input.scan = Scan::Progressive;
        let mut output = format_from_caps(&caps);
        output.scan = Scan::Progressive;
        for mode in [Mode::NvidiaGl, Mode::VaApi] {
            assert_eq!(
                DeinterlaceStatus::new(mode, &input, &output, Some(false)),
                DeinterlaceStatus::Active
            );
            assert_eq!(
                DeinterlaceStatus::new(mode, &input, &output, Some(true)),
                DeinterlaceStatus::Passthrough
            );
            assert_eq!(
                DeinterlaceStatus::new(mode, &input, &output, None),
                DeinterlaceStatus::Unknown
            );
            let pending = VideoFormat::default();
            assert_eq!(
                DeinterlaceStatus::new(mode, &pending, &output, Some(false)),
                DeinterlaceStatus::Unknown
            );
        }
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
            let streams = Streams {
                input: super::super::video_info::Monitor::observe(
                    &processor
                        .static_pad("sink")
                        .ok_or("missing processor pad")?,
                ),
                output: super::super::video_info::Monitor::observe(
                    &sink.static_pad("sink").ok_or("missing sink pad")?,
                ),
            };
            let elements = [&source, &filter, &processor, &queue, &sink];
            pipeline.0.add_many(elements)?;
            gst::Element::link_many(elements)?;
            for interlaced in [true, false, true] {
                let mut current_caps = caps.clone();
                current_caps.make_mut().structure_mut(0).unwrap().set(
                    "interlace-mode",
                    if interlaced {
                        "interleaved"
                    } else {
                        "progressive"
                    },
                );
                filter.set_property("caps", &current_caps);
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
                    mode,
                    &streams,
                );
                assert_eq!(running.input.fps, Some(30.0));
                assert_eq!(
                    running.input.scan,
                    if interlaced {
                        Scan::Interlaced
                    } else {
                        Scan::Progressive
                    }
                );
                assert_eq!(
                    running.deinterlace_status,
                    if mode == Mode::Off {
                        DeinterlaceStatus::Disabled
                    } else if interlaced {
                        DeinterlaceStatus::Active
                    } else {
                        DeinterlaceStatus::Passthrough
                    }
                );
                assert_eq!(
                    running.output.fps,
                    Some(if mode == Mode::Off || !interlaced {
                        30.0
                    } else {
                        60.0
                    })
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
                    mode,
                    &streams,
                );
                assert!(stopped.rendered.is_none());
                assert!(frame_counters(&sink).is_none());
                assert!(stopped.output.width.is_none());
                assert_eq!(stopped.input.scan, Scan::Unknown);
                assert_eq!(stopped.output.scan, Scan::Unknown);
                assert_eq!(
                    stopped.deinterlace_status,
                    if mode == Mode::Off {
                        DeinterlaceStatus::Disabled
                    } else {
                        DeinterlaceStatus::Unknown
                    }
                );
                assert!(serde_json::to_string(&stopped).is_ok());
            }
        }
        Ok(())
    }
}
