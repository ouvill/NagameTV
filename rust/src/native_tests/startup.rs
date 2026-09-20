//! Load the production Main.qml, Player and video item using validated hardware.
use super::bridge::ffi;
use crate::{features, playback, player, settings};
use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl};
use gstreamer::glib;
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

pub(super) type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
static UI_WARNINGS: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn record_qt(level: u8, category: &str, message: &str) {
    if level >= 2 {
        eprintln!("Qt [{category}]: {message}");
    }
    if level >= 2 && (message.contains("qrc:/") || category.starts_with("qt.qml")) {
        UI_WARNINGS.lock().unwrap().push(message.into());
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
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(std::io::Error::other)?;
        let programs = serde_json::json!([{
            "id":101, "eventId":7, "networkId":10, "serviceId":2,
            "startAt": now.as_secs().saturating_sub(60) * 1000, "duration":3_600_000,
            "name":"Metadata program", "description":"Program overview", "isFree":true,
            "genres":[{"lv1":3,"lv2":0}],
            "extended":{"番組内容":"詳しい番組内容", "出演者":"出演者テスト", "スタッフ":"スタッフテスト"},
            "video":{"type":"mpeg2", "resolution":"1080i"},
            "audios":[{"componentTag":16,"componentType":3,"isMain":true,"langs":["jpn"],"samplingRate":48000}],
            "series":{"name":"Test series","episode":3,"lastEpisode":12}
        }]).to_string();
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
                    r#"[{"id":1,"networkId":10,"serviceId":1,"name":"First TV","type":1,"channel":{"type":"GR"}},
                        {"id":2,"networkId":10,"serviceId":2,"name":"Saved TV","type":1,"channel":{"type":"GR"}}]"#
                } else if header.starts_with(b"GET /api/programs ") {
                    &programs
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

pub(super) fn evaluate(
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
    source: &str,
) -> TestResult<bool> {
    ffi::evaluate_root(engine.pin_mut(), &QString::from(source))?
        .value::<bool>()
        .ok_or_else(|| format!("Expected a boolean: {source}").into())
}

pub(super) fn wait_for(
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
        "root.showProgram = false; root.width = 1280; root.height = 720; commentBounds.aspectRatio = 16/9; danmaku.active = true; true",
    )?;
    wait_for(app, engine, "danmaku.item !== null")?;
    assert!(evaluate(
        engine,
        r#"
        player.configure_danmaku(true, 36, player.comment_opacity, player.comment_speed);
        danmaku.item.replayReady = false;
        danmaku.item.playbackClock = null;
        danmaku.item.paused = true;
        danmaku.item.controller.load_timeline(JSON.stringify([
            {time: 0, text: "resize flow"}, {time: 0.5, text: "resize fixed", type: "top"}
        ]));
        danmaku.item.controller.seek(1);
        danmaku.item.activeCount === 2
    "#
    )?);
    // Base size is 36 px at a 720 px picture height. Sidebars, black bars and
    // aspect ratios must use the fitted picture, not the enclosing window.
    for (open, width, height, aspect, expected_font_size) in [
        (false, 1280, 720, 16. / 9., 36),
        (true, 1280, 720, 16. / 9., 25),
        (false, 1280, 900, 16. / 9., 36),
        (false, 1280, 720, 4. / 3., 36),
        (false, 1280, 720, 2.4, 27),
        (false, 1920, 1080, 16. / 9., 54),
        (false, 1920, 1080, 4. / 3., 54),
        (true, 1100, 720, 16. / 9., 21),
        (false, 1200, 720, 16. / 9., 34),
    ] {
        assert!(evaluate(
            engine,
            &format!(
                r#"
            (function() {{
                const before = Array.from(danmaku.item.visuals.values());
                root.showProgram = {open}; root.width = {width}; root.height = {height};
                commentBounds.aspectRatio = {aspect};
                const after = Array.from(danmaku.item.visuals.values());
                return after.length === 2 && before.every((entry, i) => entry === after[i])
                    && after[1].x === (danmaku.width - after[1].width) / 2;
            }})()
        "#
            )
        )?);
        // QQuickWindow can publish its new size before resizing its content.
        wait_for(
            app,
            engine,
            &format!(
                r#"
                danmaku.item.activeCount === 2 && danmaku.item.visualCount === 2
                    && Array.from(danmaku.item.visuals.values()).every(entry =>
                        entry.font.pixelSize === {expected_font_size}
                        && (entry.placement !== DanmakuOverlay.Top
                            || entry.x === (danmaku.width - entry.width) / 2))
                "#
            ),
        )?;
    }
    for size in [72, 14, 36] {
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
                    && after[1].font.pixelSize === Math.round({size} * danmaku.height / 720)
                    && after[1].width !== oldWidth;
            }})()
        "#
            )
        )?);
    }
    assert!(evaluate(
        engine,
        r#"
        player.configure_comment_presentation("pop", "random");
        root.showProgram = true; root.sidebarPage = ProgramSidebar.Playback;
        commentBounds.aspectRatio = 4/3;
        danmaku.item.controller.seek(2);
        danmaku.width === Math.floor(Math.min(video.width, video.height * 4/3))
            && danmaku.x === Math.floor((video.width - danmaku.width)/2)
            && danmaku.item.activeCount === 2
            && Array.from(danmaku.item.visuals.values()).every(e => Math.abs(e.rotation) <= 20)
    "#
    )?);
    wait_for(
        app,
        engine,
        "sidebar.item !== null && sidebar.item.page === ProgramSidebar.Playback",
    )?;
    // The distribution UI must keep controls when comments are turned off too;
    // exercise the real Player binding, not only a standalone sidebar fixture.
    assert!(evaluate(
        engine,
        "player.configure_danmaku(false, player.comment_font_size, player.comment_opacity, player.comment_speed); sidebar.item.page === ProgramSidebar.Playback && !sidebar.item.danmakuEnabled",
    )?);
    assert!(evaluate(
        engine,
        "sidebar.item.statsRequested(true); root.showStats && sidebar.item.statsVisible",
    )?);
    assert!(evaluate(
        engine,
        "sidebar.item.statsRequested(false); !root.showStats && !sidebar.item.statsVisible",
    )?);
    assert!(evaluate(
        engine,
        "sidebar.item.pageRequested(ProgramSidebar.Program); sidebar.item.commentProgramTitle === player.comment_program_title && sidebar.item.commentStatus === player.comment_status",
    )?);
    assert!(evaluate(
        engine,
        "viewerActions.toggleSettings.trigger(); root.showProgram && root.sidebarPage === ProgramSidebar.Playback",
    )?);
    evaluate(engine, "sidebar.item.timeshiftSettingsRequested(); true")?;
    wait_for(
        app,
        engine,
        "settings.opened && settings.page === SettingsPanel.Timeshift",
    )?;
    evaluate(engine, "settings.close(); true")?;
    evaluate(
        engine,
        "player.configure_comment_presentation('scroll','sequential'); danmaku.active = false; commentBounds.aspectRatio = Qt.binding(() => player.video_aspect_ratio); true",
    )?;
    Ok(())
}

