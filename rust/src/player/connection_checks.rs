//! Exercise the real Player and HTTP worker with QCoreApplication only.
use super::ffi;
use crate::{features, settings};
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QCoreApplication, QString};
use std::{
    io::{Read, Write},
    net::TcpListener,
    path::Path,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;
const CHANNELS: &str = r#"[{"id":1,"name":"TV","type":1,"channel":{"type":"GR"}}]"#;

struct Response {
    url: String,
    received: mpsc::Receiver<()>,
    release: mpsc::Sender<()>,
    worker: thread::JoinHandle<std::io::Result<()>>,
}
impl Response {
    fn new(status: u16, body: &'static str) -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let url = format!("http://{}", listener.local_addr()?);
        let (sent, received) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        let worker = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(1))
                    }
                    Err(error) => return Err(error),
                }
            };
            socket.set_read_timeout(Some(Duration::from_secs(5)))?;
            socket.set_write_timeout(Some(Duration::from_secs(5)))?;
            let mut header = Vec::new();
            while !header.ends_with(b"\r\n\r\n") {
                assert!(header.len() < 4096);
                let mut byte = [0];
                socket.read_exact(&mut byte)?;
                header.push(byte[0]);
            }
            assert!(header.starts_with(b"GET /api/services "));
            sent.send(()).expect("test receives request");
            wait.recv_timeout(Duration::from_secs(5))
                .expect("test releases response");
            write!(
                socket,
                "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )?;
            Ok(())
        });
        Ok(Self {
            url,
            received,
            release,
            worker,
        })
    }
    fn begin(&self, player: &mut cxx::UniquePtr<ffi::Player>) -> TestResult {
        assert!(
            player
                .pin_mut()
                .connect_server(QString::from(self.url.as_str()))
        );
        assert!(player.loading());
        player.pin_mut().poll_channels()?;
        self.received.recv_timeout(Duration::from_secs(5))?;
        Ok(())
    }
    fn finish(self, player: &mut cxx::UniquePtr<ffi::Player>) -> TestResult {
        self.release.send(())?;
        self.worker.join().map_err(|_| "HTTP fixture panicked")??;
        let deadline = Instant::now() + Duration::from_secs(5);
        while player.loading() {
            assert!(Instant::now() < deadline, "connection did not finish");
            player.pin_mut().poll_channels()?;
            thread::sleep(Duration::from_millis(1));
        }
        assert!(player.rust().pending_server.is_none());
        Ok(())
    }
}

fn saved_server(path: &Path) -> String {
    settings::Loaded::open(path.to_owned())
        .expect("read saved preferences")
        .preferences()
        .server
        .clone()
}

