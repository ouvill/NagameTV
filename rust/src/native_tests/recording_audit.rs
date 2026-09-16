//! Bounded, hardware-dependent comparison of six-second recovery fixtures.
//! Reports observations, including failed seek targets; it is not a claim of
//! MPEG-TS conformance or a replacement for the assertion-based startup suite.
use super::{
    bridge::ffi,
    startup::{TestResult, evaluate, wait_for},
};
use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString};
use std::{
    path::Path,
    thread,
    time::{Duration, Instant},
};

const SAMPLE_INTERVAL: Duration = Duration::from_millis(100);
const PLAYBACK_TIMEOUT: Duration = Duration::from_secs(12);
const SEEK_TIMEOUT: Duration = Duration::from_secs(12);
const SEEK_TARGETS_MS: [u32; 3] = [4500, 1000, 4500];
const SEEK_TOLERANCE_MS: f64 = 500.0;

#[derive(serde::Deserialize)]
struct Observation {
    ended: bool,
    seeking: bool,
    paused: bool,
    position_ms: f64,
    duration_ms: f64,
    error: String,
    transport_error: String,
    rendered: Option<u64>,
}

fn observe(engine: &mut cxx::UniquePtr<QQmlApplicationEngine>) -> TestResult<Observation> {
    let json = ffi::evaluate_root(engine.pin_mut(), &QString::from(
        "JSON.stringify({ended: player.ended, seeking: player.seeking, paused: player.paused, position_ms: player.position_ms, duration_ms: player.duration_ms, estimated: player.duration_estimated, error: player.playback_error, transport_error: player.transport_error, rendered: JSON.parse(player.video_stats()).rendered, audio: JSON.parse(player.audio_tracks())})",
    ))?.value::<QString>().ok_or("missing recording observation")?.to_string();
    eprintln!("Recording observation: {json}");
    Ok(serde_json::from_str(&json)?)
}

pub(super) fn run(
    app: &QGuiApplication,
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
    path: &Path,
) -> TestResult {
    let uri = url::Url::from_file_path(path.canonicalize()?).map_err(|_| "fixture URL")?;
    let open = format!(
        "recordingInput.openUrl({})",
        serde_json::to_string(uri.as_str())?
    );
    eprintln!("Recording audit: playback {}", path.display());
    assert!(evaluate(engine, &open)?);
    wait_for(app, engine, "player.playing && player.seekable")?;
    let started = Instant::now();
    let mut previous_position = None;
    loop {
        app.process_events();
        let sample = observe(engine)?;
        if let Some(previous) = previous_position
            && sample.position_ms < previous
        {
            eprintln!(
                "Recording position decreased: {previous} -> {} ms",
                sample.position_ms
            );
        }
        previous_position = Some(sample.position_ms);
        if !sample.error.is_empty() || started.elapsed() >= PLAYBACK_TIMEOUT {
            return Err("recording audit did not reach normal EOF".into());
        }
        if sample.ended {
            eprintln!("Recording audit: normal EOF after {:?}", started.elapsed());
            break;
        }
        thread::sleep(SAMPLE_INTERVAL);
    }

    // Start again, then cross the original PID boundary in both directions
    // while paused. Resume afterwards to verify that fresh output progresses.
    eprintln!("Recording audit: seeks");
    evaluate(engine, "player.stop(); true")?;
    assert!(evaluate(engine, &open)?);
    wait_for(app, engine, "player.playing && player.seekable")?;
    assert!(evaluate(engine, "player.pause() && player.paused")?);
    let mut failed_targets = Vec::new();
    for target in SEEK_TARGETS_MS {
        let accepted = evaluate(engine, &format!("player.seek_to({target})"))?;
        eprintln!("Recording seek request: target={target} accepted={accepted}");
        let deadline = Instant::now() + SEEK_TIMEOUT;
        let completed = loop {
            app.process_events();
            let sample = observe(engine)?;
            if !sample.error.is_empty()
                || !sample.transport_error.is_empty()
                || Instant::now() >= deadline
            {
                break false;
            }
            if !sample.seeking {
                break accepted
                    && sample.paused
                    && (sample.position_ms - f64::from(target)).abs() <= SEEK_TOLERANCE_MS;
            }
            thread::sleep(SAMPLE_INTERVAL);
        };
        eprintln!("Recording seek result: target={target} reached={completed}");
        if !completed {
            failed_targets.push(target);
        }
    }
    evaluate(engine, "player.play(); true")?;
    wait_for(
        app,
        engine,
        "player.playing && !player.paused && !player.seeking",
    )?;
    wait_for(app, engine, "player.ended && !player.playing")?;
    let resumed = observe(engine)?;
    if !resumed.error.is_empty() || resumed.rendered.is_none_or(|count| count == 0) {
        return Err("no rendered output after resuming".into());
    }
    evaluate(engine, "player.stop(); true")?;
    if !failed_targets.is_empty() {
        return Err(format!("recording audit seek targets failed: {failed_targets:?}").into());
    }
    Ok(())
}

/// Real broadcast regression: cross the reported transition, then seek both ways.
pub(super) fn probe(
    app: &QGuiApplication,
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
    path: &Path,
) -> TestResult {
    const OBSERVE_FOR: Duration = Duration::from_secs(28);
    const STALL_LIMIT: Duration = Duration::from_secs(6);
    const PROBE_POLL: Duration = Duration::from_millis(500);
    const CROSSING_TARGETS_MS: [u32; 2] = [20_000, 10_000];
    const LONG_RECORDING: Duration = Duration::from_secs(60);
    const FAR_SEEK_FRACTION: f64 = 0.8;
    let uri = url::Url::from_file_path(path.canonicalize()?).map_err(|_| "recording URL")?;
    assert!(evaluate(
        engine,
        &format!(
            "recordingInput.openUrl({})",
            serde_json::to_string(uri.as_str())?
        )
    )?);
    if let Err(error) = wait_for(app, engine, "player.playing") {
        let state = ffi::evaluate_root(
            engine.pin_mut(),
            &QString::from(
                "JSON.stringify({playing: player.playing, loading: player.recording_loading, file_error: player.file_error, playback_error: player.playback_error, status: player.status})",
            ),
        )?;
        return Err(format!("{error}: {state:?}").into());
    }
    let started = Instant::now();
    let mut progressed = Instant::now();
    let mut frames = None;
    while started.elapsed() < OBSERVE_FOR {
        app.process_events();
        let sample = observe(engine)?;
        if !sample.error.is_empty() {
            return Err(sample.error.into());
        }
        if frames != sample.rendered {
            frames = sample.rendered;
            progressed = Instant::now();
        }
        if progressed.elapsed() > STALL_LIMIT {
            return Err("decoded output stopped at broadcast transition".into());
        }
        thread::sleep(PROBE_POLL);
    }
    assert!(evaluate(engine, "player.pause()")?);
    let duration = observe(engine)?.duration_ms;
    let mut targets = CROSSING_TARGETS_MS.to_vec();
    if duration > LONG_RECORDING.as_millis() as f64 {
        targets.push((duration * FAR_SEEK_FRACTION) as u32);
    }
    for target in targets {
        assert!(evaluate(engine, &format!("player.seek_to({target})"))?);
        wait_for(app, engine, "player.paused && !player.seeking")?;
        let sample = observe(engine)?;
        assert!((sample.position_ms - f64::from(target)).abs() <= SEEK_TOLERANCE_MS);
    }
    evaluate(engine, "player.stop(); true")?;
    eprintln!("Broadcast probe passed: continuous output and seeks across transition");
    Ok(())
}