fn check_screen_navigation(
    app: &QGuiApplication,
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
) -> TestResult {
    let review =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../build/navigation-review");
    std::fs::create_dir_all(&review)?;
    let capture = |engine: &mut cxx::UniquePtr<QQmlApplicationEngine>, name: &str| -> TestResult {
        let image = ffi::grabRoot(engine.pin_mut())?;
        let default_quality = -1;
        let png_compression_percent = 60;
        if !player::ffi::save_screenshot_image(
            &image,
            &QString::from(review.join(name).to_string_lossy().as_ref()),
            &QString::from("png"),
            default_quality,
            png_compression_percent,
        ) {
            return Err("could not save navigation review image".into());
        }
        Ok(())
    };
    for (width, height) in [(900, 560), (1440, 900)] {
        evaluate(
            engine,
            &format!(
                "root.width = {width}; root.height = {height}; root.showProgram = true; root.sidebarPage = ProgramSidebar.Playback; true"
            ),
        )?;
        wait_for(
            app,
            engine,
            "sidebar.open && sidebar.reveal === 1 && surface.width === root.width - sidebar.width",
        )?;
        capture(engine, &format!("viewing-{width}.png"))?;
        evaluate(
            engine,
            "modeNavigation.modeRequested(ModeNavigation.Guide); true",
        )?;
        wait_for(
            app,
            engine,
            "guideLoader.item !== null && guideLoader.item.width === root.width && !sidebar.visible && !sidebar.enabled",
        )?;
        capture(engine, &format!("guide-{width}.png"))?;
        evaluate(
            engine,
            "guideLoader.item.modeRequested(ModeNavigation.Settings); true",
        )?;
        wait_for(app, engine, "settings.opened")?;
        capture(engine, &format!("settings-{width}.png"))?;
        assert!(evaluate(
            engine,
            r#"
            function find(item, name) {
                if (item.objectName === name) return item;
                for (const child of item.children || []) { const result = find(child, name); if (result) return result; }
                return null;
            }
            const guideNavigation = find(guideLoader.item, 'guideModeNavigation');
            const settingsNavigation = find(settings.contentItem, 'settingsModeNavigation');
            const windowEdgeMargin = 18;
            [modeNavigation, guideNavigation, settingsNavigation].every(navigation => {
                if (!navigation) return false;
                const corner = navigation.mapToItem(root.contentItem, navigation.width, 0);
                return corner.x === root.width - windowEdgeMargin && corner.y === windowEdgeMargin
                    && navigation.width === modeNavigation.width && navigation.height === modeNavigation.height;
            })
        "#,
        )?);
        // Choosing Guide from settings must reveal the existing guide, not toggle it off.
        evaluate(engine, "settings.modeRequested(ModeNavigation.Guide); true")?;
        wait_for(
            app,
            engine,
            "!settings.visible && root.guideVisible && !sidebar.visible && guideLoader.item.width === root.width",
        )?;
        evaluate(engine, "guideLoader.item.closeRequested(); true")?;
        wait_for(
            app,
            engine,
            "!root.guideVisible && root.showProgram && sidebar.open && sidebar.reveal === 1 && surface.width === root.width - sidebar.width",
        )?;
        // Direct backend requests and rapid shortcut reversals share the same layout rules.
        evaluate(engine, "player.guide_open(true); true")?;
        wait_for(app, engine, "guideLoader.item !== null && !sidebar.visible")?;
        evaluate(
            engine,
            "viewerActions.toggleGuide.trigger(); viewerActions.toggleGuide.trigger(); true",
        )?;
        wait_for(
            app,
            engine,
            "guideLoader.item !== null && guideLoader.item.width === root.width && !sidebar.visible",
        )?;
        evaluate(engine, "root.requestMode(ModeNavigation.Settings); true")?;
        wait_for(app, engine, "settings.opened")?;
        evaluate(engine, "settings.modeRequested(ModeNavigation.Live); true")?;
        wait_for(
            app,
            engine,
            "!settings.visible && !root.showGuide && root.showProgram && sidebar.visible && sidebar.reveal === 1",
        )?;
    }
    evaluate(engine, "root.requestMode(ModeNavigation.Settings); true")?;
    wait_for(app, engine, "settings.opened")?;
    evaluate(
        engine,
        "settings.modeRequested(ModeNavigation.Recording); true",
    )?;
    let picker =
        "Array.from(recordingInput.data).find(item => item.objectName === 'recordingPicker')";
    wait_for(app, engine, &format!("{picker}.visible"))?;
    evaluate(engine, &format!("{picker}.reject(); true"))?;
    wait_for(
        app,
        engine,
        &format!("!{picker}.visible && settings.opened && !root.showGuide"),
    )?;
    // Settings opened from viewing can also enter the guide for the first time.
    evaluate(engine, "settings.modeRequested(ModeNavigation.Guide); true")?;
    wait_for(
        app,
        engine,
        "!settings.visible && root.guideVisible && !sidebar.visible",
    )?;
    evaluate(
        engine,
        "viewerActions.dismissTopmost.trigger(); root.showProgram = false; true",
    )?;
    wait_for(app, engine, "!sidebar.active")?;
    Ok(())
}

