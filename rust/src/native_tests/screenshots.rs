//! Exercise the production capture path, including native pixels and Qt overlays.
use super::{
    bridge::ffi,
    startup::{TestResult, evaluate, wait_for},
};
use cxx_qt_lib::{QGuiApplication, QImage, QQmlApplicationEngine, QString};
use std::{
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

const SETTLE: Duration = Duration::from_millis(300);
const SAMPLE_PERIOD: Duration = Duration::from_secs(3);
const BURST_INTERVAL: Duration = Duration::from_millis(100);
const SOURCE_WIDTH: i32 = 1920;
const SOURCE_HEIGHT: i32 = 1080;

fn pump(app: &QGuiApplication, duration: Duration) {
    let until = Instant::now() + duration;
    while Instant::now() < until {
        app.process_events();
        thread::sleep(Duration::from_millis(2));
    }
}
fn json(
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
    expression: &str,
) -> TestResult<serde_json::Value> {
    let text = ffi::evaluate_root(
        engine.pin_mut(),
        &QString::from(format!("JSON.stringify({expression})")),
    )?
    .value::<QString>()
    .ok_or("missing JSON result")?
    .to_string();
    Ok(serde_json::from_str(&text)?)
}
fn file_url(path: &Path) -> TestResult<String> {
    Ok(serde_json::to_string(
        url::Url::from_file_path(path)
            .map_err(|_| "file URL")?
            .as_str(),
    )?)
}
fn saved_files(directory: &Path) -> TestResult<Vec<PathBuf>> {
    let mut files: Vec<_> = std::fs::read_dir(directory)?
        .filter_map(Result::ok)
        .map(|file| file.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "png"))
        .collect();
    files.sort();
    Ok(files)
}
fn capture(
    app: &QGuiApplication,
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
) -> TestResult<QImage> {
    assert!(evaluate(
        engine,
        "screenshot.canCapture && player.capture_screenshot() && screenshot.canCapture"
    )?);
    wait_for(
        app,
        engine,
        "!screenshot.busy && player.screenshot_error.length === 0",
    )?;
    let file = json(engine, "screenshotNotice.savedFile.toString()")?;
    let path = url::Url::parse(file.as_str().ok_or("saved URL")?)?
        .to_file_path()
        .map_err(|_| "saved path")?;
    QImage::from_data(&std::fs::read(path)?, Some("png"))
        .ok_or_else(|| "saved image could not be decoded".into())
}
fn save(image: &QImage, path: &Path) -> TestResult {
    if !crate::player::ffi::save_screenshot_image(
        image,
        &QString::from(path.to_string_lossy().as_ref()),
        &QString::from("png"),
        -1,
        60,
    ) {
        return Err("could not save review image".into());
    }
    Ok(())
}
fn generate(path: &Path, width: i32, height: i32, par: &str) -> TestResult {
    // CPU-only synthetic input. Frame numbers make an accidental next/decode-
    // ahead frame visible; neither a tuner nor external content is required.
    let status = std::process::Command::new("gst-launch-1.0")
        .args([
            "-q",
            "videotestsrc",
            "num-buffers=750",
            "pattern=ball",
            "!",
            &format!(
                "video/x-raw,width={width},height={height},framerate=25/1,pixel-aspect-ratio={par}"
            ),
            "!",
            "timeoverlay",
            "time-mode=buffer-count",
            "font-desc=Sans 48",
            "!",
            "avenc_mpeg2video",
            "!",
            "mpegvideoparse",
            "!",
            "mpegtsmux",
            "!",
            "filesink",
            &format!("location={}", path.display()),
        ])
        .status()?;
    if !status.success() {
        return Err(format!("synthetic video generation failed: {status}").into());
    }
    Ok(())
}
fn colored(image: &QImage, channel: usize) -> (usize, i32, i32, i32, i32) {
    let mut bounds = (0, image.width(), image.height(), 0, 0);
    for y in 0..image.height() {
        for x in 0..image.width() {
            let color = image.pixel_color(x, y);
            let rgb = [color.red(), color.green(), color.blue()];
            if rgb[channel] > 140 && rgb[(channel + 1) % 3] < 100 && rgb[(channel + 2) % 3] < 100 {
                bounds.0 += 1;
                bounds.1 = bounds.1.min(x);
                bounds.2 = bounds.2.min(y);
                bounds.3 = bounds.3.max(x);
                bounds.4 = bounds.4.max(y);
            }
        }
    }
    bounds
}
// Linux-only process observations, deliberately confined to this test runner.
fn resources() -> serde_json::Value {
    #[cfg(target_os = "linux")]
    {
        let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
        let rss = status
            .lines()
            .find(|line| line.starts_with("VmRSS:"))
            .unwrap_or("");
        let stat = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
        let fields: Vec<_> = stat
            .rsplit_once(')')
            .map(|(_, rest)| rest.split_whitespace().collect())
            .unwrap_or_default();
        // Fields after the command begin at field 3; utime/stime are 14/15.
        let ticks = fields
            .get(11)
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0)
            + fields
                .get(12)
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0);
        serde_json::json!({"rss":rss,"cpu_ticks":ticks})
    }
    #[cfg(not(target_os = "linux"))]
    {
        serde_json::Value::Null
    }
}