fn checks() -> TestResult {
    features::PLAN
        .set(features::LaunchPlan::parse(["--features=none".into()])?)
        .map_err(|_| "test plan already initialized")?;
    let app = QCoreApplication::new();
    assert!(!app.is_null());
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("settings.toml");
    let mut player = ffi::new_player();
    player.pin_mut().rust_mut().preferences =
        settings::Loaded::open(path.clone())?.activate(None, None);
    assert!(
        player.rust().media.playback().is_none(),
        "test must not initialize playback"
    );
    assert!(player.server().is_empty());
    assert!(!player.server_configured());
    let configured_changes = Arc::new(Mutex::new(Vec::new()));
    let recorded_configuration = configured_changes.clone();
    let _configured_signal = player
        .pin_mut()
        .on_server_configured_changed(move |player| {
            recorded_configuration
                .lock()
                .unwrap()
                .push(player.server_configured());
        });
    let outcomes = Arc::new(Mutex::new(Vec::new()));
    let recorded = outcomes.clone();
    let _signal = player
        .pin_mut()
        .on_connection_finished(move |player, success, count| {
            assert!(
                !player.loading(),
                "completion observers see a finished request"
            );
            recorded.lock().unwrap().push((success, count));
        });

    // A pending candidate must never leak into unrelated preference saves.
    let response = Response::new(503, "unavailable")?;
    response.begin(&mut player)?;
    assert!(
        !player.server_configured(),
        "a candidate is not configuration"
    );
    assert!(!path.exists());
    assert!(outcomes.lock().unwrap().is_empty());
    player
        .pin_mut()
        .rust_mut()
        .preferences
        .change(crate::settings::Change::Volume(42.0.into()));
    player.pin_mut().save_settings();
    assert_eq!(saved_server(&path), "");
    response.finish(&mut player)?;
    player.pin_mut().save_settings();
    assert_eq!(saved_server(&path), "");
    assert_eq!(*outcomes.lock().unwrap(), [(false, 0)]);
    assert!(!player.rust().channel_refresh.enabled());
    assert!(!player.server_configured());
    assert!(configured_changes.lock().unwrap().is_empty());

    // Valid empty catalogs confirm the server but remain distinct from failures.
    for (body, count) in [("[]", 0), (CHANNELS, 1)] {
        let previous = saved_server(&path);
        let response = Response::new(200, body)?;
        let expected = response.url.clone();
        response.begin(&mut player)?;
        player.pin_mut().save_settings();
        assert_eq!(saved_server(&path), previous);
        response.finish(&mut player)?;
        assert_eq!(saved_server(&path), expected);
        assert!(player.server_configured());
        assert_eq!(outcomes.lock().unwrap().last(), Some(&(true, count)));
        assert!(!player.playing());
        assert!(!player.connecting());
    }
    let good_server = saved_server(&path);
    assert_eq!(*configured_changes.lock().unwrap(), [true]);

    // All Qt signal observers see the complete stream state, and a duplicate
    // Play cannot replenish the one automatic retry of an active attempt.
    check_stream_state(&mut player)?;
    check_guide_state();
    check_autoplay()?;
    check_screenshot_directory()?;

    // HTTP failures and non-Mirakurun responses preserve a working saved URL.
    for (status, body) in [(403, "denied"), (200, "<html>not Mirakurun</html>")] {
        let response = Response::new(status, body)?;
        response.begin(&mut player)?;
        response.finish(&mut player)?;
        player.pin_mut().save_settings();
        assert_eq!(saved_server(&path), good_server);
        assert_eq!(outcomes.lock().unwrap().last(), Some(&(false, 0)));
    }
    let before = outcomes.lock().unwrap().len();
    assert!(
        !player
            .pin_mut()
            .connect_server(QString::from("file:///tmp/no-server"))
    );
    assert_eq!(saved_server(&path), good_server);
    assert_eq!(outcomes.lock().unwrap().len(), before);

    // A valid response cannot conceal a failed settings write from the form.
    std::fs::remove_file(&path)?;
    std::fs::create_dir(&path)?;
    let response = Response::new(200, CHANNELS)?;
    let expected = response.url.clone();
    response.begin(&mut player)?;
    response.finish(&mut player)?;
    assert!(!player.settings_error().is_empty());
    assert!(path.is_dir());
    std::fs::remove_dir(&path)?;
    player.pin_mut().save_settings();
    assert!(player.settings_error().is_empty());
    assert_eq!(saved_server(&path), expected);

    // Shutdown during an outstanding attempt must keep the confirmed server.
    assert!(
        player
            .pin_mut()
            .connect_server(QString::from("http://127.0.0.1:1"))
    );
    assert!(player.pin_mut().shutdown());
    assert!(!player.loading());
    assert_eq!(saved_server(&path), expected);
    println!(
        "Connection checks passed: pending/failed saves, empty catalog, success, save failure, shutdown"
    );
    Ok(())
}

fn check_stream_state(player: &mut cxx::UniquePtr<ffi::Player>) -> TestResult {
    use super::stream_state::{Attempt, State};
    let observed = Arc::new(Mutex::new(Vec::new()));
    let connecting = observed.clone();
    let playing = observed.clone();
    let _connecting_signal = player.pin_mut().on_connecting_changed(move |player| {
        connecting
            .lock()
            .unwrap()
            .push((player.connecting(), player.playing()));
    });
    let _playing_signal = player.pin_mut().on_playing_changed(move |player| {
        playing
            .lock()
            .unwrap()
            .push((player.connecting(), player.playing()));
    });
    let attempt = Attempt::new(&player.rust().entries[0]);
    player
        .pin_mut()
        .update_stream_state(State::Connecting(attempt));
    let attempt = player.rust().stream_state.resume_retry().unwrap();
    player
        .pin_mut()
        .update_stream_state(State::Playing(attempt));
    player.pin_mut().play();
    assert!(player.playing());
    assert!(player.rust().stream_state.resume_retry().is_none());
    player.pin_mut().end_stream()?;
    assert_eq!(
        *observed.lock().unwrap(),
        [(true, false), (false, true), (false, true), (false, false)]
    );
    Ok(())
}

fn check_autoplay() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("settings.toml");
    let mut player = ffi::new_player();
    player.pin_mut().rust_mut().preferences =
        settings::Loaded::open(path.clone())?.activate(None, None);
    let changes = Arc::new(Mutex::new(Vec::new()));
    let observed = changes.clone();
    let _signal = player.pin_mut().on_autoplay_changed(move |player| {
        let value = player.autoplay();
        assert_eq!(value, player.rust().preferences.preferences().autoplay);
        observed.lock().unwrap().push(value);
    });
    assert!(!player.autoplay());
    for enabled in [true, true, false] {
        player.pin_mut().configure_autoplay(enabled);
        assert_eq!(player.autoplay(), enabled);
        assert_eq!(
            settings::Loaded::open(path.clone())?.preferences().autoplay,
            enabled
        );
        assert!(
            !player.rust().autoplay_pending,
            "edits apply on the next launch"
        );
        assert!(!player.loading() && !player.connecting() && !player.playing());
    }
    assert_eq!(*changes.lock().unwrap(), [true, false]);
    // A write error is visible, while the in-memory choice stays coherent and
    // can be saved again without requiring another toggle.
    std::fs::remove_file(&path)?;
    std::fs::create_dir(&path)?;
    player.pin_mut().configure_autoplay(true);
    assert!(player.autoplay());
    assert!(!player.settings_error().is_empty());
    std::fs::remove_dir(&path)?;
    player.pin_mut().configure_autoplay(true);
    assert!(player.settings_error().is_empty());
    assert!(settings::Loaded::open(path)?.preferences().autoplay);
    Ok(())
}

