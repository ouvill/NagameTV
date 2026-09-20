//! Product-window verification of negotiated memory, field rate and captures.
//! Run only through test-startup.sh, which validates the isolated GPU session.
use super::{
    screenshots::{capture, file_url, json},
    startup::{TestResult, evaluate, wait_for},
};
use crate::playback::deinterlace::Mode;
use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine};
use std::path::Path;

const WIDTH: i32 = 1920;
const HEIGHT: i32 = 1080;
const INPUT_FPS: f64 = 25.0;
const CAPTURE_SAMPLE_STEP: usize = 8;
const MIN_CAPTURE_CONTRAST: i32 = 128;

fn generate(path: &Path, interlaced: bool) -> TestResult {
    let mut args = vec![
        "-q".to_owned(),
        "videotestsrc".into(),
        "num-buffers=400".into(),
        "pattern=ball".into(),
        "!".into(),
        format!(
            "video/x-raw,format=I420,width={WIDTH},height={HEIGHT},framerate={}/1",
            if interlaced { 50 } else { 25 }
        ),
    ];
    if interlaced {
        args.extend(
            [
                "!",
                "interlace",
                "field-pattern=1:1",
                "top-field-first=true",
            ]
            .map(str::to_owned),
        );
    }
    args.extend(["!", "avenc_mpeg2video"].map(str::to_owned));
    if interlaced {
        args.push("flags=ildct+ilme".into());
    }
    args.extend(["!", "mpegvideoparse", "!", "mpegtsmux", "!", "filesink"].map(str::to_owned));
    args.push(format!("location={}", path.display()));
    // A CPU encoder produces deterministic fixtures, never an emulated GPU.
    let status = std::process::Command::new("gst-launch-1.0")
        .args(args)
        .status()?;
    if !status.success() {
        return Err(format!("video fixture generation failed: {status}").into());
    }
    Ok(())
}

pub(super) fn run(
    app: &QGuiApplication,
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
) -> TestResult {
    let mode = Mode::from_environment()?;
    let temporary = tempfile::tempdir()?;
    evaluate(
        engine,
        &format!(
            "player.configure_screenshot_directory({}); true",
            file_url(&temporary.path().join("images"))?
        ),
    )?;
    // Reuse the same player to exercise context/pool lifetime with unchanged
    // caps as well as progressive bypass and returning to interlaced video.
    let interlaced = temporary.path().join("interlaced.ts");
    let progressive = temporary.path().join("progressive.ts");
    generate(&interlaced, true)?;
    generate(&progressive, false)?;
    for (path, is_interlaced) in [
        (&interlaced, true),
        (&interlaced, true),
        (&progressive, false),
        (&progressive, false),
        (&interlaced, true),
    ] {
        evaluate(
            engine,
            &format!("player.open_recording({}); true", file_url(path)?),
        )?;
        if let Err(error) = wait_for(
            app,
            engine,
            "player.playing && JSON.parse(player.video_stats()).rendered > 20",
        ) {
            return Err(format!(
                "{error}; {}",
                json(
                    engine,
                    "({error: player.playback_error, stats: JSON.parse(player.video_stats())})"
                )?
            )
            .into());
        }
        let stats = json(engine, "JSON.parse(player.video_stats())")?;
        eprintln!("Video processing ({mode:?}, interlaced={is_interlaced}): {stats}");
        assert_eq!(stats["output"]["memory"], "memory:GLMemory");
        assert_eq!(stats["output"]["width"], WIDTH);
        assert_eq!(stats["output"]["height"], HEIGHT);
        let rate = if is_interlaced && matches!(mode, Mode::Yadif | Mode::Linear | Mode::VaApi) {
            INPUT_FPS * 2.0
        } else {
            INPUT_FPS
        };
        assert_eq!(stats["output"]["fps"], rate);
        if is_interlaced {
            // avdec advertises mixed caps and marks interlaced buffers; NVDEC
            // may declare the entire sequence interleaved instead.
            assert!(matches!(
                stats["input"]["interlace"].as_str(),
                Some("interleaved" | "mixed")
            ));
        }
        match mode {
            Mode::NvidiaGl => {
                assert_eq!(stats["processor_passthrough"], !is_interlaced);
                let format = if std::env::var("NAGAMETV_VIDEO_FORMAT").as_deref() == Ok("nv12") {
                    "NV12"
                } else {
                    "RGBA"
                };
                assert_eq!(stats["output"]["pixel_format"], format);
                assert_eq!(stats["input"]["memory"], "memory:GLMemory");
                assert_eq!(stats["decoders"], serde_json::json!(["nvmpeg2videodec"]));
            }
            Mode::VaApi => {
                assert_eq!(stats["processor_passthrough"], !is_interlaced);
                assert_eq!(stats["output"]["interlace"], "progressive");
                assert_eq!(stats["input"]["memory"], "memory:VAMemory");
                assert_eq!(stats["decoders"], serde_json::json!(["vampeg2dec"]));
            }
            Mode::Yadif | Mode::Linear | Mode::Off => {}
        }
        match std::env::var("NAGAMETV_VIDEO_FORMAT").as_deref() {
            Ok("nv12") => assert_eq!(stats["output"]["pixel_format"], "NV12"),
            Ok("rgba") => assert_eq!(stats["output"]["pixel_format"], "RGBA"),
            _ => {}
        }
        assert!(evaluate(engine, "player.pause()")?);
        wait_for(app, engine, "player.paused")?;
        let image = capture(app, engine)?;
        assert_eq!((image.width(), image.height()), (WIDTH, HEIGHT));
        // A valid size alone also accepts a blank GPU readback. The fixture
        // contains a white ball on black; sample more finely than its diameter.
        let mut darkest = i32::MAX;
        let mut brightest = i32::MIN;
        for y in (0..HEIGHT).step_by(CAPTURE_SAMPLE_STEP) {
            for x in (0..WIDTH).step_by(CAPTURE_SAMPLE_STEP) {
                let value = image.pixel_color(x, y).red();
                darkest = darkest.min(value);
                brightest = brightest.max(value);
            }
        }
        assert!(
            brightest - darkest >= MIN_CAPTURE_CONTRAST,
            "blank captured frame"
        );
        evaluate(engine, "player.stop(); true")?;
    }
    println!(
        "Video processing passed: interlaced/progressive/restart, negotiated memory, decoder, frame rate, screenshot"
    );
    Ok(())
}
