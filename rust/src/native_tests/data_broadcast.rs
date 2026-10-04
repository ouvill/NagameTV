//! Exercise the bundled web-bml with synthetic BML over its real WebSocket API.
//! Playback/display/audio use the same validated GUI session as the startup suite.
use super::bridge::ffi;
use super::startup::{TestResult, evaluate, wait_for};
pub(super) use crate::features::data_broadcast::test_fixture as fixture;
use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString};
use serde_json::{Value, json};
use std::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use tokio_tungstenite::tungstenite::{self, Message};

const COMPONENT: u8 = 0x40;
const SOCKET_TIMEOUT: Duration = Duration::from_secs(5);
const OBSERVER: &str =
    "root.contentItem.children.find(child => child.objectName === 'bmlObserver')";
const STARTUP: &str = include_str!("../../../tests/fixtures/bml/startup.bml");
const VISIBLE_STARTUP: &str = include_str!("../../../tests/fixtures/bml/startup-visible.bml");
const PRELOAD_STARTUP: &str = include_str!("../../../tests/fixtures/bml/startup-preload.bml");
const TIMER_STARTUP: &str = include_str!("../../../tests/fixtures/bml/startup-timer.bml");
const INITIALIZING_STARTUP: &str =
    include_str!("../../../tests/fixtures/bml/startup-initializing.bml");
const TOP: &str = include_str!("../../../tests/fixtures/bml/top.bml");
const CHILD: &str = include_str!("../../../tests/fixtures/bml/child.bml");
const SCRIPT: &str = include_str!("../../../tests/fixtures/bml/show.ecm");
const VIDEO_SAMPLE_DIVISIONS: i32 = 10;
// Allow for MPEG-2 quantization and texture filtering of the black/white fixture.
const DARK_CHANNEL_MAX: i32 = 60;
const BRIGHT_CHANNEL_MIN: i32 = 180;
const WHITE_CHANNEL_MIN: i32 = 240;
const OVERLAY_MIN_PIXELS: usize = 100;

fn start_overlays(
    app: &QGuiApplication,
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
) -> TestResult {
    // The transport fixture has no caption ES or comment service. Feed authored
    // cues into the production overlays to verify their final Qt composition.
    evaluate(
        engine,
        "captions.active = true; danmaku.active = true; true",
    )?;
    wait_for(
        app,
        engine,
        "captions.view !== null && danmaku.view !== null",
    )?;
    let cue = json!({"planeWidth":1920,"planeHeight":1080,"text":"字幕", "cells":[{
        "text":"字幕", "x":640,"y":760,"width":240,"height":160,"glyphWidth":240,"glyphHeight":120,
        "foreground":"#00ff00","background":"transparent","stroke":"#000000","stroked":true,
        "bold":true,"italic":false,"underline":false
    }]});
    evaluate(
        engine,
        &format!(
            "captions.view.captionJson = {}; danmaku.view.replayReady = false; danmaku.view.playbackClock = null; danmaku.view.paused = true; player.configure_danmaku(true, 36, 1, 1); danmaku.view.controller.load_timeline(JSON.stringify([{{time:0,text:'COMMENT',type:'top',color:16711680}}])); danmaku.view.controller.seek(1); {OBSERVER}.overlays = true; true",
            serde_json::to_string(&cue.to_string())?
        ),
    )?;
    Ok(())
}

