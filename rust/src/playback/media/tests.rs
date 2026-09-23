//! Explicit CPU decoders and memory outputs: never accesses playback hardware.
use super::*;
use crate::playback::{
    recording::{Loader, Recording, Request},
    timeline::{Controller, Phase, Resume, StartPosition},
};
use std::{
    path::Path,
    time::{Duration, Instant},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;
const DEADLINE: Duration = Duration::from_secs(5);
struct Pipeline(gst::Pipeline);
impl Drop for Pipeline {
    fn drop(&mut self) {
        let _ = self.0.set_state(gst::State::Null);
    }
}
fn load(request: Request) -> Result<Recording, Box<dyn std::error::Error>> {
    let mut loader = Loader::default();
    loader.begin(request);
    let deadline = Instant::now() + DEADLINE;
    loop {
        if let Some((_, outcome)) = loader.poll() {
            return Ok(outcome?);
        }
        assert!(Instant::now() < deadline, "inspection timeout");
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn settle(control: &mut Controller, pipeline: &gst::Pipeline) -> TestResult {
    let deadline = Instant::now() + DEADLINE;
    while matches!(control.phase(), Phase::Seeking(_)) {
        control.poll(pipeline.upcast_ref())?;
        if let Some(message) = pipeline
            .bus()
            .unwrap()
            .pop_filtered(&[gst::MessageType::Error])
        {
            return Err(format!("{message:?}").into());
        }
        assert!(Instant::now() < deadline, "seek timeout");
        std::thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}

#[test]
fn local_and_http_containers_decode_both_tracks_and_seek_with_the_shared_controller() -> TestResult
{
    gst::init()?;
    for (codec, parser, decoder) in [
        ("h264", "h264parse", "avdec_h264"),
        ("hevc", "h265parse", "avdec_h265"),
    ] {
        for (extension, demux) in [("mp4", "qtdemux"), ("mkv", "matroskademux")] {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("../tests/fixtures/media-{codec}.{extension}"));
            for http in [false, true] {
                eprintln!("{codec}/{extension} http={http}");
                let server = http
                    .then(|| crate::playback::recording::serve_ts(std::fs::read(&path).unwrap()));
                let request = match &server {
                    Some(server) => Request::from_url(&format!(
                        "{}/api/videos/123?token=private",
                        server.uri()
                    ))?,
                    None => Request::open(path.clone()),
                };
                let file = load(request)?;
                assert!(!format!("{file:?}").contains("private"));
                let Recording::Media(file) = file else {
                    panic!("media detected as TS")
                };
                let pipeline = Pipeline(gst::parse::launch(&format!(
                    "appsrc name=source ! {demux} name=demux demux.video_0 ! queue ! {parser} ! {decoder} ! appsink name=video max-buffers=1 drop=true sync=true demux.audio_0 ! queue ! aacparse ! avdec_aac ! audioconvert ! scaletempo ! appsink name=audio max-buffers=1 drop=true sync=true"
                ))?.downcast::<gst::Pipeline>().map_err(|_| "pipeline")?);
                let source = pipeline
                    .0
                    .by_name("source")
                    .unwrap()
                    .downcast::<AppSrc>()
                    .unwrap();
                let registrations = Subscriptions::default();
                configure(
                    &source,
                    &file,
                    Arc::new(AtomicBool::new(false)),
                    &registrations,
                );
                let video = pipeline.0.by_name("video").unwrap();
                let audio = pipeline
                    .0
                    .by_name("audio")
                    .unwrap()
                    .downcast::<gstreamer_app::AppSink>()
                    .unwrap();
                let mut control = Controller::new(&video, StartPosition::Beginning)?;
                control.pause(pipeline.0.upcast_ref(), Resume::Paused)?;
                let state = pipeline.0.state(gst::ClockTime::from_seconds(5));
                if state.0.is_err() {
                    panic!(
                        "preroll failed: {:?}",
                        pipeline
                            .0
                            .bus()
                            .unwrap()
                            .pop_filtered(&[gst::MessageType::Error])
                    );
                }
                assert_eq!(state.1, gst::State::Paused);
                let sample = audio
                    .try_pull_preroll(gst::ClockTime::SECOND)
                    .ok_or("no decoded audio")?;
                assert_eq!(
                    sample
                        .caps()
                        .unwrap()
                        .structure(0)
                        .unwrap()
                        .get::<i32>("rate")?,
                    48000
                );
                assert!(
                    sample
                        .buffer()
                        .unwrap()
                        .map_readable()?
                        .iter()
                        .any(|&byte| byte != 0)
                );
                control.poll(pipeline.0.upcast_ref())?;
                assert!(
                    control
                        .snapshot()
                        .duration
                        .is_some_and(|time| (12..=13).contains(&time.seconds()))
                );
                for target in [8000.0, 2000.0, 9000.0] {
                    control.prepare(pipeline.0.upcast_ref())?.seek(target)?;
                    settle(&mut control, &pipeline.0)?;
                    assert_eq!(control.phase(), Phase::Paused);
                    let actual = control.snapshot().position.ok_or("position")?.mseconds() as f64;
                    assert!((actual - target).abs() < 250.0, "{target}: {actual}");
                }
                control
                    .prepare(pipeline.0.upcast_ref())?
                    .set_rate(crate::playback::speed::Rate::checked(15).unwrap())?;
                settle(&mut control, &pipeline.0)?;
                assert_eq!(control.phase(), Phase::Paused);
                assert_eq!(
                    control.speed().applied,
                    crate::playback::speed::Rate::checked(15).unwrap()
                );
                pipeline.0.set_state(gst::State::Null)?;
                registrations.close();
            }
        }
    }
    Ok(())
}

#[test]
fn content_detection_ignores_extensions_and_replay_revalidates() -> TestResult {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("録画 #100%.ts");
    std::fs::write(
        &path,
        include_bytes!("../../../../tests/fixtures/media-h264.mp4"),
    )?;
    let recording = load(Request::open(path.clone()))?;
    assert!(matches!(recording, Recording::Media(_)));
    assert_eq!(recording.name(), "録画 #100%.ts");
    std::fs::write(&path, b"<html>login page</html>")?;
    assert!(load(recording.replay()).is_err());
    Ok(())
}

#[test]
fn playbin_typefinds_the_byte_source_once_and_supports_container_seeks() -> TestResult {
    gst::init()?;
    for name in [
        "media-h264.mp4",
        "media-h264.mkv",
        "media-hevc.mp4",
        "media-hevc.mkv",
    ] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures")
            .join(name);
        let Recording::Media(file) = load(Request::open(path))? else {
            panic!("expected general media")
        };
        let video = gstreamer_app::AppSink::builder()
            .max_buffers(1)
            .drop(true)
            .sync(true)
            .build();
        let audio = gstreamer_app::AppSink::builder()
            .max_buffers(1)
            .drop(true)
            .sync(true)
            .build();
        let playbin = gst::ElementFactory::make("playbin3")
            .property("video-sink", &video)
            .property("audio-sink", &audio)
            .build()?;
        // Explicit CPU-only test mode, with memory outputs; never probes output
        // hardware or uses automatic device sinks.
        playbin.set_property_from_str("flags", "video+audio+force-sw-decoders");
        let input = super::Input::new(&playbin, &file);
        // Stop the native pipeline before releasing the input on every exit path.
        let pipeline = Pipeline(playbin.downcast().map_err(|_| "playbin pipeline")?);
        pipeline.0.set_property("uri", "appsrc://");
        let mut control = Controller::new(video.upcast_ref(), StartPosition::Beginning)?;
        control.pause(pipeline.0.upcast_ref(), Resume::Paused)?;
        let (_, state, _) = pipeline.0.state(gst::ClockTime::from_seconds(5));
        if state != gst::State::Paused {
            return Err(format!(
                "{name}: preroll failed: {:?}",
                pipeline
                    .0
                    .bus()
                    .unwrap()
                    .pop_filtered(&[gst::MessageType::Error])
            )
            .into());
        }
        assert!(video.try_pull_preroll(gst::ClockTime::SECOND).is_some());
        assert!(audio.try_pull_preroll(gst::ClockTime::SECOND).is_some());
        control.prepare(pipeline.0.upcast_ref())?.seek(8000.0)?;
        settle(&mut control, &pipeline.0)?;
        let position = control.snapshot().position.ok_or("position")?.mseconds() as f64;
        assert!((position - 8000.0).abs() < 250.0, "{name}: {position}");
        drop(pipeline);
        drop(input);
    }
    Ok(())
}
