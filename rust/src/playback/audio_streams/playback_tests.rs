//! Explicit CPU integration fixture: native playbin3 selection with measured samples.
use super::*;
use std::{
    sync::{
        Arc,
        atomic::{AtomicU32, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

struct Playing(gst::Element);
impl Drop for Playing {
    fn drop(&mut self) {
        if let Err(error) = self.0.set_state(gst::State::Null) {
            eprintln!("Audio fixture shutdown: {error}");
        }
    }
}

fn pump_until(
    player: &gst::Element,
    streams: &mut Streams,
    condition: impl Fn(&Streams) -> bool,
) -> TestResult {
    let bus = player.bus().ok_or("fixture bus missing")?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(message) = bus.timed_pop(gst::ClockTime::from_mseconds(10)) {
            if let gst::MessageView::Error(error) = message.view() {
                return Err(format!("{}: {:?}", error.error(), error.debug()).into());
            }
            streams.observe(player, &message)?;
        }
        if condition(streams) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err("native selection/sample confirmation timed out".into());
        }
    }
}

#[test]
fn switching_native_audio_changes_samples_while_video_continues() -> TestResult {
    gst::init()?;
    let peak = Arc::new(AtomicU32::new(0));
    let audio_buffers = Arc::new(AtomicU64::new(0));
    let video_buffers = Arc::new(AtomicU64::new(0));
    // Deliberately consume generated test media in CPU sinks, never auto-select hardware.
    let audio = gst::ElementFactory::make("fakesink")
        .property("sync", true)
        .property("signal-handoffs", true)
        .build()?;
    let measured = peak.clone();
    let delivered = audio_buffers.clone();
    audio.connect("handoff", false, move |values| {
        if let Some(buffer) = values
            .get(1)
            .and_then(|value| value.get::<gst::Buffer>().ok())
            && let Ok(bytes) = buffer.map_readable()
        {
            let maximum = bytes
                .as_slice()
                .as_chunks::<4>()
                .0
                .iter()
                .map(|sample| f32::from_le_bytes(*sample).abs())
                .fold(0_f32, f32::max);
            measured.store(maximum.to_bits(), Ordering::Relaxed);
            delivered.fetch_add(1, Ordering::Release);
        }
        None
    });
    let video = gst::ElementFactory::make("fakesink")
        .property("sync", true)
        .property("signal-handoffs", true)
        .build()?;
    let rendered = video_buffers.clone();
    video.connect("handoff", false, move |_| {
        rendered.fetch_add(1, Ordering::Release);
        None
    });
    let routing = crate::playback::audio_routing::Routing::default();
    let filter = routing.filter()?;
    let player = gst::ElementFactory::make("playbin3")
        .property("audio-sink", &audio).property("video-sink", &video)
        .property("audio-filter", &filter)
        .property("uri", "testbin://audio,volume=0.1,is-live=true+audio,volume=0.8,is-live=true+video,is-live=true,caps=[video/x-raw,width=160,height=90,framerate=30/1]")
        .build()?;
    let guard = Playing(player.clone());
    let mut streams = Streams::default();
    player.set_state(gst::State::Playing)?;
    pump_until(&player, &mut streams, |streams| {
        let tracks = streams.tracks();
        tracks.len() == 2
            && tracks.iter().any(|track| track.selected)
            && audio_buffers.load(Ordering::Acquire) > 2
    })?;
    let first_peak = f32::from_bits(peak.load(Ordering::Relaxed));
    let initial = streams
        .tracks()
        .into_iter()
        .find(|track| track.selected)
        .ok_or("no selected audio")?
        .id;
    let other = streams
        .tracks()
        .into_iter()
        .find(|track| !track.selected)
        .ok_or("no second audio")?
        .id;
    // Determine the expected amplitude from the measured first track, not catalog order.
    assert!((0.07..0.13).contains(&first_peak) || (0.65..0.9).contains(&first_peak));
    let expected = if first_peak < 0.5 {
        0.65..0.9
    } else {
        0.07..0.13
    };
    let initial_range = if first_peak < 0.5 {
        0.07..0.13
    } else {
        0.65..0.9
    };
    for (target, range) in [(&other, expected), (&initial, initial_range)] {
        let audio_before = audio_buffers.load(Ordering::Acquire);
        let video_before = video_buffers.load(Ordering::Acquire);
        streams.select(&player, target)?;
        pump_until(&player, &mut streams, |streams| {
            streams
                .tracks()
                .iter()
                .any(|track| track.id == *target && track.selected)
                && audio_buffers.load(Ordering::Acquire) > audio_before + 3
                && video_buffers.load(Ordering::Acquire) > video_before + 3
                && range.contains(&f32::from_bits(peak.load(Ordering::Relaxed)))
        })?;
    }
    player.set_state(gst::State::Null)?;
    drop(guard);
    Ok(())
}