pub(super) fn run(
    app: &QGuiApplication,
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
) -> TestResult {
    let temporary = tempfile::tempdir()?;
    let review = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../build/screenshot-review");
    std::fs::create_dir_all(&review)?;
    let video = temporary.path().join("numbered.ts");
    generate(&video, SOURCE_WIDTH, SOURCE_HEIGHT, "1/1")?;
    let images = temporary.path().join("images");
    assert!(evaluate(
        engine,
        &format!(
            "player.configure_screenshot_directory({}); player.open_recording({}); true",
            file_url(&images)?,
            file_url(&video)?
        )
    )?);
    wait_for(
        app,
        engine,
        "player.playing && JSON.parse(player.video_stats()).rendered > 5",
    )?;
    assert!(evaluate(engine, "player.pause()")?);
    wait_for(app, engine, "player.paused")?;
    evaluate(
        engine,
        "root.showProgram = false; root.width = 960; root.height = 600; overlayVisibility.controlsVisible = false; bottomPanel.visible = false; setup.close(); screenshotNotice.visible = false; captions.active = false; danmaku.active = false; true",
    )?;
    pump(app, SETTLE);
    let plain = capture(app, engine)?;
    assert_eq!(
        (plain.width(), plain.height()),
        (SOURCE_WIDTH, SOURCE_HEIGHT)
    );
    save(&plain, &review.join("native.png"))?;
    let window = ffi::grabRoot(engine.pin_mut())?;
    save(&window, &review.join("window.png"))?;
    // Compare native pixels with the actual displayed, numbered frame. The
    // window includes letterboxing, scaling and Qt's texture filtering.
    let geometry = json(
        engine,
        "(function(){const p=video.mapToItem(null,0,0); return {x:p.x,y:p.y,w:video.width,h:video.height,dpr:root.devicePixelRatio};})()",
    )?;
    let ratio = window.width() as f64 / json(engine, "root.width")?.as_f64().ok_or("root width")?;
    let vw = geometry["w"].as_f64().ok_or("video width")?;
    let vh = geometry["h"].as_f64().ok_or("video height")?;
    let scale = (vw / f64::from(SOURCE_WIDTH)).min(vh / f64::from(SOURCE_HEIGHT));
    let left = geometry["x"].as_f64().unwrap() + (vw - f64::from(SOURCE_WIDTH) * scale) / 2.0;
    let top = geometry["y"].as_f64().unwrap() + (vh - f64::from(SOURCE_HEIGHT) * scale) / 2.0;
    let mut difference = 0_u64;
    let mut samples = 0_u64;
    for y in (20..240).step_by(2) {
        for x in (20..500).step_by(2) {
            let actual = window.pixel_color(
                ((left + (f64::from(x) + 0.5) * scale) * ratio) as i32,
                ((top + (f64::from(y) + 0.5) * scale) * ratio) as i32,
            );
            let original = plain.pixel_color(x, y);
            difference += (actual.red() - original.red()).unsigned_abs() as u64;
            samples += 1;
        }
    }
    let difference = difference as f64 / samples as f64;
    assert!(
        difference < 8.0,
        "display/native pixel difference: {difference}"
    );

    evaluate(
        engine,
        "captions.active = true; danmaku.active = true; true",
    )?;
    wait_for(
        app,
        engine,
        "captions.item !== null && danmaku.item !== null",
    )?;
    let cue = serde_json::json!({"planeWidth":1920,"planeHeight":1080,"text":"字幕", "cells":[{
        "text":"字幕", "x":640,"y":760,"width":240,"height":160,"glyphWidth":240,"glyphHeight":120,
        "foreground":"#00ff00","background":"transparent","stroke":"#000000","stroked":true,
        "bold":true,"italic":false,"underline":true
    }]});
    evaluate(
        engine,
        &format!(
            "captions.item.captionJson = {}; danmaku.item.replayReady=false; danmaku.item.playbackClock=null; danmaku.item.controller.reset(); danmaku.item.paused=false; player.configure_danmaku(true,48,1,1); danmaku.item.receive('COMMENT', 'top', 16711680) && danmaku.item.receive('FLOW', 'right', 255)",
            serde_json::to_string(&cue.to_string())?
        ),
    )?;
    pump(app, SETTLE);
    evaluate(engine, "danmaku.item.paused=true; true")?;
    pump(app, SETTLE);
    let overlaid = capture(app, engine)?;
    save(&overlaid, &review.join("overlays.png"))?;
    save(
        &ffi::grabRoot(engine.pin_mut())?,
        &review.join("overlays-window.png"),
    )?;

    let green = colored(&overlaid, 1);
    let red = colored(&overlaid, 0);
    assert!(
        green.0 > 100 && red.0 > 100,
        "missing caption/comment: {green:?} / {red:?}"
    );
    assert!(
        colored(&overlaid, 2).0 > 100,
        "scrolling comment was not captured"
    );
    evaluate(engine, "captions.item.visible=false; true")?;
    pump(app, SETTLE);
    let comments_only = capture(app, engine)?;
    assert!(colored(&comments_only, 1).0 < 10 && colored(&comments_only, 0).0 > 100);
    evaluate(engine, "captions.item.visible=true; true")?;
    pump(app, SETTLE);
    // Accept several identical frames, then immediately hide overlays and
    // resize. Workers must retain the accepted frame and text positions.
    let before = saved_files(&images)?.len();
    assert!(evaluate(
        engine,
        "player.capture_screenshot() && player.capture_screenshot() && player.capture_screenshot()"
    )?);
    evaluate(
        engine,
        "captions.item.visible=false; danmaku.item.visible=false; root.width=1280; root.height=720; true",
    )?;
    wait_for(
        app,
        engine,
        "!screenshot.busy && player.screenshot_error.length===0",
    )?;
    let files = saved_files(&images)?;
    assert_eq!(files.len(), before + 3);
    for path in &files[before..] {
        let image = QImage::from_data(&std::fs::read(path)?, Some("png")).ok_or("burst image")?;
        assert_eq!(
            image, overlaid,
            "accepted overlay/image changed while queued"
        );
    }
    pump(app, SETTLE);
    let hidden = capture(app, engine)?;
    assert_eq!(
        hidden, plain,
        "hidden overlays or player controls in saved image"
    );
    evaluate(
        engine,
        "captions.item.visible=true; danmaku.item.visible=true; danmaku.item.paused=false; danmaku.item.receive('COMMENT', 'top', 16711680); danmaku.item.paused=true; true",
    )?;
    pump(app, SETTLE);
    let resized = capture(app, engine)?;
    let resized_green = colored(&resized, 1);
    let displayed = ffi::grabRoot(engine.pin_mut())?;
    let displayed_green = colored(&displayed, 1);
    let logical_width = json(engine, "root.width")?.as_f64().unwrap();
    let factor = f64::from(SOURCE_WIDTH) / f64::from(displayed.width());
    assert!(
        (f64::from(resized_green.1) - f64::from(displayed_green.1) * factor).abs() < 5.0,
        "resized caption no longer matches displayed position at width {logical_width}"
    );
    assert!(
        colored(&resized, 0).0 > 100 && colored(&resized, 0).0 < red.0,
        "fixed UI comment font did not scale down with a larger viewport"
    );
    evaluate(engine, "root.showFullScreen(); true")?;
    pump(app, SETTLE);
    // Entering fullscreen reveals controls. Hide the UI gradient before
    // comparing glyph pixels; screenshots deliberately omit that gradient.
    evaluate(engine, "overlayVisibility.controlsVisible=false; true")?;
    pump(app, SETTLE);
    let full = capture(app, engine)?;
    assert_eq!((full.width(), full.height()), (SOURCE_WIDTH, SOURCE_HEIGHT));
    let full_green = colored(&full, 1);
    let displayed_full = ffi::grabRoot(engine.pin_mut())?;
    save(&full, &review.join("fullscreen-native.png"))?;
    save(&displayed_full, &review.join("fullscreen-window.png"))?;
    let shown = colored(&displayed_full, 1);
    let factor = f64::from(SOURCE_WIDTH) / f64::from(displayed_full.width());
    // Compare the caption's left edge and underlined baseline. Qt Quick's
    // distance-field glyph ink differs slightly from QPainter's font outlines.
    assert!(
        (f64::from(full_green.1) - f64::from(shown.1) * factor).abs() < 5.0
            && (f64::from(full_green.4) - f64::from(shown.4) * factor).abs() < 5.0,
        "fullscreen caption does not match displayed placement: native {full_green:?}, shown {shown:?}, factor {factor}, geometry {}",
        json(
            engine,
            "({w:root.width,h:root.height,vw:video.width,vh:video.height})"
        )?
    );
    evaluate(
        engine,
        "root.showNormal(); captions.active=false; danmaku.active=false; player.play(); true",
    )?;
    wait_for(app, engine, "player.playing && !player.paused")?;

    let mut reports = Vec::new();
    for mode in ["none", "single", "burst"] {
        pump(app, SETTLE);
        let count_before = saved_files(&images)?.len();
        let observer = ffi::watchFrames(engine)?;
        let started = Instant::now();
        let mut next = Duration::ZERO;
        let mut accepted = 0;
        let mut acceptance = Vec::new();
        let mut single_save_ms = None;
        let before = resources();
        while started.elapsed() < SAMPLE_PERIOD {
            app.process_events();
            if mode != "none" && started.elapsed() >= next && (mode == "burst" || accepted == 0) {
                let request = Instant::now();
                assert!(evaluate(engine, "player.capture_screenshot()")?);
                acceptance.push(request.elapsed().as_secs_f64() * 1000.0);
                accepted += 1;
                next += BURST_INTERVAL;
                if mode == "single" {
                    wait_for(
                        app,
                        engine,
                        "!screenshot.busy && player.screenshot_error.length===0",
                    )?;
                    single_save_ms = Some(request.elapsed().as_secs_f64() * 1000.0);
                }
            }
            thread::sleep(Duration::from_millis(2));
        }
        let mut intervals: Vec<f64> = serde_json::from_str(&observer.samples().to_string())?;
        drop(observer);
        assert!(intervals.len() > 30, "Qt stopped presenting during {mode}");
        intervals.sort_by(f64::total_cmp);
        let max = *intervals.last().unwrap();
        assert!(
            max < 500.0,
            "visible playback stall during {mode}: {max} ms"
        );
        let measured = started.elapsed();
        wait_for(
            app,
            engine,
            "!screenshot.busy && player.screenshot_error.length===0",
        )?;
        let all = saved_files(&images)?;
        assert_eq!(all.len() - count_before, accepted);
        if mode == "burst" {
            let distinct: std::collections::HashSet<_> = all[count_before..]
                .iter()
                .map(std::fs::read)
                .collect::<Result<_, _>>()?;
            // Encoding has no time metadata. Repeated copies of a stalled
            // native frame must not pass just because Qt still swaps its UI.
            assert!(
                distinct.len() >= accepted * 4 / 5,
                "video stopped advancing during burst capture"
            );
        }
        let sizes: Vec<_> = all[count_before..]
            .iter()
            .map(|file| std::fs::metadata(file).map(|m| m.len()))
            .collect::<Result<_, _>>()?;
        reports.push(serde_json::json!({"mode":mode,"qt_frames":intervals.len(),"frame_p95_ms":intervals[intervals.len()*95/100],"frame_max_ms":max,"accept_ms":acceptance,"single_save_ms":single_save_ms,"period_ms":measured.as_millis(),"drain_ms":started.elapsed().saturating_sub(measured).as_millis(),"file_bytes":sizes,"before":before,"after":resources()}));
    }
    std::fs::write(
        review.join("metrics.json"),
        serde_json::to_string_pretty(
            &serde_json::json!({"display_difference":difference,"modes":reports}),
        )?,
    )?;
    // Non-square pixels and a 4:3 video must produce square-pixel 768x576.
    evaluate(engine, "player.stop(); true")?;
    let anamorphic = temporary.path().join("anamorphic.ts");
    generate(&anamorphic, 720, 576, "16/15")?;
    evaluate(
        engine,
        &format!("player.open_recording({}); true", file_url(&anamorphic)?),
    )?;
    wait_for(
        app,
        engine,
        "player.playing && JSON.parse(player.video_stats()).rendered>5",
    )?;
    let aspect = capture(app, engine)?;
    assert_eq!((aspect.width(), aspect.height()), (768, 576));
    save(&aspect, &review.join("aspect.png"))?;
    assert!(evaluate(engine, "player.pause()")?);
    pump(app, SETTLE);
    let before_seek = capture(app, engine)?;
    let count = saved_files(&images)?.len();
    assert!(evaluate(
        engine,
        "player.capture_screenshot() && player.seek_to(10000) && !screenshot.canCapture"
    )?);
    wait_for(
        app,
        engine,
        "!player.seeking && !screenshot.busy && player.screenshot_error.length===0",
    )?;
    let files = saved_files(&images)?;
    assert_eq!(files.len(), count + 1);
    let accepted = QImage::from_data(&std::fs::read(files.last().unwrap())?, Some("png"))
        .ok_or("accepted seek image")?;
    assert_eq!(
        accepted, before_seek,
        "a pending image was replaced by the seek result"
    );
    pump(app, SETTLE);
    assert_ne!(
        capture(app, engine)?,
        before_seek,
        "a stale frame survived the seek"
    );
    // An accepted native frame remains valid after stop and graph teardown.
    let count = saved_files(&images)?.len();
    assert!(evaluate(
        engine,
        "player.capture_screenshot() && (player.stop(), !player.capture_screenshot())"
    )?);
    wait_for(
        app,
        engine,
        "!screenshot.busy && player.screenshot_error.length===0",
    )?;
    assert_eq!(saved_files(&images)?.len(), count + 1);
    println!(
        "Native screenshots passed: displayed numbered frame, 1080p/4:3/PAR, overlays, resize/fullscreen, hidden state, immutable burst, stop, playback metrics; artifacts: {}",
        review.display()
    );
    Ok(())
}