enum WindowCheck {
    Startup,
    PidChange,
    Timeshift,
    Screenshots,
    VideoProcessing,
    RecordingAudit(std::path::PathBuf),
    RecordingProbe(std::path::PathBuf),
}

fn window(
    app: &QGuiApplication,
    preferences: &settings::Preferences,
    check: WindowCheck,
) -> TestResult {
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
    match check {
        WindowCheck::Startup => {}
        WindowCheck::VideoProcessing => {
            let result = super::video_processing::run(app, &mut engine);
            evaluate(&mut engine, "player.stop(); root.close(); true")?;
            return result;
        }
        WindowCheck::Screenshots => {
            let result = super::screenshots::run(app, &mut engine);
            evaluate(&mut engine, "player.stop(); root.close(); true")?;
            return result;
        }
        WindowCheck::Timeshift => {
            let result = super::timeshift::run(app, &mut engine);
            assert!(evaluate(&mut engine, "root.close(); root.closing")?);
            app.process_events();
            return result;
        }
        WindowCheck::PidChange => {
            // Targeted regression check, also included in the startup suite.
            let result = check_recording_recovery(app, &mut engine, "recording-pid-change.ts");
            assert!(evaluate(&mut engine, "root.close(); root.closing")?);
            app.process_events();
            return result;
        }
        WindowCheck::RecordingAudit(path) => {
            let result = super::recording_audit::run(app, &mut engine, &path);
            assert!(evaluate(&mut engine, "root.close(); root.closing")?);
            app.process_events();
            return result;
        }
        WindowCheck::RecordingProbe(path) => {
            let result = super::recording_audit::probe(app, &mut engine, &path);
            evaluate(&mut engine, "player.stop(); root.close(); true")?;
            return result;
        }
    }
    assert!(evaluate(&mut engine, "!screenshot.canCapture")?);
    assert!(evaluate(
        &mut engine,
        "modeNavigation.mode === ModeNavigation.Live"
    )?);
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
            std::env::var("NAGAMETV_AUTOPLAY").ok().as_deref(),
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
        check_shortcuts(app, &mut engine)?;
        check_screen_navigation(app, &mut engine)?;
        // Every way of changing guide visibility must run the same synchronization.
        evaluate(&mut engine, "viewerActions.toggleGuide.trigger(); true")?;
        wait_for(app, &mut engine, "root.guideVisible && root.showGuide")?;
        evaluate(&mut engine, "viewerActions.dismissTopmost.trigger(); true")?;
        assert!(evaluate(
            &mut engine,
            "!root.guideVisible && !root.showGuide"
        )?);
        evaluate(&mut engine, "viewerActions.toggleGuide.trigger(); true")?;
        wait_for(app, &mut engine, "guideLoader.item !== null")?;
        wait_for(
            app,
            &mut engine,
            "JSON.parse(guideLoader.item.programsJson).some(column => column.programs.some(program => program.id === 101))",
        )?;
        // A user can select this card only after channel visibility arrives.
        // Its initial publication clears selectedProgram; selecting earlier
        // races that update and can close the details popup during this wait.
        wait_for(
            app,
            &mut engine,
            "guideLoader.item.visibilityJson !== 'null' && guideLoader.item.visibleRows.some(row => row.index === player.selected)",
        )?;
        evaluate(
            &mut engine,
            r#"
            const guide = guideLoader.item;
            guide.selectedChannel = 'Saved TV';
            const column = JSON.parse(guide.programsJson).find(column => column.programs.some(program => program.id === 101));
            guide.selectedProgram = column.programs.find(program => program.id === 101);
            true
        "#,
        )?;
        wait_for(
            app,
            &mut engine,
            r#"
            const loader = Array.from(guideLoader.item.data).find(item => item.objectName === 'scheduledDetailsLoader');
            Boolean(loader && loader.item && loader.item.opened)
        "#,
        )?;
        assert!(evaluate(
            &mut engine,
            r#"
            function find(item, name) {
                if (item.objectName === name) return item;
                for (const child of item.children || []) { const result = find(child, name); if (result) return result; }
                return null;
            }
            const loader = Array.from(guideLoader.item.data).find(item => item.objectName === 'scheduledDetailsLoader');
            const metadata = find(loader.item.contentItem, 'programMetadata');
            const valid = metadata.sections.some(section => section.heading === '出演者' && section.text === '出演者テスト')
                && metadata.broadcastFields.some(field => field.text === 'HD · 1080i / MPEG-2')
                && metadata.broadcastFields.some(field => field.text.includes('48 kHz'))
                && metadata.broadcastFields.some(field => field.text.includes('Test series'));
            loader.item.close();
            valid
        "#
        )?);
        wait_for(
            app,
            &mut engine,
            "guideLoader.item.selectedProgram === null",
        )?;
        evaluate(
            &mut engine,
            "guideLoader.item.modeRequested(ModeNavigation.Settings); true",
        )?;
        wait_for(app, &mut engine, "settings.opened && root.guideVisible")?;
        evaluate(&mut engine, "settings.close(); true")?;
        wait_for(app, &mut engine, "!settings.visible")?;
        evaluate(
            &mut engine,
            "guideLoader.item.modeRequested(ModeNavigation.Recording); true",
        )?;
        let guide_picker =
            "Array.from(recordingInput.data).find(item => item.objectName === 'recordingPicker')";
        wait_for(app, &mut engine, &format!("{guide_picker}.visible"))?;
        evaluate(&mut engine, &format!("{guide_picker}.reject(); true"))?;
        wait_for(
            app,
            &mut engine,
            &format!("!{guide_picker}.visible && root.guideVisible"),
        )?;
        evaluate(
            &mut engine,
            "guideLoader.item.modeRequested(ModeNavigation.Live); true",
        )?;
        wait_for(app, &mut engine, "!root.guideVisible && !root.showGuide")?;
        evaluate(
            &mut engine,
            "viewerActions.toggleGuide.trigger(); root.chooseConnectedChannel(); true",
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
        // Exercise the production settings page and live listener lifecycle.
        let port = TcpListener::bind("127.0.0.1:0")?.local_addr()?.port();
        assert!(evaluate(
            &mut engine,
            &format!("player.configure_remote(true, '127.0.0.1', {port})")
        )?);
        wait_for(app, &mut engine, "player.remote_status === 'listening'")?;
        evaluate(
            &mut engine,
            "settings.open(); settings.page = SettingsPanel.Remote; true",
        )?;
        wait_for(app, &mut engine, "settings.opened")?;
        assert!(evaluate(
            &mut engine,
            &format!(
                "player.remote_enabled && player.remote_port === {port} && player.remote_endpoints === '127.0.0.1:{port}'"
            )
        )?);
        assert!(evaluate(
            &mut engine,
            "player.configure_remote(false, '0.0.0.0', 50051)"
        )?);
        wait_for(app, &mut engine, "player.remote_status === 'disabled'")?;
        TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))?;
        evaluate(&mut engine, "settings.close(); true")?;
        if !autoplay {
            check_recording(app, &mut engine)?;
            evaluate(
                &mut engine,
                "modeNavigation.modeRequested(ModeNavigation.Live); true",
            )?;
            wait_for(
                app,
                &mut engine,
                "!player.recording && !player.connecting && player.playback_error.length > 0 && modeNavigation.mode === ModeNavigation.Live",
            )?;
        }
    } else {
        wait_for(app, &mut engine, "setup.opened")?;
        assert!(evaluate(
            &mut engine,
            "root.setupRequired && !player.server_configured && !player.loading && !player.server.length && !root.channelRows.length"
        )?);
        check_danmaku_layout(app, &mut engine)?;
        // Comments are enabled here with an empty catalog, so shortcut coverage
        // does not start external comment/activity requests.
        evaluate(&mut engine, "setup.close(); true")?;
        wait_for(app, &mut engine, "!setup.visible")?;
        check_shortcuts(app, &mut engine)?;
        evaluate(&mut engine, "setup.open(); true")?;
        wait_for(app, &mut engine, "setup.opened")?;
        check_recording(app, &mut engine)?;
        check_recording_recovery(app, &mut engine, "recording-clock-reset.ts")?;
        check_recording_recovery(app, &mut engine, "recording-pid-change.ts")?;
        super::timeshift::run(app, &mut engine)?;
    }
    assert!(evaluate(&mut engine, "root.close(); root.closing")?);
    app.process_events();
    drop(engine);
    app.process_events();
    let warnings = UI_WARNINGS.lock().unwrap();
    assert!(warnings.is_empty(), "UI warnings: {warnings:?}");
    Ok(())
}

