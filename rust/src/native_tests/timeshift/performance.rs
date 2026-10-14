//! Optional measurements on the same production window and paced local fixture.
use super::{Server, Traffic};
use crate::native_tests::{
    screenshots::json,
    startup::{TestResult, evaluate, wait_for},
};
use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine};
use serde_json::{Value, json};
use std::{
    thread,
    time::{Duration, Instant},
};

const REPETITIONS: usize = 7;
const POLLS_PER_BATCH: usize = 100;
const OBSERVE_INTERVAL: Duration = Duration::from_millis(5);
const FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(20);
const SETTLE: Duration = Duration::from_millis(500);

fn pump(app: &QGuiApplication, duration: Duration) {
    let started = Instant::now();
    while started.elapsed() < duration {
        app.process_events();
        thread::sleep(OBSERVE_INTERVAL);
    }
}

fn summary(samples: &[f64]) -> Value {
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    json!({
        "samples": samples,
        "median": sorted[sorted.len() / 2],
        "min": sorted[0],
        "max": sorted[sorted.len() - 1],
    })
}

fn poll_batches(
    app: &QGuiApplication,
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
) -> TestResult<Value> {
    let source = format!("for (let i = 0; i < {POLLS_PER_BATCH}; i++) player.poll(); true");
    let mut samples = Vec::new();
    for _ in 0..REPETITIONS {
        let started = Instant::now();
        assert!(evaluate(engine, &source)?);
        samples.push(started.elapsed().as_secs_f64() * 1_000_000.0 / POLLS_PER_BATCH as f64);
        pump(app, SETTLE);
    }
    Ok(summary(&samples))
}

pub(super) fn run(
    app: &QGuiApplication,
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
) -> TestResult {
    evaluate(engine, "player.stop(); true")?;
    let idle_poll = poll_batches(app, engine)?;
    let server = Server::new(Traffic::Broadcast)?;
    assert!(evaluate(
        engine,
        &format!(
            "player.connect_server({})",
            serde_json::to_string(&server.url)?
        )
    )?);
    wait_for(
        app,
        engine,
        "player.server_configured && !player.loading && player.channels.count === 2",
    )?;
    evaluate(
        engine,
        "setup.close(); player.select(0); player.play(); true",
    )?;
    wait_for(
        app,
        engine,
        "player.playing && JSON.parse(player.video_stats()).rendered > 0",
    )?;
    pump(app, SETTLE);
    let mut request = Vec::new();
    let mut playing = Vec::new();
    let mut frame = Vec::new();
    let mut presented = Vec::new();
    for trial in 0..REPETITIONS {
        let selected = (trial + 1) % 2;
        let started = Instant::now();
        evaluate(
            engine,
            &format!("player.select({selected}); player.play(); true"),
        )?;
        request.push(started.elapsed().as_secs_f64() * 1000.0);
        let mut playing_ms = None;
        let mut frame_ms = None;
        let mut presented_ms = None;
        loop {
            app.process_events();
            let state = json(
                engine,
                "({playing: player.playing, selected: player.selected, error: player.playback_error, stats: JSON.parse(player.video_stats())})",
            )?;
            if state["error"].as_str().is_some_and(|text| !text.is_empty()) {
                return Err(format!("Channel switch failed: {state}").into());
            }
            if state["selected"] != selected {
                return Err("channel selection changed during measurement".into());
            }
            if state["playing"] == true {
                playing_ms.get_or_insert_with(|| started.elapsed().as_secs_f64() * 1000.0);
            }
            if state["stats"]["rendered"]
                .as_u64()
                .is_some_and(|count| count > 0)
            {
                frame_ms.get_or_insert_with(|| started.elapsed().as_secs_f64() * 1000.0);
            }
            // GstBaseSink's rendered counter excludes preroll. The receive
            // latency sample is committed only after Qt swaps the mapped video
            // frame, and its tracker belongs to this newly selected stream.
            if state["stats"]["receive_latency"]["status"] == "measuring" {
                presented_ms.get_or_insert_with(|| started.elapsed().as_secs_f64() * 1000.0);
            }
            if playing_ms.is_some() && frame_ms.is_some() && presented_ms.is_some() {
                break;
            }
            if started.elapsed() >= FIRST_FRAME_TIMEOUT {
                return Err(format!("No first video frame: {state}").into());
            }
            thread::sleep(OBSERVE_INTERVAL);
        }
        playing.push(playing_ms.ok_or("first frame without PLAYING observation")?);
        frame.push(frame_ms.ok_or("missing first-frame observation")?);
        presented.push(presented_ms.ok_or("missing Qt video presentation observation")?);
        pump(app, SETTLE);
    }
    evaluate(
        engine,
        "overlayVisibility.pinned = true; overlayVisibility.reveal(); true",
    )?;
    wait_for(app, engine, "playerControls.visible")?;
    pump(app, SETTLE);
    let visible_poll = poll_batches(app, engine)?;
    evaluate(
        engine,
        "overlayVisibility.pinned = false; overlayVisibility.dismiss(); true",
    )?;
    wait_for(app, engine, "!playerControls.visible")?;
    pump(app, SETTLE);
    let hidden_poll = poll_batches(app, engine)?;
    let stats = json(engine, "JSON.parse(player.video_stats())")?;
    eprintln!(
        "PLAYBACK_PERF {}",
        json!({
            "trials": REPETITIONS,
            "observe_interval_ms": OBSERVE_INTERVAL.as_millis(),
            "request_ms": summary(&request),
            "playing_ms": summary(&playing),
            "first_video_ms": summary(&frame),
            "first_presented_video_ms": summary(&presented),
            "idle_poll_us": idle_poll,
            "live_controls_visible_poll_us": visible_poll,
            "live_controls_hidden_poll_us": hidden_poll,
            "final_video_stats": stats,
        })
    );
    evaluate(engine, "player.stop(); true")?;
    Ok(())
}