fn check_overlays(
    app: &QGuiApplication,
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
) -> TestResult {
    assert!(evaluate(
        engine,
        r#"(() => {
        const origin = item => item.mapToItem(video, 0, 0);
        const caption = origin(captions);
        const comments = origin(danmaku);
        const bitmap = videoViewport.children.find(item => item.objectName === 'mediaCaption');
        const bitmapOrigin = origin(bitmap);
        return videoViewport.clip && Math.abs(caption.x) < 1 && Math.abs(caption.y) < 1
            && Math.abs(captions.width - video.width) < 1 && Math.abs(captions.height - video.height) < 1
            && Math.abs(comments.x) < 1 && Math.abs(comments.y) < 1
            && Math.abs(danmaku.width - video.width) < 1 && Math.abs(danmaku.height - video.height) < 1
            && bitmapOrigin.x === 0 && bitmapOrigin.y === 0
            && bitmap.width === video.width && bitmap.height === video.height
            && danmaku.view.fontSize === Math.round(36 * danmaku.height / 720);
    })()"#
    )?);
    let geometry = super::screenshots::json(
        engine,
        "(() => { const r = video.mapToItem(null, 0, 0, video.width, video.height); return {x:r.x,y:r.y,width:r.width,height:r.height,windowWidth:root.width}; })()",
    )?;
    let number = |name: &str| -> TestResult<f64> {
        geometry[name]
            .as_f64()
            .ok_or_else(|| format!("missing overlay geometry: {name}").into())
    };
    let deadline = Instant::now() + SOCKET_TIMEOUT;
    loop {
        app.process_events();
        let image = ffi::grabRoot(engine.pin_mut())?;
        let scale = f64::from(image.width()) / number("windowWidth")?;
        let red = super::screenshots::colored(&image, 0);
        let green = super::screenshots::colored(&image, 1);
        let contained = |bounds: (usize, i32, i32, i32, i32)| -> TestResult<bool> {
            Ok(bounds.0 >= OVERLAY_MIN_PIXELS
                && f64::from(bounds.1) >= number("x")? * scale - 1.
                && f64::from(bounds.2) >= number("y")? * scale - 1.
                && f64::from(bounds.3) <= (number("x")? + number("width")?) * scale + 1.
                && f64::from(bounds.4) <= (number("y")? + number("height")?) * scale + 1.)
        };
        if contained(red)? && contained(green)? {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!("BML overlays missing/outside video: comment={red:?}, caption={green:?}, video={geometry}").into());
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn module(id: u16, name: &str, content: &str, subtype: &str) -> Value {
    json!({
        "type": "moduleDownloaded", "componentId": COMPONENT, "moduleId": id,
        "version": 0, "dataEventId": 0,
        "files": [{
            "contentLocation": name,
            "contentType": { "type": "text", "subtype": subtype,
                "originalType": "text", "originalSubtype": subtype, "parameters": [] },
            "dataBase64": gstreamer::glib::base64_encode(content.as_bytes()).as_str(),
        }],
    })
}

struct Server {
    url: String,
    messages: mpsc::Sender<Value>,
    worker: Option<thread::JoinHandle<Result<(), String>>>,
}

impl Server {
    fn new() -> TestResult<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let url = format!("ws://{}", listener.local_addr()?);
        let (messages, receiver) = mpsc::channel();
        let worker = thread::spawn(move || {
            let run = || -> TestResult {
                let deadline = Instant::now() + SOCKET_TIMEOUT;
                let stream: TcpStream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            if Instant::now() >= deadline {
                                return Err("BML test WebSocket was not connected".into());
                            }
                            thread::sleep(Duration::from_millis(10));
                        }
                        Err(error) => return Err(error.into()),
                    }
                };
                stream.set_read_timeout(Some(SOCKET_TIMEOUT))?;
                stream.set_write_timeout(Some(SOCKET_TIMEOUT))?;
                let mut socket = tungstenite::accept(stream)?;
                socket.send(Message::Text(
                    json!({"type":"begin", "version":1, "epoch":"fixture"})
                        .to_string()
                        .into(),
                ))?;
                socket.send(Message::Text(
                    json!({"type":"ready", "epoch":"fixture"})
                        .to_string()
                        .into(),
                ))?;
                for message in receiver {
                    socket.send(Message::Text(serde_json::to_string(&message)?.into()))?;
                }
                // The browser normally closes first when the feature is disabled
                // or during test teardown.
                match socket.close(None) {
                    Ok(())
                    | Err(
                        tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed,
                    ) => Ok(()),
                    Err(error) => Err(error.into()),
                }
            };
            run().map_err(|error| {
                tracing::error!(operation = "serve BML test content", error = error.as_ref());
                error.to_string()
            })
        });
        Ok(Self {
            url,
            messages,
            worker: Some(worker),
        })
    }

    fn send(&self, message: Value) -> TestResult {
        self.messages
            .send(json!({"type":"update", "epoch":"fixture", "message":message}))?;
        Ok(())
    }

    fn finish(mut self) -> TestResult {
        let worker = self.worker.take().expect("test server worker");
        drop(self.messages);
        worker.join().map_err(|_| "BML test server panicked")??;
        Ok(())
    }
}

fn browser(
    app: &QGuiApplication,
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
    script: &str,
) -> TestResult {
    evaluate(
        engine,
        &format!(
            "(() => {{ const observer = {OBSERVER}; observer.done = false; dataBroadcast.view.runJavaScript({}, function(result) {{ observer.passed = result === true; observer.done = true; }}); return true; }})()",
            serde_json::to_string(script)?
        ),
    )?;
    wait_for(app, engine, &format!("{OBSERVER}.done"))?;
    assert!(
        evaluate(engine, &format!("{OBSERVER}.passed"))?,
        "browser script: {script}"
    );
    Ok(())
}

fn wait_document(
    app: &QGuiApplication,
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
    path: &str,
) -> TestResult {
    let script = format!(
        "window.nagamePresentation().state === 'presenting' && window.nagamePresentation().documentUrl === {}",
        serde_json::to_string(path)?
    );
    wait_browser(app, engine, &script)
}