fn check_recording(
    app: &QGuiApplication,
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
) -> TestResult {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("録画 #100%.ts");
    std::fs::write(
        &path,
        include_bytes!("../../../tests/fixtures/recording.ts"),
    )?;
    let url = url::Url::from_file_path(&path)
        .map_err(|_| "fixture URL")?
        .to_string();
    let quoted_url = serde_json::to_string(&url)?;
    // Exercise the real picker, not only drag-and-drop. Keep its options intact
    // so the test also covers the production startup dialog policy.
    let picker =
        "Array.from(recordingInput.data).find(item => item.objectName === 'recordingPicker')";
    evaluate(
        engine,
        &format!(
            "{picker}.selectedFile = {quoted_url}; modeNavigation.modeRequested(ModeNavigation.Recording); true"
        ),
    )?;
    wait_for(app, engine, &format!("{picker}.visible"))?;
    evaluate(engine, &format!("{picker}.reject(); true"))?;
    wait_for(app, engine, &format!("!{picker}.visible"))?;
    assert!(evaluate(
        engine,
        "!player.recording_loading && !player.recording && !player.playing && modeNavigation.mode === ModeNavigation.Live"
    )?);
    assert!(ffi::drop_file_on_root(
        engine.pin_mut(),
        &QString::from(&url),
        &cxx_qt_lib::QPoint::new(20, 200)
    ));
    wait_for(
        app,
        engine,
        "player.playing && JSON.parse(player.video_stats()).rendered > 0 && JSON.parse(player.audio_tracks()).length > 0",
    )?;
    assert!(evaluate(
        engine,
        "player.recording && player.recording_name === '録画 #100%.ts' && modeNavigation.mode === ModeNavigation.Recording && !setup.visible && player.current_program_data === 'null' && !player.comment_post_available && !danmaku.active && player.subtitles_active"
    )?);
    wait_for(app, engine, "!overlayVisibility.pinned")?;
    assert!(evaluate(
        engine,
        "surface.activity(); overlayVisibility.controlsVisible"
    )?);
    assert!(evaluate(
        engine,
        "surface.pointerExited(); !overlayVisibility.controlsVisible && !bottomPanel.enabled"
    )?);
    assert!(evaluate(
        engine,
        "surface.activity(); overlayVisibility.controlsVisible && bottomPanel.enabled"
    )?);
    // Capture a real playing video through Main.qml, then confirm the render
    // counter advances. Accepted saves must leave the transport actions enabled.
    let capture_directory =
        url::Url::from_directory_path(directory.path()).map_err(|_| "capture directory URL")?;
    let quoted_directory = serde_json::to_string(capture_directory.as_str())?;
    assert!(evaluate(
        engine,
        &format!("player.configure_screenshot_directory({quoted_directory})")
    )?);
    let rendered_before: u64 = ffi::evaluate_root(
        engine.pin_mut(),
        &QString::from("String(JSON.parse(player.video_stats()).rendered)"),
    )?
    .value::<QString>()
    .ok_or("rendered counter")?
    .to_string()
    .parse()?;
    assert!(evaluate(
        engine,
        "screenshot.canCapture && (screenshot.capture(), screenshot.busy && viewerActions.enabled)"
    )?);
    wait_for(app, engine, "!screenshot.busy")?;
    assert!(evaluate(
        engine,
        "player.screenshot_error.length === 0 && player.playing && viewerActions.enabled && screenshotNotice.visible && screenshotNotice.kind === ScreenshotNotice.Saved"
    )?);
    assert!(
        std::fs::read_dir(directory.path())?
            .filter_map(Result::ok)
            .any(|entry| entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "png"))
    );
    wait_for(
        app,
        engine,
        &format!("JSON.parse(player.video_stats()).rendered > {rendered_before}"),
    )?;
    // Invalid input and a multiple-file drop must leave the current stream intact.
    assert!(evaluate(
        engine,
        "!player.open_recording('https://example.invalid/recording.ts') && player.playing && player.recording && player.file_error.length > 0"
    )?);
    assert!(!ffi::drop_files_on_root(
        engine.pin_mut(),
        &[url.to_string(), url.to_string()],
        &cxx_qt_lib::QPoint::new(20, 200),
    ));
    assert!(evaluate(engine, "player.playing")?);
    evaluate(engine, "player.stop(); true")?;
    assert!(evaluate(
        engine,
        "!player.playing && !player.connecting && !player.subtitles_active && player.recording"
    )?);
    evaluate(
        engine,
        "modeNavigation.modeRequested(ModeNavigation.Recording); true",
    )?;
    wait_for(app, engine, &format!("{picker}.visible"))?;
    evaluate(engine, &format!("{picker}.accept(); true"))?;
    wait_for(
        app,
        engine,
        "player.recording && player.playing && JSON.parse(player.video_stats()).rendered > 0",
    )?;
    evaluate(engine, "player.stop(); true")?;
    // Replay through the ordinary Play button, then reach a normal file EOF.
    evaluate(engine, "player.play(); true")?;
    wait_for(
        app,
        engine,
        "player.playing && JSON.parse(player.video_stats()).rendered > 0",
    )?;
    wait_for(
        app,
        engine,
        "!player.playing && !player.connecting && player.status === 'Playback finished'",
    )?;
    assert!(evaluate(
        engine,
        "!player.playback_error.length && player.ended && player.media_active && player.recording && player.seekable"
    )?);
    evaluate(engine, "player.play(); true")?;
    wait_for(
        app,
        engine,
        "player.playing && !player.seeking && !player.ended",
    )?;
    evaluate(engine, "player.stop(); true")?;
    std::fs::remove_file(&path)?;
    evaluate(engine, "player.play(); true")?;
    wait_for(
        app,
        engine,
        "!player.recording_loading && !player.playing && player.playback_error.length > 0 && player.recording",
    )?;
    std::fs::write(
        &path,
        include_bytes!("../../../tests/fixtures/recording.ts"),
    )?;
    assert!(evaluate(
        engine,
        &format!("recordingInput.openUrl({quoted_url})")
    )?);
    // Cancellation during connection must not resurrect playback on a late message.
    evaluate(engine, "player.stop(); true")?;
    app.process_events();
    assert!(evaluate(
        engine,
        "!player.connecting && !player.playing && !player.subtitles_active"
    )?);
    // Long synthetic TS: use production playbin3, GL sink, audio and Main.qml.
    std::fs::write(
        &path,
        include_bytes!("../../../tests/fixtures/recording-seek.ts"),
    )?;
    assert!(evaluate(
        engine,
        &format!("recordingInput.openUrl({quoted_url})")
    )?);
    wait_for(
        app,
        engine,
        "player.playing && player.seekable && player.duration_ms > 59000",
    )?;
    wait_for(
        app,
        engine,
        "JSON.parse(player.current_program_data)?.eventId === 1",
    )?;
    assert!(evaluate(
        engine,
        "JSON.parse(player.current_program_data).name === '日本語 1' && JSON.parse(player.current_program_data).station === '日本語 TV'"
    )?);
    evaluate(
        engine,
        "root.requestActivate(); surface.forceActiveFocus(); true",
    )?;
    wait_for(
        app,
        engine,
        "root.active && inputContext.accepts(InputContext.Playback)",
    )?;
    ffi::clickRootKey(engine.pin_mut(), &QString::from("Space"))?;
    wait_for(
        app,
        engine,
        "player.paused && !player.playing && player.media_active",
    )?;
    for (target, event) in [(45000, 2), (10000, 1), (35000, 2)] {
        assert!(evaluate(
            engine,
            &format!("player.seek_to({target}) && player.seeking && player.paused")
        )?);
        wait_for(
            app,
            engine,
            &format!(
                "!player.seeking && player.paused && Math.abs(player.position_ms - {target}) < 1500"
            ),
        )?;
        wait_for(
            app,
            engine,
            &format!("JSON.parse(player.current_program_data)?.eventId === {event}"),
        )?;
    }
    assert!(evaluate(
        engine,
        "!player.seek_to(NaN) && player.paused && player.transport_error.length > 0"
    )?);
    assert!(evaluate(
        engine,
        "player.seek_to(20000) && player.seek_to(40000) && player.seek_to(15000)"
    )?);
    wait_for(
        app,
        engine,
        "!player.seeking && player.paused && Math.abs(player.position_ms - 15000) < 1500 && !player.transport_error.length",
    )?;
    evaluate(engine, "viewerActions.playbackToggle.trigger(); true")?;
    wait_for(app, engine, "player.playing && !player.paused")?;
    assert!(evaluate(engine, "player.seek_to(59000)")?);
    wait_for(app, engine, "player.ended && !player.playing")?;
    assert!(evaluate(engine, "player.seek_to(5000)")?);
    wait_for(
        app,
        engine,
        "player.playing && !player.seeking && !player.ended && player.position_ms < 8000",
    )?;
    evaluate(engine, "player.stop(); true")?;
    assert!(evaluate(
        engine,
        "!player.media_active && !player.seekable && player.position_ms < 0 && !player.subtitles_active"
    )?);
    Ok(())
}

