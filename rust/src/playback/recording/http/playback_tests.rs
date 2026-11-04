//! Real demuxers and CPU decoders measure HTTP locality at normal playback speed.
use super::*;
use crate::playback::{
    input, media,
    recording::{Recording, source::Location},
};
use gstreamer::{self as gst, prelude::*};
use gstreamer_app::AppSink;
use std::time::Instant;

const OBSERVE: Duration = Duration::from_secs(30);
const START_TIMEOUT: Duration = Duration::from_secs(8);
const POLL: Duration = Duration::from_millis(20);

struct Stop(gst::Pipeline);
impl Drop for Stop {
    fn drop(&mut self) {
        if let Err(error) = self.0.set_state(gst::State::Null) {
            tracing::error!(%error, "Could not stop HTTP locality test pipeline");
        }
    }
}
enum Input {
    Media(Box<media::Input>),
    Ts(Box<input::Input>),
}
impl Input {
    fn control(&self) -> Option<Control> {
        match self {
            Self::Media(input) => input.http_control(),
            Self::Ts(input) => input.http_control(),
        }
    }
}

fn counts(server: &wiremock::MockServer) -> (usize, usize) {
    let requests = runtime()
        .unwrap()
        .block_on(server.received_requests())
        .unwrap();
    let continuous = requests
        .iter()
        .filter(|request| {
            request.headers[header::RANGE]
                .to_str()
                .unwrap()
                .ends_with('-')
        })
        .count();
    (requests.len(), continuous)
}

fn observe(bytes: &[u8], mp4: bool) -> Result<(), Box<dyn std::error::Error>> {
    gst::init()?;
    let server = super::tests::serve_ts(bytes.to_vec());
    let recording = Recording::inspect(
        &Location::Http(Url::parse(&format!("{}/api/videos/123", server.uri()))?),
        &Arc::new(AtomicBool::new(false)),
    )?;
    let video = AppSink::builder()
        .max_buffers(1)
        .drop(true)
        .sync(true)
        .build();
    let audio = AppSink::builder()
        .max_buffers(1)
        .drop(true)
        .sync(true)
        .build();
    // These explicit memory sinks and software decoders never use output hardware.
    let pipeline = gst::ElementFactory::make("playbin3")
        .property("video-sink", &video)
        .property("audio-sink", &audio)
        .build()?
        .downcast::<gst::Pipeline>()
        .map_err(|_| "pipeline")?;
    pipeline.set_property_from_str("flags", "video+audio+force-sw-decoders");
    let input = match recording {
        Recording::Media(file) => {
            assert!(mp4);
            Input::Media(Box::new(media::Input::new(pipeline.upcast_ref(), &file)))
        }
        Recording::Transport(file) => {
            assert!(!mp4);
            Input::Ts(Box::new(input::Input::recording(
                pipeline.upcast_ref(),
                &file,
                false,
            )?))
        }
    };
    let pipeline = Stop(pipeline);
    pipeline.0.set_property("uri", "appsrc://");
    pipeline.0.set_state(gst::State::Playing)?;
    let bus = pipeline.0.bus().ok_or("bus")?;
    let mut started = None;
    let deadline = Instant::now() + START_TIMEOUT;
    let mut initial = None;
    let mut at_first_frame = None;
    let mut saw_audio = false;
    let mut last_pts = gst::ClockTime::ZERO;
    loop {
        if let Some(message) = bus.pop_filtered(&[gst::MessageType::Error]) {
            return Err(format!("HTTP locality playback failed: {message:?}").into());
        }
        if let Some(duration) = pipeline.0.query_duration::<gst::ClockTime>()
            && let Some(control) = input.control()
        {
            control.duration(Duration::from_nanos(duration.nseconds()));
        }
        if let Some(sample) = video.try_pull_sample(gst::ClockTime::ZERO) {
            if let Some(pts) = sample.buffer().and_then(|buffer| buffer.pts()) {
                last_pts = pts;
            }
            if started.is_none() {
                at_first_frame = Some(counts(&server));
                started = Some(Instant::now());
            }
        }
        saw_audio |= audio.try_pull_sample(gst::ClockTime::ZERO).is_some();
        match started {
            None => assert!(Instant::now() < deadline, "HTTP playback never started"),
            Some(start) => {
                if start.elapsed() >= Duration::from_secs(2) && initial.is_none() {
                    initial = Some(counts(&server));
                }
                if start.elapsed() >= OBSERVE {
                    break;
                }
            }
        }
        std::thread::sleep(POLL);
    }
    let final_count = counts(&server);
    let warm = initial.ok_or("warm-up count")?;
    let startup = at_first_frame.ok_or("startup count")?;
    eprintln!(
        "HTTP locality {}: 30 seconds, GETs={} open_GETs={} startup_GETs={} playback_GETs={} steady_GETs={} steady_open_GETs={} last_video_ms={}",
        if mp4 { "qtdemux/HEVC MP4" } else { "MPEG-TS" },
        final_count.0,
        final_count.1,
        startup.0,
        final_count.0 - startup.0,
        final_count.0 - warm.0,
        final_count.1 - warm.1,
        last_pts.mseconds()
    );
    assert!(
        last_pts >= gst::ClockTime::from_seconds(29),
        "playback stopped advancing"
    );
    assert!(saw_audio, "no decoded audio");
    // Typefinding may inspect both ends before the first decoded frame.
    // Its legitimate startup requests are separate from 30 s of sample reads.
    assert!(
        final_count.0 - startup.0 <= 3,
        "ordinary playback issued too many GETs: {startup:?} -> {final_count:?}"
    );
    assert!(
        final_count.0 - warm.0 <= 1,
        "steady playback restarted HTTP: {warm:?} -> {final_count:?}"
    );
    assert!(
        final_count.1 - startup.1 <= 3,
        "too many continuous GETs during playback: {startup:?} -> {final_count:?}"
    );
    Ok(())
}

#[test]
fn interleaved_mp4_uses_few_gets_during_thirty_seconds_of_qtdemux_playback()
-> Result<(), Box<dyn std::error::Error>> {
    observe(
        include_bytes!("../../../../../tests/fixtures/media-http-interleaved.mp4"),
        true,
    )
}

#[test]
fn transport_stream_uses_few_gets_during_thirty_seconds_of_playback()
-> Result<(), Box<dyn std::error::Error>> {
    // Keep the real PCR/PTS and payload, but add null packets so this fixture
    // exercises a broadcast-rate stream and evicts old history during playback.
    const PACKET_BYTES: usize = 188;
    const PACKET_EXPANSION: usize = 64;
    let fixture = include_bytes!("../../../../../tests/fixtures/recording-seek.ts");
    let mut padding = [0xff; PACKET_BYTES];
    padding[..4].copy_from_slice(&[0x47, 0x1f, 0xff, 0x10]);
    let mut bytes = Vec::with_capacity(fixture.len() * PACKET_EXPANSION);
    for packet in fixture.as_chunks::<PACKET_BYTES>().0 {
        bytes.extend_from_slice(packet);
        for _ in 1..PACKET_EXPANSION {
            bytes.extend_from_slice(&padding);
        }
    }
    observe(&bytes, false)
}