fn wait_browser(
    app: &QGuiApplication,
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
    script: &str,
) -> TestResult {
    // Ask WebEngine repeatedly because load completion happens in its process.
    let deadline = Instant::now() + SOCKET_TIMEOUT;
    loop {
        evaluate(
            engine,
            &format!(
                "(() => {{ const observer = {OBSERVER}; observer.done = false; dataBroadcast.view.runJavaScript({}, function(result) {{ observer.passed = result === true; observer.done = true; }}); return true; }})()",
                serde_json::to_string(&script)?
            ),
        )?;
        wait_for(app, engine, &format!("{OBSERVER}.done"))?;
        if evaluate(engine, &format!("{OBSERVER}.passed"))? {
            return Ok(());
        }
        if Instant::now() >= deadline {
            browser(
                app,
                engine,
                "console.error('BML test timeout:', JSON.stringify(window.nagamePresentation())); true",
            )?;
            return Err(format!("BML condition did not become true: {script}").into());
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn check_video_window(
    app: &QGuiApplication,
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
) -> TestResult {
    wait_for(app, engine, "!!dataBroadcast.view.videoRect")?;
    if evaluate(engine, &format!("{OBSERVER}.overlays"))? {
        check_overlays(app, engine)?;
    }
    browser(
        app,
        engine,
        "(() => { const r = window.nagameVideoRect(); const scale = Math.min(innerWidth / 960, innerHeight / 540); return r !== null && Math.abs(r.width - 480 * scale) < 1 && Math.abs(r.height - 270 * scale) < 1 && Math.abs(r.x - ((innerWidth - 960 * scale) / 2 + 60 * scale)) < 1 && Math.abs(r.y - ((innerHeight - 540 * scale) / 2 + 120 * scale)) < 1; })()",
    )?;
    // Sample the actual composed window. Correct geometry alone cannot detect
    // an opaque BML background covering the native video's moving white ball.
    let geometry = super::screenshots::json(
        engine,
        "(() => { const r = video.mapToItem(null, 0, 0, video.width, video.height); return { x: r.x, y: r.y, width: r.width, height: r.height, windowWidth: root.width }; })()",
    )?;
    let number = |name: &str| -> TestResult<f64> {
        geometry[name]
            .as_f64()
            .ok_or_else(|| format!("missing video geometry: {name}").into())
    };
    let deadline = Instant::now() + SOCKET_TIMEOUT;
    let sample_count = (VIDEO_SAMPLE_DIVISIONS - 1).pow(2);
    loop {
        app.process_events();
        let image = ffi::grabRoot(engine.pin_mut())?;
        let scale = f64::from(image.width()) / number("windowWidth")?;
        let mut dark = 0;
        let mut brightest = 0;
        for row in 1..VIDEO_SAMPLE_DIVISIONS {
            for column in 1..VIDEO_SAMPLE_DIVISIONS {
                let x = (number("x")?
                    + number("width")? * f64::from(column) / f64::from(VIDEO_SAMPLE_DIVISIONS))
                    * scale;
                let y = (number("y")?
                    + number("height")? * f64::from(row) / f64::from(VIDEO_SAMPLE_DIVISIONS))
                    * scale;
                let color = image.pixel_color(x as i32, y as i32);
                brightest = brightest.max(color.red());
                if color.red() < DARK_CHANNEL_MAX {
                    dark += 1;
                }
            }
        }
        // The white BML background to the right must still be drawn. Making
        // the whole page transparent would reveal the ball but lose the BML.
        let background = image.pixel_color(
            ((number("x")? + number("width")? * 1.5) * scale) as i32,
            ((number("y")? + number("height")? * 0.5) * scale) as i32,
        );
        let background_visible = background.red() > WHITE_CHANNEL_MIN
            && background.green() > WHITE_CHANNEL_MIN
            && background.blue() > WHITE_CHANNEL_MIN;
        if dark > sample_count / 2 && brightest > BRIGHT_CHANNEL_MIN && background_visible {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!("BML video composition failed: {dark}/{sample_count} dark samples, brightest={brightest}, background_visible={background_visible}").into());
        }
        thread::sleep(Duration::from_millis(10));
    }
}

pub(super) fn run(
    app: &QGuiApplication,
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
) -> TestResult {
    run_startup(app, engine, STARTUP)?;
    run_startup(app, engine, VISIBLE_STARTUP)?;
    run_startup(app, engine, PRELOAD_STARTUP)?;
    run_startup(app, engine, INITIALIZING_STARTUP)?;
    run_startup(app, engine, TIMER_STARTUP)
}

pub(super) fn click_remote(
    app: &QGuiApplication,
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
    button: &str,
) -> TestResult {
    evaluate(
        engine,
        "surface.forceActiveFocus(); overlayVisibility.reveal(); root.showProgram = true; true",
    )?;
    wait_for(app, engine, "sidebar.view !== null && sidebar.reveal === 1")?;
    ffi::clickRootItem(engine.pin_mut(), &QString::from("remoteSidebarTab"))?;
    wait_for(app, engine, "sidebar.view.page === ProgramSidebar.Remote")?;
    ffi::clickRootItem(engine.pin_mut(), &QString::from(button))?;
    ffi::clickRootKey(engine.pin_mut(), &QString::from("Escape"))?;
    wait_for(app, engine, "!root.showProgram && sidebar.reveal === 0")?;
    wait_for(app, engine, "inputContext.videoFocused")
}

fn check_remote(
    app: &QGuiApplication,
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
) -> TestResult {
    evaluate(
        engine,
        "overlayVisibility.reveal(); root.showProgram = true; true",
    )?;
    wait_for(app, engine, "sidebar.view !== null && sidebar.reveal === 1")?;
    ffi::clickRootItem(engine.pin_mut(), &QString::from("remoteSidebarTab"))?;
    wait_for(
        app,
        engine,
        "sidebar.view.page === ProgramSidebar.Remote && inputContext.sidebarFocused",
    )?;
    // Pointer input must reach the real BML document while the sidebar owns focus.
    evaluate(
        engine,
        &format!("{OBSERVER}.previousVideoRect = dataBroadcast.view.videoRect; true"),
    )?;
    ffi::clickRootItem(engine.pin_mut(), &QString::from("remoteKey_r"))?;
    wait_for(
        app,
        engine,
        &format!("dataBroadcast.view.videoRect.x > {OBSERVER}.previousVideoRect.x + 1"),
    )?;
    ffi::clickRootItem(engine.pin_mut(), &QString::from("remoteKey_b"))?;
    wait_for(
        app,
        engine,
        &format!("Math.abs(dataBroadcast.view.videoRect.x - {OBSERVER}.previousVideoRect.x) < 1"),
    )?;
    for (width, height) in [(1280, 720), (640, 360)] {
        evaluate(
            engine,
            &format!("root.width = {width}; root.height = {height}; true"),
        )?;
        super::startup::capture_navigation(app, engine, &format!("data-remote-{width}.png"))?;
        assert!(evaluate(
            engine,
            "inputContext.sidebarFocused && !dataBroadcast.view.activeFocus"
        )?);
    }
    evaluate(engine, "root.width = 1280; root.height = 720; true")?;
    ffi::clickRootItem(engine.pin_mut(), &QString::from("remoteKey_Backspace"))?;
    wait_for(
        app,
        engine,
        "!root.showDataBroadcast && root.showProgram && inputContext.sidebarFocused",
    )?;
    ffi::clickRootItem(engine.pin_mut(), &QString::from("remoteDataButton"))?;
    wait_document(app, engine, "/40/0001/top.bml")?;
    assert!(evaluate(
        engine,
        "root.showProgram && inputContext.sidebarFocused"
    )?);
    ffi::clickRootKey(engine.pin_mut(), &QString::from("Escape"))?;
    wait_for(
        app,
        engine,
        "!root.showProgram && inputContext.navigationEnabled",
    )?;
    wait_for(app, engine, "sidebar.reveal === 0")?;
    assert!(evaluate(engine, "root.showDataBroadcast")?);
    Ok(())
}

fn send_content(server: &Server, startup: &str) -> TestResult {
    server.send(json!({ "type": "pmt", "components": [{
        "pid": 0x101, "componentId": COMPONENT, "streamType": 0x0d, "dataComponentId": 0x000c,
    }] }))?;
    server.send(json!({ "type": "moduleListUpdated", "componentId": COMPONENT, "dataEventId": 0,
        "modules": (0..5).map(|id| json!({ "id": id, "version": 0, "size": 1 })).collect::<Vec<_>>() }))?;
    server.send(module(0, "startup.bml", startup, "x-arib-bml"))?;
    server.send(module(1, "top.bml", TOP, "x-arib-bml"))?;
    server.send(module(2, "child.bml", CHILD, "x-arib-bml"))?;
    Ok(())
}

fn check_keyboard_isolation(
    app: &QGuiApplication,
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
) -> TestResult {
    browser(
        app,
        engine,
        r#"window.nativeKeyEvents = 0;
        window.remoteKeyEvents = 0;
        window.addEventListener('keydown', () => window.nativeKeyEvents++, true);
        window.addEventListener('keyup', () => window.nativeKeyEvents++, true);
        const remoteKey = window.nagameRemoteKey;
        window.nagameRemoteKey = key => { window.remoteKeyEvents++; remoteKey(key); };
        true"#,
    )?;
    // Suppress playback side effects while checking every former BML binding.
    // The input context stays enabled, so accidental BML shortcuts still fire.
    evaluate(
        engine,
        "viewerActions.enabled = false; surface.forceActiveFocus(); true",
    )?;
    for key in [
        "Left",
        "Right",
        "Up",
        "Down",
        "Return",
        "Enter",
        "Space",
        "Back",
        "Backspace",
        "X",
        "Escape",
        "D",
        "B",
        "R",
        "G",
        "Y",
        "0",
        "1",
        "2",
        "3",
        "4",
        "5",
        "6",
        "7",
        "8",
        "9",
    ] {
        evaluate(engine, "surface.forceActiveFocus(); true")?;
        ffi::clickRootKey(engine.pin_mut(), &QString::from(key))?;
    }
    browser(
        app,
        engine,
        "window.nativeKeyEvents === 0 && window.remoteKeyEvents === 0",
    )?;
    wait_document(app, engine, "/40/0001/top.bml")?;
    evaluate(
        engine,
        "viewerActions.enabled = true; surface.forceActiveFocus(); true",
    )?;
    // Displaying and operating BML must leave native focus with the receiver.
    assert!(evaluate(engine, "!dataBroadcast.view.activeFocus")?);
    ffi::clickRootKey(engine.pin_mut(), &QString::from("Down"))?;
    wait_for(app, engine, "playerControls.activeFocus")?;
    ffi::clickRootKey(engine.pin_mut(), &QString::from("G"))?;
    wait_for(app, engine, "root.showGuide")?;
    ffi::clickRootKey(engine.pin_mut(), &QString::from("Escape"))?;
    wait_for(app, engine, "!root.showGuide")?;
    ffi::clickRootKey(engine.pin_mut(), &QString::from("Back"))?;
    wait_for(app, engine, "inputContext.videoFocused")?;
    ffi::clickRootKey(engine.pin_mut(), &QString::from("Escape"))?;
    browser(
        app,
        engine,
        "window.nativeKeyEvents === 0 && window.remoteKeyEvents === 0",
    )?;
    wait_document(app, engine, "/40/0001/top.bml")
}

fn run_startup(
    app: &QGuiApplication,
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
    startup: &str,
) -> TestResult {
    evaluate(
        engine,
        "surface.forceActiveFocus(); root.setDataBroadcast(true); true",
    )?;
    wait_for(
        app,
        engine,
        "dataBroadcast.view !== null && dataBroadcast.view.browserReady && player.data_broadcast_connected()",
    )?;
    let observer = r#"import QtQuick; Item {
        objectName: "bmlObserver"
        property bool done: false
        property bool passed: false
        property var retainedView
        property int previousRevision
        property var previousVideoRect
        property real previousWidth
        property bool overlays: false
    }"#;
    evaluate(
        engine,
        &format!(
            "Qt.createQmlObject({}, root.contentItem).retainedView = dataBroadcast.view; true",
            serde_json::to_string(observer)?
        ),
    )?;
    let mut server = Server::new()?;
    browser(
        app,
        engine,
        &format!(
            "window.nagameConnect({}); true",
            serde_json::to_string(&server.url)?
        ),
    )?;
    server.send(viewer_web_bml::program_info(42, Some(1)))?;
    if startup == STARTUP {
        // A complete PMT without a BML entry is not indefinite reception.
        server.send(json!({"type": "pmt", "components": []}))?;
        server.send(viewer_web_bml::program(&viewer_web_bml::ProgramInfo {
            service_id: 42,
            original_network_id: Some(1),
            transport_stream_id: Some(1),
            event: None,
        }))?;
        wait_for(
            app,
            engine,
            "dataBroadcast.view.presentation === DataBroadcastView.Unavailable",
        )?;
        // A subsequent PMT can add data broadcasting without reopening the view.
    }
    send_content(&server, startup)?;
    if startup == PRELOAD_STARTUP || startup == TIMER_STARTUP {
        wait_browser(
            app,
            engine,
            "window.nagamePresentation().documentUrl === '/40/0000/startup.bml' && window.nagamePresentation().state === 'opening'",
        )?;
        server.send(module(4, "startup.bml", VISIBLE_STARTUP, "x-arib-bml"))?;
    }
    wait_for(
        app,
        engine,
        "dataBroadcast.view.presentation === DataBroadcastView.Presenting",
    )?;
    // The visible startup deliberately shows only full-screen video. Opening
    // must send d once and reach the top page, just as with an invisible startup.
    wait_document(app, engine, "/40/0001/top.bml")?;
    if startup == STARTUP {
        start_overlays(app, engine)?;
    }
    check_video_window(app, engine)?;
    if startup == STARTUP {
        check_remote(app, engine)?;
        check_keyboard_isolation(app, engine)?;
        // Geometry notifications must reach Qt without the former 100 ms poll.
        evaluate(
            engine,
            &format!("{OBSERVER}.previousVideoRect = dataBroadcast.view.videoRect; true"),
        )?;
        browser(app, engine, "window.nagameRemoteKey('r'); true")?;
        wait_for(
            app,
            engine,
            &format!("dataBroadcast.view.videoRect.x > {OBSERVER}.previousVideoRect.x + 1"),
        )?;
        check_overlays(app, engine)?;
        browser(app, engine, "window.nagameRemoteKey('b'); true")?;
        wait_for(
            app,
            engine,
            &format!(
                "Math.abs(dataBroadcast.view.videoRect.x - {OBSERVER}.previousVideoRect.x) < 1"
            ),
        )?;
        check_video_window(app, engine)?;

        evaluate(
            engine,
            &format!("{OBSERVER}.previousWidth = root.width; root.width += 80; true"),
        )?;
        wait_for(
            app,
            engine,
            &format!(
                "Math.abs(dataBroadcast.view.videoRect.x - {OBSERVER}.previousVideoRect.x) > 1 || Math.abs(dataBroadcast.view.videoRect.width - {OBSERVER}.previousVideoRect.width) > 1"
            ),
        )?;
        check_video_window(app, engine)?;
        evaluate(
            engine,
            &format!("root.width = {OBSERVER}.previousWidth; true"),
        )?;
        wait_for(
            app,
            engine,
            &format!(
                "Math.abs(dataBroadcast.view.videoRect.x - {OBSERVER}.previousVideoRect.x) < 1 && Math.abs(dataBroadcast.view.videoRect.width - {OBSERVER}.previousVideoRect.width) < 1"
            ),
        )?;
    }
    if startup != STARTUP {
        if startup == VISIBLE_STARTUP {
            // A visible video-only document retains the default basic mask,
            // but has no BML key target. The remote must follow that availability.
            click_remote(app, engine, "remoteDataButton")?;
            wait_document(app, engine, "/40/0000/startup.bml")?;
            wait_for(
                app,
                engine,
                "inputContext.navigationEnabled && inputContext.videoFocused",
            )?;
            assert!(evaluate(
                engine,
                "root.showDataBroadcast && dataBroadcast.view.usedKeyList === 'basic data-button' && !dataBroadcast.view.inputAvailable"
            )?);
            ffi::clickRootKey(engine.pin_mut(), &QString::from("Down"))?;
            wait_for(app, engine, "playerControls.activeFocus")?;
            ffi::clickRootKey(engine.pin_mut(), &QString::from("G"))?;
            wait_for(app, engine, "root.showGuide")?;
            ffi::clickRootKey(engine.pin_mut(), &QString::from("Escape"))?;
            wait_for(app, engine, "!root.showGuide")?;
            click_remote(app, engine, "remoteDataButton")?;
            wait_document(app, engine, "/40/0001/top.bml")?;
            wait_for(
                app,
                engine,
                "dataBroadcast.view.usedKeyGroups.includes('basic')",
            )?;
            assert!(evaluate(
                engine,
                &format!("dataBroadcast.view === {OBSERVER}.retainedView")
            )?);
        }
        // The last case leaves WebEngine in BML standby for source changes.
        // The other cases release the browser through the feature setting.
        if startup != TIMER_STARTUP {
            evaluate(engine, "player.configure_data_broadcast(false); true")?;
            wait_for(app, engine, "dataBroadcast.view === null")?;
            evaluate(engine, "player.configure_data_broadcast(true); true")?;
        } else {
            click_remote(app, engine, "remoteKey_Backspace")?;
            wait_for(
                app,
                engine,
                "dataBroadcast.view.presentation === DataBroadcastView.Standby",
            )?;
        }
        evaluate(
            engine,
            &format!(
                "(() => {{ const observer = {OBSERVER}; observer.objectName = ''; observer.destroy(); return true; }})()"
            ),
        )?;
        return server.finish();
    }

    // Both a new receive generation and a lost subscriber replace the JS realm.
    // The playback-owned receiver remains available and the QML view is retained.
    for reset in [true, false] {
        browser(app, engine, "window.obsoleteSessionMarker = true; true")?;
        evaluate(
            engine,
            &format!("{OBSERVER}.previousRevision = dataBroadcast.view.activationRevision; true"),
        )?;
        if reset {
            server
                .messages
                .send(json!({"type":"begin", "version":1, "epoch":"replacement"}))?;
            wait_for(
                app,
                engine,
                &format!(
                    "dataBroadcast.view.activationRevision > {OBSERVER}.previousRevision && dataBroadcast.view.browserReady"
                ),
            )?;
            server.finish()?;
        } else {
            server.finish()?;
            wait_for(
                app,
                engine,
                &format!(
                    "dataBroadcast.view.activationRevision > {OBSERVER}.previousRevision && dataBroadcast.view.browserReady"
                ),
            )?;
        }
        browser(
            app,
            engine,
            "window.nagameReady && window.obsoleteSessionMarker === undefined",
        )?;
        assert!(evaluate(
            engine,
            &format!(
                "dataBroadcast.view === {OBSERVER}.retainedView && root.dataBroadcastSessionOpen"
            )
        )?);
        server = Server::new()?;
        browser(
            app,
            engine,
            &format!(
                "window.nagameConnect({}); true",
                serde_json::to_string(&server.url)?
            ),
        )?;
        server.send(viewer_web_bml::program_info(42, Some(1)))?;
        send_content(&server, startup)?;
        wait_document(app, engine, "/40/0001/top.bml")?;
        check_video_window(app, engine)?;
    }

    // A pending document retains the remote mask without taking keyboard focus.
    click_remote(app, engine, "remoteKey_Enter")?;
    wait_for(
        app,
        engine,
        "dataBroadcast.view.presentation === DataBroadcastView.Transitioning",
    )?;
    assert!(evaluate(
        engine,
        "root.showDataBroadcast && inputContext.navigationEnabled"
    )?);
    server.send(module(3, "show.ecm", SCRIPT, "x-arib-ecmascript"))?;
    wait_for(
        app,
        engine,
        "dataBroadcast.view.presentation === DataBroadcastView.Presenting",
    )?;
    click_remote(app, engine, "remoteKey_Backspace")?;
    wait_document(app, engine, "/40/0001/top.bml")?;
    check_video_window(app, engine)?;
    assert!(evaluate(engine, "root.showDataBroadcast")?);
    click_remote(app, engine, "remoteKey_Backspace")?;
    wait_for(
        app,
        engine,
        "!root.showDataBroadcast && inputContext.navigationEnabled && inputContext.videoFocused",
    )?;
    assert!(evaluate(
        engine,
        &format!(
            "root.dataBroadcastSessionOpen && dataBroadcast.view === {OBSERVER}.retainedView && !root.showDataBroadcast"
        )
    )?);
    browser(app, engine, "window.nagameSocketReady === true")?;
    browser(
        app,
        engine,
        "window.nagamePresentation().videoRect === null",
    )?;
    assert!(evaluate(
        engine,
        "!dataBroadcast.view.videoRect && video.width === videoPicture.width && video.height === videoPicture.height"
    )?);
    check_overlays(app, engine)?;

    // Wait past the activation interval: remote Back must not reopen the document.
    let deadline = Instant::now() + Duration::from_millis(2300);
    while Instant::now() < deadline {
        app.process_events();
        assert!(evaluate(engine, "!root.showDataBroadcast")?);
        thread::sleep(Duration::from_millis(10));
    }
    ffi::clickRootKey(engine.pin_mut(), &QString::from("Down"))?;
    wait_for(app, engine, "playerControls.activeFocus")?;
    click_remote(app, engine, "remoteDataButton")?;
    wait_for(
        app,
        engine,
        "root.showDataBroadcast && dataBroadcast.view.presentation === DataBroadcastView.Presenting && inputContext.videoFocused",
    )?;
    assert!(evaluate(
        engine,
        &format!("dataBroadcast.view === {OBSERVER}.retainedView")
    )?);
    browser(app, engine, "window.nagameSocketReady === true")?;
    check_video_window(app, engine)?;
    click_remote(app, engine, "remoteKey_Backspace")?;
    wait_for(
        app,
        engine,
        "!root.showDataBroadcast && inputContext.navigationEnabled",
    )?;
    // Esc in standby preserves the retained browser and receiver. Only the
    // feature setting, input lifetime or an error should release this view.
    evaluate(engine, "surface.forceActiveFocus(); true")?;
    ffi::clickRootKey(engine.pin_mut(), &QString::from("Escape"))?;
    assert!(evaluate(
        engine,
        &format!(
            "root.dataBroadcastSessionOpen && dataBroadcast.view === {OBSERVER}.retainedView && inputContext.navigationEnabled && player.data_broadcast_receiving()"
        )
    )?);
    browser(app, engine, "window.nagameSocketReady === true")?;
    evaluate(engine, "player.configure_data_broadcast(false); true")?;
    wait_for(
        app,
        engine,
        "!root.dataBroadcastSessionOpen && dataBroadcast.view === null && inputContext.navigationEnabled",
    )?;
    evaluate(engine, "player.configure_data_broadcast(true); true")?;
    evaluate(
        engine,
        "captions.active = false; danmaku.active = false; true",
    )?;
    evaluate(
        engine,
        &format!(
            "(() => {{ const observer = {OBSERVER}; observer.objectName = ''; observer.destroy(); return true; }})()"
        ),
    )?;
    server.finish()
}

/// Real PAT/PMT + carousel fixture, supplied by the paced TS HTTP server.
pub(super) fn run_entry(
    app: &QGuiApplication,
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
    automatic: bool,
) -> TestResult {
    if !automatic {
        // The manual-start PMT must leave WebEngine absent until the first d.
        let deadline = Instant::now() + Duration::from_millis(500);
        while Instant::now() < deadline {
            app.process_events();
            assert!(evaluate(engine, "dataBroadcast.view === null")?);
            thread::sleep(Duration::from_millis(10));
        }
        click_remote(app, engine, "remoteDataButton")?;
    }
    wait_for(
        app,
        engine,
        "dataBroadcast.view !== null && dataBroadcast.view.browserReady && root.showDataBroadcast",
    )?;
    let observer = r#"import QtQuick; Item { objectName: "bmlObserver"; property bool done: false; property bool passed: false; property var retainedView; }"#;
    evaluate(
        engine,
        &format!(
            "Qt.createQmlObject({}, root.contentItem).retainedView = dataBroadcast.view; true",
            serde_json::to_string(observer)?
        ),
    )?;
    // A directly encoded one-file module uses the module URI itself.
    wait_document(app, engine, "/40/0000")?;
    assert!(
        evaluate(
            engine,
            &format!("dataBroadcast.view === {OBSERVER}.retainedView")
        )?,
        "initial BML view was replaced"
    );
    // Neither automatic startup nor the first manual-start d may inject a
    // deferred toggle after the adapter's compatibility interval.
    let deadline = Instant::now() + Duration::from_millis(1300);
    while Instant::now() < deadline {
        app.process_events();
        assert!(evaluate(
            engine,
            "dataBroadcast.view.usedKeyGroups.length === 0"
        )?);
        thread::sleep(Duration::from_millis(10));
    }
    assert!(evaluate(
        engine,
        "inputContext.navigationEnabled && !dataBroadcast.view.activeFocus"
    )?);
    click_remote(app, engine, "remoteDataButton")?;
    wait_for(
        app,
        engine,
        "dataBroadcast.view.usedKeyGroups.includes('basic') && dataBroadcast.view.usedKeyGroups.includes('data-button') && dataBroadcast.view.usedKeyGroups.includes('numeric-tuning')",
    )?;
    click_remote(app, engine, "remoteKey_r")?;
    wait_for(
        app,
        engine,
        "!dataBroadcast.view.usedKeyGroups.includes('basic') && !dataBroadcast.view.usedKeyGroups.includes('data-button') && dataBroadcast.view.usedKeyGroups.includes('numeric-tuning')",
    )?;
    ffi::clickRootKey(engine.pin_mut(), &QString::from("G"))?;
    wait_for(app, engine, "root.showGuide")?;
    assert!(
        evaluate(
            engine,
            &format!("dataBroadcast.view === {OBSERVER}.retainedView")
        )?,
        "guide replaced BML view"
    );
    ffi::clickRootKey(engine.pin_mut(), &QString::from("Escape"))?;
    wait_for(app, engine, "!root.showGuide")?;
    assert!(
        evaluate(
            engine,
            &format!("dataBroadcast.view === {OBSERVER}.retainedView")
        )?,
        "closing guide replaced BML view"
    );
    click_remote(app, engine, "remoteDataButton")?;
    wait_for(
        app,
        engine,
        "dataBroadcast.view.usedKeyGroups.length === 0 && root.showDataBroadcast",
    )?;
    assert!(evaluate(
        engine,
        &format!("dataBroadcast.view === {OBSERVER}.retainedView")
    )?);
    // A BML timer changes the input mask after d has closed the menu.
    wait_for(
        app,
        engine,
        "dataBroadcast.view.usedKeyGroups.includes('numeric-tuning') && !dataBroadcast.view.usedKeyGroups.includes('basic') && root.showDataBroadcast",
    )?;
    evaluate(engine, "surface.forceActiveFocus(); true")?;
    ffi::clickRootKey(engine.pin_mut(), &QString::from("Escape"))?;
    let deadline = Instant::now() + Duration::from_millis(500);
    while Instant::now() < deadline {
        app.process_events();
        assert!(evaluate(
            engine,
            &format!(
                "dataBroadcast.view === {OBSERVER}.retainedView && player.data_broadcast_requested && player.data_broadcast_receiving()"
            )
        )?);
        thread::sleep(Duration::from_millis(10));
    }
    evaluate(
        engine,
        &format!(
            "(() => {{ const observer = {OBSERVER}; observer.objectName = ''; observer.destroy(); return true; }})()"
        ),
    )?;
    Ok(())
}
