//! Hardware-free Player notifications and real portal wire operations on a private bus.
use super::{ffi, stream_state::State};
use crate::{
    features, qt, settings,
    sleep_inhibit::{Inhibitor, Progress, Request},
};
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QCoreApplication, QString};
use std::{
    fs,
    path::Path,
    process::{Child, Command},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;
struct Service(Child);
impl Drop for Service {
    fn drop(&mut self) {
        if let Err(error) = self.0.kill() {
            tracing::error!(%error, "Stop fake inhibit portal failed");
        }
        if let Err(error) = self.0.wait() {
            tracing::error!(%error, "Wait for fake inhibit portal failed");
        }
    }
}
fn wait(app: &QCoreApplication, mut ready: impl FnMut() -> bool) -> Result {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        app.process_events();
        if ready() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err("sleep inhibition check timed out".into());
        }
        thread::sleep(Duration::from_millis(2));
    }
}
fn events(path: &Path) -> Vec<serde_json::Value> {
    fs::read_to_string(path.join("events.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn checks(no_bus: bool) -> Result {
    features::PLAN
        .set(features::LaunchPlan::Restricted("none".parse()?))
        .map_err(|_| "plan initialized twice")?;
    let app = QCoreApplication::new();
    assert!(!app.is_null());
    let mut backend = qt::sleep_inhibit::Portal;
    if no_bus {
        let mut request = backend.acquire()?;
        let error = match request.poll() {
            Err(error) => error,
            Ok(_) => return Err("missing bus was accepted".into()),
        };
        assert!(error.unavailable);
        return Ok(());
    }
    let status = Command::new(std::env::current_exe()?)
        .args(["--native-tests", "sleep-inhibit-no-bus"])
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            "unix:path=/nonexistent/nagametv-test-bus",
        )
        .status()?;
    assert!(status.success());
    // No service exists yet. A missing portal must not look like a held grant.
    let mut missing = backend.acquire()?;
    wait(&app, || match missing.poll() {
        Err(error) => {
            assert!(error.unavailable);
            true
        }
        Ok(_) => false,
    })?;
    drop(missing);
    let directory = tempfile::tempdir()?;
    let path = directory.path();
    fs::write(path.join("mode"), "success")?;
    let service = Service(
        Command::new("python3")
            .args(["-m", "scripts.testing.sleep_inhibit_service"])
            .env("NAGAMETV_INHIBIT_FIXTURE", path)
            .spawn()?,
    );
    wait(&app, || path.join("ready").exists())?;
    let mut player = ffi::new_player();
    player.pin_mut().configure_sleep_inhibit(false);
    assert!(matches!(
        player.pin_mut().rust_mut().preferences.flush()?,
        settings::SaveStatus::Transient
    ));
    player.pin_mut().configure_sleep_inhibit(true);
    player.pin_mut().rust_mut().preferences =
        settings::Loaded::open(path.join("settings.toml"))?.activate(None, None);
    assert!(player.sleep_inhibit_enabled());
    assert_eq!(player.sleep_inhibit_state(), QString::from("idle"));
    let notifications = Arc::new(Mutex::new(Vec::new()));
    let observed = notifications.clone();
    let _connection = player.pin_mut().on_sleep_inhibit_changed(move |p| {
        let enabled = p.sleep_inhibit_enabled();
        let state = p.sleep_inhibit_state().to_string();
        let error = p.sleep_inhibit_error().to_string();
        assert_eq!(
            enabled,
            p.rust().preferences.preferences().sleep_inhibit_enabled
        );
        assert_eq!(state, p.rust().sleep_inhibit.status().as_str());
        assert_eq!(
            error.is_empty(),
            !matches!(state.as_str(), "failed" | "unavailable")
        );
        observed.lock().unwrap().push((enabled, state));
    });
    // Simulate an output-confirmed live session without constructing native playback.
    let channels = crate::channels::parse(br#"[{"id":1,"name":"TV","type":1}]"#)?;
    let live = || {
        State::Connecting(super::stream_state::Attempt::new(
            &channels[0],
            crate::playback::input::Retention::Off.into(),
        ))
        .started()
    };
    player.pin_mut().rust_mut().stream_state = live();
    player.pin_mut().poll_sleep_inhibit();
    assert_eq!(player.sleep_inhibit_state(), QString::from("acquiring"));
    wait(&app, || {
        player.pin_mut().poll_sleep_inhibit();
        player.sleep_inhibit_state() == QString::from("inhibited")
    })?;
    assert_eq!(events(path)[0]["flags"], 12);
    assert_eq!(events(path)[0]["window"], "");
    player.pin_mut().configure_sleep_inhibit(false);
    assert_eq!(player.sleep_inhibit_state(), QString::from("disabled"));
    assert!(
        !settings::Loaded::open(path.join("settings.toml"))?
            .preferences()
            .sleep_inhibit_enabled
    );
    wait(&app, || events(path).iter().any(|e| e["kind"] == "close"))?;
    player.pin_mut().configure_sleep_inhibit(true);
    wait(&app, || {
        player.pin_mut().poll_sleep_inhibit();
        player.sleep_inhibit_state() == QString::from("inhibited")
    })?;
    player.pin_mut().rust_mut().stream_state = State::default();
    player.pin_mut().poll_sleep_inhibit();
    assert_eq!(player.sleep_inhibit_state(), QString::from("idle"));
    for mode in [
        "failed",
        "unavailable",
        "malformed",
        "response-failed",
        "no-response",
    ] {
        fs::write(path.join("mode"), mode)?;
        player.pin_mut().rust_mut().stream_state = live();
        wait(&app, || {
            player.pin_mut().poll_sleep_inhibit();
            !player.sleep_inhibit_error().is_empty()
        })?;
        assert_eq!(
            player.sleep_inhibit_state().to_string(),
            if mode == "unavailable" {
                "unavailable"
            } else {
                "failed"
            }
        );
        assert!(player.pin_mut().request_language(QString::from("ja")));
        assert!(player.sleep_inhibit_error().to_string().contains("画面"));
        assert!(player.pin_mut().request_language(QString::from("en")));
        player.pin_mut().rust_mut().stream_state = State::default();
        player.pin_mut().poll_sleep_inhibit();
    }
    fs::write(path.join("mode"), "delayed")?;
    let count = events(path)
        .iter()
        .filter(|e| e["kind"] == "acquire")
        .count();
    let mut delayed = backend.acquire()?;
    wait(&app, || {
        events(path)
            .iter()
            .filter(|e| e["kind"] == "acquire")
            .count()
            > count
    })?;
    assert!(matches!(delayed.poll()?, Progress::Pending));
    drop(delayed);
    wait(&app, || {
        events(path).iter().any(|e| e["kind"] == "cancelled")
    })?;
    fs::write(path.join("mode"), "success")?;
    player.pin_mut().rust_mut().stream_state = live();
    wait(&app, || {
        player.pin_mut().poll_sleep_inhibit();
        player.sleep_inhibit_state() == QString::from("inhibited")
    })?;
    drop(service);
    wait(&app, || {
        player.pin_mut().poll_sleep_inhibit();
        player.sleep_inhibit_state() == QString::from("failed")
    })?;
    assert!(!notifications.lock().unwrap().is_empty());
    drop(player);
    Ok(())
}
pub fn run(no_bus: bool) -> i32 {
    match checks(no_bus) {
        Ok(()) => {
            println!("Sleep inhibition checks passed");
            0
        }
        Err(error) => {
            tracing::error!(error = error.as_ref(), "Sleep inhibition checks failed");
            1
        }
    }
}