fn check_shortcuts(
    app: &QGuiApplication,
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
) -> TestResult {
    evaluate(
        engine,
        "root.showChannels = false; root.requestActivate(); surface.forceActiveFocus(); true",
    )?;
    wait_for(app, engine, "root.active && inputContext.navigationEnabled")?;
    for (key, condition) in [
        ("S", "root.showChannels"),
        ("G", "root.showGuide && root.showChannels"),
        ("Escape", "!root.showGuide && root.showChannels"),
        ("Escape", "!root.showGuide && !root.showChannels"),
        ("F11", "viewerActions.fullscreen"),
        ("F11", "!viewerActions.fullscreen"),
        ("S", "root.showChannels"),
        ("G", "root.showGuide && root.showChannels"),
    ] {
        ffi::clickRootKey(engine.pin_mut(), &QString::from(key))?;
        wait_for(app, engine, condition)?;
    }
    ffi::clickRootKey(engine.pin_mut(), &QString::from("C"))?;
    if evaluate(engine, "player.comments_enabled")? {
        wait_for(
            app,
            engine,
            "root.showCommentComposer && composer.visible && inputContext.editingText && !root.showChannels && !root.showGuide",
        )?;
        assert!(evaluate(engine, "player.comment_draft.length === 0")?);
        // Opening focuses the editor without inserting C. Later letters are text.
        ffi::clickRootKey(engine.pin_mut(), &QString::from("C"))?;
        ffi::clickRootKey(engine.pin_mut(), &QString::from("S"))?;
        assert!(evaluate(
            engine,
            "!root.showChannels && player.comment_draft.toLowerCase() === 'cs'"
        )?);
        evaluate(engine, "surface.forceActiveFocus(); true")?;
        wait_for(app, engine, "!inputContext.editingText")?;
        ffi::clickRootKey(engine.pin_mut(), &QString::from("C"))?;
        wait_for(app, engine, "composer.visible && inputContext.editingText")?;
        assert!(evaluate(
            engine,
            "player.comment_draft.toLowerCase() === 'cs'"
        )?);
        assert!(ffi::sendTestInputMethodCursor(&QString::from("にほんご")));
        assert!(ffi::sendTestInputMethod(
            &QString::default(),
            &QString::from("日本語")
        ));
        assert!(ffi::sendTestInputMethodCursor(&QString::default()));
        assert!(evaluate(
            engine,
            "!composer.composing && player.comment_draft.endsWith('日本語')"
        )?);
        // This fixture has no posting target. Clicking its disabled send button
        // must stay inside the production composer, preserving the draft/focus.
        ffi::clickRootItem(engine.pin_mut(), &QString::from("sendComment"))?;
        assert!(evaluate(
            engine,
            "composer.visible && inputContext.editingText && player.comment_draft.endsWith('日本語')"
        )?);
        ffi::clickRootKey(engine.pin_mut(), &QString::from("Escape"))?;
        assert!(evaluate(engine, "!root.showCommentComposer")?);
        for (key, condition) in [
            ("S", "root.showChannels"),
            ("S", "!root.showChannels"),
            ("G", "root.showGuide"),
            ("G", "!root.showGuide"),
            ("C", "composer.visible && inputContext.editingText"),
            ("Escape", "!root.showCommentComposer"),
        ] {
            assert!(ffi::forwardFocusKey(&QString::from(key))?);
            wait_for(app, engine, condition)?;
        }
    } else {
        assert!(evaluate(
            engine,
            "!root.showCommentComposer && root.showChannels && root.showGuide"
        )?);
        ffi::clickRootKey(engine.pin_mut(), &QString::from("Escape"))?;
        wait_for(app, engine, "!root.showGuide && root.showChannels")?;
        ffi::clickRootKey(engine.pin_mut(), &QString::from("Escape"))?;
        wait_for(app, engine, "!root.showChannels")?;
    }
    evaluate(
        engine,
        "player.edit_comment_draft(''); settings.open(); true",
    )?;
    wait_for(app, engine, "settings.opened && inputContext.popupOpen")?;
    ffi::clickRootKey(engine.pin_mut(), &QString::from("Escape"))?;
    wait_for(app, engine, "!settings.visible && !inputContext.popupOpen")?;
    Ok(())
}