fn check_screenshot_directory() -> TestResult {
    use cxx_qt_lib::{QColor, QImage, QImageFormat, QUrl};
    let temporary = tempfile::tempdir()?;
    let path = temporary.path().join("settings.toml");
    let directory = temporary.path().join("画像 #100%");
    let mut player = ffi::new_player();
    player.pin_mut().rust_mut().preferences =
        settings::Loaded::open(path.clone())?.activate(None, None);
    let observed = Arc::new(Mutex::new(Vec::new()));
    let changes = observed.clone();
    let _signal = player
        .pin_mut()
        .on_screenshot_directory_changed(move |player| {
            let path = player.screenshot_directory().to_string();
            let saved = player
                .rust()
                .preferences
                .preferences()
                .screenshot_directory
                .resolve(Path::new(""));
            assert_eq!(saved.as_deref(), Some(Path::new(&path)));
            assert_eq!(
                std::fs::read_dir(&path).unwrap().count(),
                0,
                "the writable probe is released before notification"
            );
            changes.lock().unwrap().push(path);
        });
    let url = QUrl::from_local_file(&QString::from(directory.to_string_lossy().as_ref()));
    assert!(player.pin_mut().configure_screenshot_directory(url.clone()));
    assert_eq!(player.screenshot_directory_url(), url);
    assert!(player.pin_mut().configure_screenshot_directory(url));
    assert_eq!(observed.lock().unwrap().len(), 1);
    let mut restored = ffi::new_player();
    restored.pin_mut().rust_mut().preferences = settings::Loaded::open(path)?.activate(None, None);
    assert_eq!(
        restored.screenshot_directory(),
        player.screenshot_directory()
    );
    assert!(
        !player
            .pin_mut()
            .configure_screenshot_directory(QUrl::from("https://example.test/folder"))
    );
    assert!(!player.screenshot_error().is_empty());
    let mut image = QImage::from_width_height_and_format(8, 6, QImageFormat::Format_RGB32);
    image.fill(&QColor::from_rgb(255, 0, 0));
    let saved = player
        .pin_mut()
        .save_screenshot(&image)
        .to_local_file()
        .expect("saved local image");
    let bytes = std::fs::read(saved.to_string())?;
    let decoded = QImage::from_data(&bytes, Some("PNG")).expect("PNG image");
    assert_eq!(decoded.size(), image.size());
    assert!(player.screenshot_error().is_empty());
    // Losing the destination after configuration reports an error and permits
    // recovery without changing the saved folder or overwriting existing files.
    std::fs::remove_file(saved.to_string())?;
    std::fs::remove_dir(&directory)?;
    std::fs::write(&directory, b"keep")?;
    assert!(player.pin_mut().save_screenshot(&image).is_empty());
    assert!(!player.screenshot_error().is_empty());
    assert_eq!(std::fs::read(&directory)?, b"keep");
    std::fs::remove_file(&directory)?;
    assert!(!player.pin_mut().save_screenshot(&image).is_empty());
    assert!(player.screenshot_error().is_empty());
    Ok(())
}

fn check_guide_state() {
    use crate::features::program_info::guide::Guide;
    let mut player = ffi::new_player();
    player.pin_mut().rust_mut().epg_enabled = true;
    let changes = Arc::new(Mutex::new(Vec::new()));
    let recorded = changes.clone();
    let _signal = player
        .pin_mut()
        .on_guide_visible_changed(move |mut player| {
            recorded.lock().unwrap().push(player.guide_visible());
            if player.guide_visible() {
                // A newly constructed QML guide asks for its day during notification.
                player.as_mut().guide_day(0.0, 86_400_000.0);
                assert!(matches!(player.rust().guide, Guide::Showing(_)));
            }
        });
    player.pin_mut().guide_open(true);
    player.pin_mut().guide_open(true);
    assert!(
        matches!(player.rust().guide, Guide::Showing(_)),
        "reopening must not discard the current day"
    );
    player.pin_mut().guide_open(false);
    player.pin_mut().guide_day(0.0, 86_400_000.0);
    assert!(matches!(player.rust().guide, Guide::Closed));
    assert_eq!(*changes.lock().unwrap(), [true, false]);
}

pub fn run() -> i32 {
    match checks() {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("Connection checks failed: {error}");
            1
        }
    }
}
