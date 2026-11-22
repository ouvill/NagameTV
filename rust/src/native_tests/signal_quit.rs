//! Exercise signal delivery through the production Qt event loop.
use super::startup::TestResult;
use crate::{settings, startup_test_server::Server};
use std::{
    fs::File,
    io,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const PROCESS_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(10);

fn signal_process(child: &Child, signal: i32) -> io::Result<()> {
    // SAFETY: The ID belongs to our unreaped child, so it cannot be reused.
    if unsafe { libc::kill(child.id() as libc::pid_t, signal) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

fn wait_for_app_exit(child: &mut Child, server: &Server, signal: i32) -> TestResult {
    let deadline = Instant::now() + PROCESS_TIMEOUT;
    while server.requests() == 0 {
        if let Some(status) = child.try_wait()? {
            return Err(format!("Viewer exited before the startup request: {status}").into());
        }
        if Instant::now() >= deadline {
            return Err("Viewer did not request its channel catalogue".into());
        }
        thread::sleep(POLL_INTERVAL);
    }
    signal_process(child, signal)?;
    let deadline = Instant::now() + PROCESS_TIMEOUT;
    loop {
        if let Some(status) = child.try_wait()? {
            return if status.success() {
                Ok(())
            } else {
                Err(format!("Viewer quit with {status} after signal {signal}").into())
            };
        }
        if Instant::now() >= deadline {
            return Err(format!("Viewer kept running after signal {signal}").into());
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn check_signal(signal: i32, name: &str) -> TestResult {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("nagametv/settings.toml");
    let server = Server::new()?;
    let mut preferences = settings::Loaded::open(path.clone())?.activate(Some(server.url()), None);
    preferences.change(settings::Change::AutoUpdateCheck(false));
    assert!(preferences.preferences().window_size.is_none());
    assert!(!preferences.preferences().hdmi_cec_enabled);
    preferences.flush()?;

    let log = directory.path().join("viewer.log");
    let mut child = Command::new(std::env::current_exe()?)
        .env("XDG_CONFIG_HOME", directory.path())
        .env("QT_QPA_PLATFORM", "wayland")
        .env("RUST_LOG", "info")
        .stderr(Stdio::from(File::create(&log)?))
        .spawn()?;
    let result = wait_for_app_exit(&mut child, &server, signal);
    if result.is_err() && child.try_wait()?.is_none() {
        child.kill()?;
        child.wait()?;
    }
    let log = std::fs::read_to_string(log)?;
    result.map_err(|error| format!("{error}\n{log}"))?;
    assert_eq!(
        log.lines()
            .filter(
                |line| line.contains("Application quit requested by signal") && line.contains(name)
            )
            .count(),
        1,
        "{log}"
    );
    // Main.qml remembers the geometry after Player::shutdown succeeds.
    assert!(
        settings::Loaded::open(path)?
            .preferences()
            .window_size
            .is_some()
    );
    eprintln!("{name}: production Qt quit saved the window geometry");
    Ok(())
}

pub(super) fn run() -> i32 {
    for (signal, name) in [
        (libc::SIGTERM, "SIGTERM"),
        (libc::SIGINT, "SIGINT"),
        (libc::SIGHUP, "SIGHUP"),
    ] {
        if let Err(error) = check_signal(signal, name) {
            tracing::error!(
                signal = name,
                error = error.as_ref(),
                "Signal quit check failed"
            );
            return 1;
        }
    }
    0
}