fn check_recording_recovery(
    app: &QGuiApplication,
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
    name: &str,
) -> TestResult {
    // Each fixture contains two halves of 75 frames. Allow a few rendering
    // drops and a polling gap at a counter reset, but never an entire half.
    const MIN_RENDERED_FRAMES: u64 = 140;
    const RECOVERY_TIMEOUT: Duration = Duration::from_secs(12);
    const SAMPLE_INTERVAL: Duration = Duration::from_millis(5);
    #[derive(Debug, serde::Deserialize)]
    struct Snapshot {
        rendered: Option<u64>,
        ended: bool,
        playing: bool,
        error: String,
    }

    eprintln!("Recording recovery: {name}");
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures")
        .join(name)
        .canonicalize()?;
    let url = url::Url::from_file_path(path).map_err(|_| "fixture URL")?;
    assert!(evaluate(
        engine,
        &format!(
            "recordingInput.openUrl({})",
            serde_json::to_string(url.as_str())?
        ),
    )?);
    wait_for(app, engine, "player.playing")?;
    let deadline = Instant::now() + RECOVERY_TIMEOUT;
    let mut previous = 0;
    let mut rendered = 0;
    let mut counter_resets = 0;
    loop {
        app.process_events();
        let state = ffi::evaluate_root(engine.pin_mut(), &QString::from(
            "JSON.stringify({status: player.status, error: player.playback_error, ended: player.ended, playing: player.playing, position: player.position_ms, rendered: JSON.parse(player.video_stats()).rendered, audio: JSON.parse(player.audio_tracks())})",
        ))?.value::<QString>().ok_or("missing recovery snapshot")?.to_string();
        let snapshot: Snapshot = serde_json::from_str(&state)?;
        if let Some(current) = snapshot.rendered {
            // GstBaseSink counters reset when replacement streams start. Sum
            // observed increments; the last sink snapshot is not a file total.
            rendered += if current < previous {
                counter_resets += 1;
                current
            } else {
                current - previous
            };
            previous = current;
        }
        if !snapshot.error.is_empty() || Instant::now() >= deadline {
            return Err(format!(
                "{name}: recovery failed; observed {rendered} frames, {counter_resets} counter resets; {state}"
            ).into());
        }
        if snapshot.ended && !snapshot.playing {
            if rendered < MIN_RENDERED_FRAMES {
                return Err(format!(
                    "{name}: incomplete playback; observed {rendered} frames, {counter_resets} counter resets; {state}"
                ).into());
            }
            eprintln!(
                "{name}: normal EOF; observed {rendered} frames, {counter_resets} counter resets; {state}"
            );
            break;
        }
        thread::sleep(SAMPLE_INTERVAL);
    }
    evaluate(engine, "player.stop(); true")?;
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
            command.env("NAGAMETV_AUTOPLAY", value);
        }
        None => {
            command.env_remove("NAGAMETV_AUTOPLAY");
        }
    }
    let status = command.status()?;
    if !status.success() {
        return Err(format!("startup process failed: {status}").into());
    }
    Ok(())
}

