//! CPU-only media sinks and libass; no Qt application or hardware resource.
use super::*;
use std::{
    path::Path,
    time::{Duration, Instant},
};
const DEADLINE: Duration = Duration::from_secs(8);
type TestResult = Result<(), Box<dyn std::error::Error>>;
struct Pipeline(gst::Element);
impl Drop for Pipeline {
    fn drop(&mut self) {
        let _ = self.0.set_state(gst::State::Null);
    }
}
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures")
        .join(name)
}
fn visible(frame: &Frame) -> bool {
    frame.pixels.as_chunks::<4>().0.iter().any(|p| p[3] != 0)
}
fn seek(player: &gst::Element, position: u64) -> TestResult {
    while player.bus().unwrap().pop().is_some() {}
    player.seek_simple(
        gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE,
        gst::ClockTime::from_mseconds(position),
    )?;
    let message = player
        .bus()
        .unwrap()
        .timed_pop_filtered(
            gst::ClockTime::from_seconds(8),
            &[gst::MessageType::AsyncDone, gst::MessageType::Error],
        )
        .ok_or("seek timed out")?;
    if let gst::MessageView::Error(error) = message.view() {
        return Err(format!("{} {:?}", error.error(), error.debug()).into());
    }
    Ok(())
}
fn wait_frame(
    session: &mut Session,
    milliseconds: u64,
    expected: bool,
) -> Result<Frame, Box<dyn std::error::Error>> {
    let deadline = Instant::now() + DEADLINE;
    loop {
        if let Some(frame) =
            session.render(gst::ClockTime::from_mseconds(milliseconds), 640, 384)?
            && visible(&frame) == expected
        {
            return Ok(frame);
        }
        if Instant::now() >= deadline {
            return Err(format!("subtitle at {milliseconds}ms expected visible={expected}").into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
#[test]
fn empty_subtitle_track_clears_once_without_repainting() -> TestResult {
    gst::init()?;
    let player = gst::ElementFactory::make("playbin3").build()?;
    let mut session = Session::start(&player, None)?;
    let render = |session: &mut Session, milliseconds| {
        session.render(gst::ClockTime::from_mseconds(milliseconds), 640, 384)
    };
    assert!(!visible(&render(&mut session, 0)?.expect("initial clear")));
    for milliseconds in [16, 32, 1000] {
        assert!(
            render(&mut session, milliseconds)?.is_none(),
            "an empty subtitle track must not rebuild a transparent image on every poll"
        );
    }
    session.invalidate();
    assert!(!visible(
        &render(&mut session, 2000)?.expect("explicit clear")
    ));
    assert!(render(&mut session, 2016)?.is_none());

    let script = Script::load(&fixture("media-subtitles.srt"))?;
    let mut session = Session::start(&player, Some(script))?;
    assert!(visible(&render(&mut session, 2000)?.expect("external cue")));
    session.select_embedded()?;
    assert!(!visible(
        &render(&mut session, 2000)?.expect("clear old cue")
    ));
    assert!(render(&mut session, 2016)?.is_none());
    Ok(())
}
#[test]
fn embedded_subtitles_follow_segments_and_track_selection() -> TestResult {
    gst::init()?;
    let pipeline = Pipeline(gst::ElementFactory::make("playbin3").build()?);
    let video = gst::ElementFactory::make("fakesink")
        .property("sync", true)
        .build()?;
    let audio = gst::ElementFactory::make("fakesink")
        .property("sync", true)
        .build()?;
    pipeline
        .0
        .set_property_from_str("flags", "video+audio+text+force-sw-decoders");
    pipeline.0.set_property("video-sink", video);
    pipeline.0.set_property("audio-sink", audio);
    for name in ["media-subtitles.mp4", "media-subtitles.mkv"] {
        let mut session = Session::start(&pipeline.0, None)?;
        pipeline.0.set_property(
            "uri",
            url::Url::from_file_path(fixture(name)).unwrap().as_str(),
        );
        pipeline.0.set_state(gst::State::Paused)?;
        let (state, _, _) = pipeline.0.state(gst::ClockTime::from_seconds(8));
        state?;
        let mut streams = crate::playback::audio_streams::Streams::default();
        while let Some(message) = pipeline.0.bus().unwrap().pop() {
            streams.observe(&pipeline.0, &message)?;
        }
        seek(&pipeline.0, 2000)?;
        wait_frame(&mut session, 2000, true)?;
        seek(&pipeline.0, 5000)?;
        wait_frame(&mut session, 5000, false)?;
        seek(&pipeline.0, 8000)?;
        wait_frame(&mut session, 8000, true)?;
        seek(&pipeline.0, 2000)?;
        wait_frame(&mut session, 2000, true)?;
        if name.ends_with(".mkv") {
            let tracks = streams.text_tracks();
            assert_eq!(tracks.len(), 2);
            let styled_id = &tracks[1].id;
            let audio_id = streams
                .tracks()
                .into_iter()
                .find(|t| t.selected)
                .unwrap()
                .id;
            streams.select_text(&pipeline.0, styled_id)?;
            pipeline.0.set_state(gst::State::Playing)?;
            let deadline = Instant::now() + DEADLINE;
            while !streams
                .text_tracks()
                .iter()
                .any(|t| t.id == *styled_id && t.selected)
            {
                assert!(
                    Instant::now() < deadline,
                    "subtitle selection was not confirmed"
                );
                if let Some(message) = pipeline
                    .0
                    .bus()
                    .unwrap()
                    .timed_pop(gst::ClockTime::from_mseconds(10))
                {
                    streams.observe(&pipeline.0, &message)?;
                }
            }
            assert!(
                streams
                    .tracks()
                    .iter()
                    .any(|t| t.id == audio_id && t.selected)
            );
            pipeline.0.set_state(gst::State::Paused)?;
            pipeline.0.state(gst::ClockTime::from_seconds(8)).0?;
            seek(&pipeline.0, 8000)?;
            let frame = wait_frame(&mut session, 8000, true)?;
            assert!(
                frame
                    .pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|p| p[1] > 150 && p[0] < 20 && p[2] < 20),
                "ASS track retains its green style"
            );
        }
        pipeline.0.set_state(gst::State::Ready)?;
    }
    Ok(())
}
#[test]
fn external_srt_ass_render_style_seek_and_reject_invalid_replacement() -> TestResult {
    gst::init()?;
    let player = gst::ElementFactory::make("playbin3").build()?;
    let mut session = Session::start(&player, None)?;
    for name in ["media-subtitles.srt", "media-subtitles.ass"] {
        session.load(fixture(name))?;
        let deadline = Instant::now() + DEADLINE;
        loop {
            if let Some(result) = session.poll_load() {
                result?;
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        wait_frame(&mut session, 2000, true)?;
        wait_frame(&mut session, 5000, false)?;
        wait_frame(&mut session, 8000, true)?;
        wait_frame(&mut session, 2000, true)?;
        assert!(
            session
                .render(gst::ClockTime::from_seconds(2), 640, 384)?
                .is_none(),
            "paused/static subtitles do not allocate another image"
        );
        let script = session.external().unwrap();
        let mut replay = Session::start(&player, Some(script))?;
        wait_frame(&mut replay, 2000, true)?;
    }
    let before = session.external();
    session.load(fixture("missing.srt"))?;
    while session.loading() {
        if let Some(result) = session.poll_load() {
            assert!(result.is_err());
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(session.external(), before);
    Ok(())
}
#[test]
fn subtitle_inputs_are_bounded_and_plain_text_cannot_inject_ass_commands() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("bad.srt");
    for text in [
        "",
        "not a subtitle",
        "1\n00:00:01,000 --> 00:00:00,000\ninvalid",
    ] {
        std::fs::write(&path, text)?;
        assert!(Script::load(&path).is_err());
    }
    std::fs::write(&path, [0xff, 0xfe])?;
    assert!(matches!(Script::load(&path), Err(Error::Encoding)));
    std::fs::write(&path, vec![b'x'; script::MAX_BYTES + 1])?;
    assert!(matches!(Script::load(&path), Err(Error::Capacity)));
    std::fs::write(
        &path,
        "\u{feff}1\r\n00:00:01,000 --> 00:00:02,000\r\none\r\n\r\n\r\n2\r\n00:00:03,000 --> 00:00:04,000\r\ntwo\r\n",
    )?;
    assert!(Script::load(&path).is_ok());
    assert!(!script::plain_text(r"{\pos(0,0)}").starts_with(r"{\pos"));
    Ok(())
}
