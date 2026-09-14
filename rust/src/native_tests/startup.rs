//! Load the production Main.qml, Player and video item using validated hardware.
use super::bridge::ffi;
use crate::{features, playback, player, settings};
use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
static QML_WARNINGS: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn record_qt(level: u8, category: &str, message: &str) {
    if level >= 2 {
        eprintln!("Qt [{category}]: {message}");
    }
    if level >= 2 && (message.contains("qrc:/") || category.starts_with("qt.qml")) {
        QML_WARNINGS.lock().unwrap().push(message.into());
    }
}

struct Server {
    url: String,
    requests: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<std::io::Result<()>>>,
}

impl Server {
    fn new() -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let url = format!("http://{}", listener.local_addr()?);
        let requests = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let count = requests.clone();
        let stopped = stop.clone();
        let worker = thread::spawn(move || {
            while !stopped.load(Ordering::Relaxed) {
                let mut socket = match listener.accept() {
                    Ok((socket, _)) => socket,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(error) => return Err(error),
                };
                socket.set_read_timeout(Some(Duration::from_secs(3)))?;
                socket.set_write_timeout(Some(Duration::from_secs(3)))?;
                let mut header = Vec::new();
                while !header.ends_with(b"\r\n\r\n") {
                    if header.len() >= 8192 {
                        return Err(std::io::ErrorKind::InvalidData.into());
                    }
                    let mut byte = [0];
                    socket.read_exact(&mut byte)?;
                    header.push(byte[0]);
                }
                let body = if header.starts_with(b"GET /api/services ") {
                    count.fetch_add(1, Ordering::Relaxed);
                    r#"[{"id":1,"name":"First TV","type":1,"channel":{"type":"GR"}},
                        {"id":2,"name":"Saved TV","type":1,"channel":{"type":"GR"}}]"#
                } else {
                    "[]"
                };
                let status = if header.starts_with(b"GET /api/services/2/stream") {
                    503
                } else {
                    200
                };
                if let Err(error) = write!(
                    socket,
                    "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                ) && !matches!(
                    error.kind(),
                    std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset
                ) {
                    return Err(error);
                }
            }
            Ok(())
        });
        Ok(Self {
            url,
            requests,
            stop,
            worker: Some(worker),
        })
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .expect("HTTP fixture thread")
                .expect("HTTP fixture requests");
        }
    }
}

fn evaluate(engine: &mut cxx::UniquePtr<QQmlApplicationEngine>, source: &str) -> TestResult<bool> {
    ffi::evaluate_root(engine.pin_mut(), &QString::from(source))?
        .value::<bool>()
        .ok_or_else(|| format!("Expected a boolean: {source}").into())
}

fn wait_for(
    app: &QGuiApplication,
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
    source: &str,
) -> TestResult {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        app.process_events();
        if evaluate(engine, source)? {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!("Timed out: {source}").into());
        }
        thread::sleep(Duration::from_millis(5));
    }
}

fn check_danmaku_layout(
    app: &QGuiApplication,
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
) -> TestResult {
    // Use the production overlay and layout with synthetic comments, without
    // needing a successful broadcast stream from the HTTP failure fixture.
    evaluate(
        engine,
        "root.showProgram = false; root.width = 1280; root.height = 720; danmaku.active = true; true",
    )?;
    wait_for(app, engine, "danmaku.item !== null")?;
    assert!(evaluate(
        engine,
        r#"
        danmaku.item.receive("resize flow", "right", 16777215);
        danmaku.item.receive("resize fixed", "top", 16777215);
        danmaku.item.paused = true;
        danmaku.item.activeCount === 2
    "#
    )?);
    for (open, width) in [(true, 1280), (false, 1280), (true, 1100), (false, 1200)] {
        assert!(evaluate(
            engine,
            &format!(
                r#"
            (function() {{
                const before = Array.from(danmaku.item.visuals.values());
                root.showProgram = {open}; root.width = {width};
                const after = Array.from(danmaku.item.visuals.values());
                return after.length === 2 && before.every((entry, i) => entry === after[i])
                    && after[1].x === (danmaku.width - after[1].width) / 2;
            }})()
        "#
            )
        )?);
        wait_for(
            app,
            engine,
            "danmaku.item.activeCount === 2 && danmaku.item.visualCount === 2",
        )?;
    }
    for size in [36, 72, 14] {
        assert!(evaluate(
            engine,
            &format!(
                r#"
            (function() {{
                const before = Array.from(danmaku.item.visuals.values());
                const oldWidth = before[1].width;
                player.configure_danmaku(true, {size}, player.comment_opacity, player.comment_speed);
                const after = Array.from(danmaku.item.visuals.values());
                return after.length === 2 && before.every((entry, i) => entry === after[i])
                    && after[1].font.pixelSize === {size} && after[1].width !== oldWidth;
            }})()
        "#
            )
        )?);
    }
    evaluate(engine, "danmaku.active = false; true")?;
    Ok(())
}