pub fn run_window() -> i32 {
    run_window_check(WindowCheck::Startup)
}

pub fn run_timeshift() -> i32 {
    run_window_check(WindowCheck::Timeshift)
}
pub fn run_video_processing() -> i32 {
    run_window_check(WindowCheck::VideoProcessing)
}

pub fn run_screenshots() -> i32 {
    run_window_check(WindowCheck::Screenshots)
}

pub fn run_pid_change() -> i32 {
    run_window_check(WindowCheck::PidChange)
}

pub fn run_recording_audit(path: std::path::PathBuf) -> i32 {
    run_window_check(WindowCheck::RecordingAudit(path))
}
pub fn run_recording_probe(path: std::path::PathBuf) -> i32 {
    run_window_check(WindowCheck::RecordingProbe(path))
}

fn run_window_check(check: WindowCheck) -> i32 {
    // SAFETY: This is a fresh subprocess, before Qt or worker initialization.
    #[cfg(target_os = "linux")]
    unsafe {
        crate::platform::configure_at_startup();
    }
    #[cfg(target_os = "linux")]
    let dialogs = unsafe { crate::platform::DialogSetup::prepare() };
    // Native GTK dialog warnings bypass Qt's message handler. Keep them visible
    // and fail this test if the GTK icon-surface regression returns.
    let gtk_log = glib::log_set_handler(
        Some("Gtk"),
        glib::LogLevels::LEVEL_WARNING | glib::LogLevels::LEVEL_CRITICAL,
        false,
        false,
        |domain, level, message| {
            glib::log_default_handler(domain, level, Some(message));
            UI_WARNINGS.lock().unwrap().push(format!("Gtk: {message}"));
        },
    );
    let result = (|| -> TestResult {
        features::PLAN
            .set(features::LaunchPlan::parse([])?)
            .map_err(|_| "plan already initialized")?;
        player::ffi::install_qt_logging(record_qt);
        cxx_qt::init_qml_module!("MinimalViewer");
        player::ffi::configure_qt_quick_open_gl();
        let app = QGuiApplication::new();
        assert!(!app.is_null());
        #[cfg(target_os = "linux")]
        dialogs.finish(&app);
        let preferences = settings::Loaded::open(settings::settings_path()?)?;
        window(&app, preferences.preferences(), check)
    })();
    glib::log_remove_handler(Some("Gtk"), gtk_log);
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