fn window(app: &QGuiApplication, preferences: &settings::Preferences) -> TestResult {
    let _preloaded = playback::preload()?;
    let mut engine = QQmlApplicationEngine::new();
    assert!(player::ffi::initialize_ui_language(
        engine.pin_mut(),
        &QString::from("en")
    ));
    engine
        .pin_mut()
        .load(&QUrl::from("qrc:/qt/qml/MinimalViewer/qml/Main.qml"));
    assert_eq!(ffi::root_count(&engine), 1);
    assert!(evaluate(&mut engine, "!screenshot.canCapture")?);
    assert!(evaluate(
        &mut engine,
        &format!("player.autoplay === {}", preferences.autoplay)
    )?);
    if !preferences.server.is_empty() {
        wait_for(
            app,
            &mut engine,
            "!player.loading && root.channelRows.length === 2",
        )?;
        let autoplay = settings::autoplay_requested(
            preferences.autoplay,
            std::env::var("MIRAKURUN_AUTOPLAY").ok().as_deref(),
        );
        assert!(evaluate(
            &mut engine,
            "!root.setupRequired && !setup.visible && player.server_configured && player.selected === 1"
        )?);
        if autoplay {
            // No explicit play(): the saved setting / launch override must start
            // the restored channel after its asynchronous catalog arrives.
            wait_for(
                app,
                &mut engine,
                "!player.connecting && player.playback_error.length > 0",
            )?;
        } else {
            assert!(evaluate(
                &mut engine,
                "!player.playing && !player.connecting && !player.playback_error.length"
            )?);
        }
        // The restored channel is now stable. Exercise danmaku before opening
        // the channel browser, whose navigation is checked separately below.
        check_danmaku_layout(app, &mut engine)?;
        // Every way of changing guide visibility must run the same synchronization.
        evaluate(&mut engine, "root.toggleGuide(); true")?;
        wait_for(app, &mut engine, "root.guideVisible && root.showGuide")?;
        evaluate(&mut engine, "root.closeTopmost(); true")?;
        assert!(evaluate(
            &mut engine,
            "!root.guideVisible && !root.showGuide"
        )?);
        evaluate(
            &mut engine,
            "root.toggleGuide(); root.chooseConnectedChannel(); true",
        )?;
        wait_for(app, &mut engine, "!root.guideVisible && root.showChannels")?;
        // Exercise real GStreamer start/failure/stop without an external tuner.
        evaluate(&mut engine, "player.play(); true")?;
        wait_for(
            app,
            &mut engine,
            "!player.connecting && player.playback_error.length > 0",
        )?;
        assert!(evaluate(
            &mut engine,
            "!player.playing && player.selected === 1"
        )?);
        evaluate(&mut engine, "player.stop(); true")?;
    } else {
        wait_for(app, &mut engine, "setup.opened")?;
        assert!(evaluate(
            &mut engine,
            "root.setupRequired && !player.server_configured && !player.loading && !player.server.length && !root.channelRows.length"
        )?);
        check_danmaku_layout(app, &mut engine)?;
    }
    assert!(evaluate(&mut engine, "root.close(); root.closing")?);
    app.process_events();
    drop(engine);
    app.process_events();
    let warnings = QML_WARNINGS.lock().unwrap();
    assert!(warnings.is_empty(), "QML warnings: {warnings:?}");
    Ok(())
}

fn checks() -> TestResult {
    let server = Server::new()?;
    let path = settings::settings_path()?;
    assert!(
        !path.exists(),
        "run with the isolated test-startup.sh configuration"
    );
    launch_window(None)?;
    assert_eq!(server.requests.load(Ordering::Relaxed), 0);
    let mut preferences =
        settings::Loaded::open(path.clone())?.activate(Some(server.url.clone()), Some("2".into()));
    preferences.change(settings::Change::Comments(false));
    preferences.flush()?;
    for (autoplay, launch_override) in [
        (false, None),
        (false, None),
        (true, None),
        (true, Some("0")),
        (false, Some("1")),
    ] {
        preferences.change(settings::Change::Autoplay(autoplay));
        preferences.flush()?;
        let before = server.requests.load(Ordering::Relaxed);
        launch_window(launch_override)?;
        assert!(server.requests.load(Ordering::Relaxed) > before);
        let saved = settings::Loaded::open(path.clone())?;
        assert_eq!(saved.preferences().server, server.url);
        assert_eq!(saved.preferences().service_id, "2");
        assert_eq!(saved.preferences().autoplay, autoplay);
    }
    println!(
        "Main.qml checks passed: first run, saved startup, autoplay and environment overrides, guide/channel visibility, clean shutdown"
    );
    Ok(())
}

fn launch_window(autoplay_override: Option<&str>) -> TestResult {
    let mut command = std::process::Command::new(std::env::current_exe()?);
    command.args(["--native-tests", "startup-window"]);
    match autoplay_override {
        Some(value) => {
            command.env("MIRAKURUN_AUTOPLAY", value);
        }
        None => {
            command.env_remove("MIRAKURUN_AUTOPLAY");
        }
    }
    let status = command.status()?;
    if !status.success() {
        return Err(format!("startup process failed: {status}").into());
    }
    Ok(())
}

pub fn run_window() -> i32 {
    let result = (|| -> TestResult {
        features::PLAN
            .set(features::LaunchPlan::parse([])?)
            .map_err(|_| "plan already initialized")?;
        player::ffi::install_qt_logging(record_qt);
        cxx_qt::init_qml_module!("MinimalViewer");
        player::ffi::configure_qt_quick_open_gl();
        let app = QGuiApplication::new();
        assert!(!app.is_null());
        let preferences = settings::Loaded::open(settings::settings_path()?)?;
        window(&app, preferences.preferences())
    })();
    match result {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("Startup window failed: {error}");
            1
        }
    }
}

pub fn run() -> i32 {
    match checks() {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("Startup checks failed: {error}");
            1
        }
    }
}
