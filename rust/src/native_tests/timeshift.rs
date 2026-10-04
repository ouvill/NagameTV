//! Product Main.qml and real output; the HTTP fixture only replaces the tuner.
use super::bridge::ffi;
use super::startup::{TestResult, evaluate, wait_for};
use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const ACCEPT_POLL: Duration = Duration::from_millis(10);
const NETWORK_TIMEOUT: Duration = Duration::from_secs(2);
const SEND_INTERVAL: Duration = Duration::from_millis(250);
// Small, regular deliveries expose live-edge sampling errors hidden by bursts.
const BROADCAST_SEND_INTERVAL: Duration = Duration::from_millis(20);
const FIXTURE_SECONDS: usize = 60;
const MIN_SEEKABLE_HISTORY: Duration = Duration::from_secs(3);
const PAUSE_RECEIVE_GROWTH: Duration = Duration::from_secs(1);
const SPEED_CATCH_UP_DELAY_MS: u64 = 3500;
const REWIND_TARGET: Duration = Duration::from_secs(1);
const REPEATED_SEEK_STEP: Duration = Duration::from_millis(500);
const SEEK_TOLERANCE: Duration = Duration::from_millis(500);
const LIVE_EDGE_TOLERANCE: Duration = Duration::from_millis(2500);
// Includes the fixture's 250 ms deliveries and the UI's sampled playhead. This
// catches startup lag that a subsequent manual return-to-live could still remove.
const STARTUP_LIVE_EDGE_TOLERANCE: Duration = Duration::from_millis(750);
const CUSTOM_LIVE_BUFFER_MS: i32 = 150;
const UPDATED_LIVE_BUFFER_MS: i32 = crate::settings::LiveBuffer::DEFAULT_MS;
const MEMORY_MIB: i32 = 16;
const FILESYSTEM_MIB: i32 = 64;
const RETENTION_MINUTES: i32 = 1;
const OBSERVER: &str =
    "root.contentItem.children.find(child => child.objectName === 'liveTimelineObserver')";
const RECOVERY_OBSERVATION: Duration = Duration::from_secs(4);
const RESIZE_HISTORY: Duration = Duration::from_secs(7);
const LIVE_METADATA_READY: &str = "player.program_status === 'available' && JSON.parse(player.current_program_data) !== null && JSON.parse(player.live_timeline).viewing.utc !== null";

#[derive(Clone, Copy)]
enum Traffic {
    Broadcast,
    CapacityPressure,
    DataBroadcast { automatic: bool },
}

struct Server {
    url: String,
    stop: Arc<AtomicBool>,
    streams: Arc<AtomicUsize>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new(traffic: Traffic) -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let url = format!("http://{}", listener.local_addr()?);
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let streams = Arc::new(AtomicUsize::new(0));
        let received_streams = streams.clone();
        let worker = thread::spawn(move || {
            let mut clients = Vec::new();
            while !stopped.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((socket, _)) => {
                        let stopped = stopped.clone();
                        let received_streams = received_streams.clone();
                        clients.push(thread::spawn(move || {
                            let _ = serve(socket, &stopped, traffic, &received_streams);
                        }));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(ACCEPT_POLL)
                    }
                    Err(_) => break,
                }
            }
            for client in clients {
                let _ = client.join();
            }
        });
        Ok(Self {
            url,
            stop,
            streams,
            worker: Some(worker),
        })
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
fn serve(
    mut socket: TcpStream,
    stopped: &AtomicBool,
    traffic: Traffic,
    streams: &AtomicUsize,
) -> std::io::Result<()> {
    const MAX_HEADER_BYTES: usize = 8 * 1024;
    socket.set_read_timeout(Some(NETWORK_TIMEOUT))?;
    socket.set_write_timeout(Some(NETWORK_TIMEOUT))?;
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") && header.len() < MAX_HEADER_BYTES {
        let mut byte = [0];
        socket.read_exact(&mut byte)?;
        header.extend(byte);
    }
    if header.starts_with(b"GET /api/services/1/stream ")
        || header.starts_with(b"GET /api/services/2/stream ")
    {
        streams.fetch_add(1, Ordering::AcqRel);
        let ts = include_bytes!("../../../tests/fixtures/recording-seek.ts");
        let augmented;
        let ts: &[u8] = if let Traffic::DataBroadcast { automatic } = traffic {
            augmented = super::data_broadcast::fixture::with_overlay(ts, automatic);
            &augmented
        } else {
            ts
        };
        const PACKET: usize = crate::transport::wire::TS_PACKET_SIZE;
        let send_interval = match traffic {
            Traffic::Broadcast | Traffic::DataBroadcast { .. } => BROADCAST_SEND_INTERVAL,
            Traffic::CapacityPressure => SEND_INTERVAL,
        };
        let intervals = FIXTURE_SECONDS
            * (Duration::from_secs(1).as_millis() / send_interval.as_millis()) as usize;
        let packets = ts.len() / PACKET;
        // Valid null packets raise the raw bitrate without changing decode load.
        // 16 MiB capacity then expires in about four seconds on either store.
        const PAD_BYTES_PER_INTERVAL: usize = 1024 * 1024;
        const PAYLOAD_ONLY: u8 = 0x10;
        let null_header = [
            crate::transport::wire::SYNC_BYTE,
            (crate::transport::wire::Pid::NULL.0 >> u8::BITS) as u8,
            crate::transport::wire::Pid::NULL.0 as u8,
            PAYLOAD_ONLY,
        ];
        let mut null_packet = [crate::transport::wire::STUFFING_BYTE; PACKET];
        null_packet[..null_header.len()].copy_from_slice(&null_header);
        let padding = match traffic {
            Traffic::Broadcast | Traffic::DataBroadcast { .. } => Vec::new(),
            Traffic::CapacityPressure => null_packet.repeat(PAD_BYTES_PER_INTERVAL / PACKET),
        };
        write!(
            socket,
            "HTTP/1.1 200 OK\r\nContent-Type: video/mp2t\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            ts.len() + intervals * padding.len()
        )?;
        for interval in 0..intervals {
            if stopped.load(Ordering::Acquire) {
                break;
            }
            let start = packets * interval / intervals * PACKET;
            let end = packets * (interval + 1) / intervals * PACKET;
            socket.write_all(&ts[start..end])?;
            socket.write_all(&padding)?;
            thread::sleep(send_interval);
        }
    } else {
        let body = if header.starts_with(b"GET /api/services ") {
            // Two selections use the same TS fixture to isolate stream replacement.
            r#"[{"id":1,"serviceId":1,"networkId":1,"name":"Timeshift fixture","type":1,"channel":{"type":"GR"}},{"id":2,"serviceId":1,"networkId":1,"name":"Second fixture","type":1,"channel":{"type":"GR"}}]"#
        } else {
            "[]"
        };
        write!(
            socket,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )?;
    }
    Ok(())
}

fn check_data_broadcast(
    app: &QGuiApplication,
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
) -> TestResult {
    // Open through the same mouse path as the toolbar button in production.
    evaluate(engine, "setup.close(); overlayVisibility.reveal(); true")?;
    wait_for(
        app,
        engine,
        "playerControls.visible && player.data_broadcast_available && !setup.visible",
    )?;
    ffi::clickRootItem(engine.pin_mut(), &QString::from("dataBroadcastButton"))?;
    wait_for(app, engine, "root.showDataBroadcast")?;
    wait_for(
        app,
        engine,
        "dataBroadcast.item !== null && dataBroadcast.item.browserReady",
    )?;
    wait_for(app, engine, "player.data_broadcast_connected()")?;
    // This TS has no BML. Merely creating a browser does not claim keys.
    wait_for(app, engine, "inputContext.navigationEnabled")?;
    evaluate(engine, "surface.forceActiveFocus(); true")?;
    ffi::clickRootKey(engine.pin_mut(), &QString::from("Escape"))?;
    assert!(evaluate(
        engine,
        "dataBroadcast.view !== null && player.data_broadcast_requested"
    )?);
    evaluate(engine, "player.configure_data_broadcast(false); true")?;
    wait_for(app, engine, "dataBroadcast.view === null")?;
    evaluate(engine, "player.configure_data_broadcast(true); true")?;
    Ok(())
}

pub(super) fn run_data_broadcast(
    app: &QGuiApplication,
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
) -> TestResult {
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
        "player.server_configured && !player.loading && player.selected >= 0",
    )?;
    evaluate(engine, "player.play(); true")?;
    wait_for(app, engine, "player.playing && player.media_active")?;
    check_data_broadcast_disabled(app, engine)?;
    toggle_data_broadcast_setting(app, engine, "dataBroadcastEnabledSetting")?;
    wait_for(
        app,
        engine,
        "player.data_broadcast_enabled && player.data_broadcast_available",
    )?;
    assert!(
        crate::settings::Loaded::open(crate::settings::settings_path()?)?
            .preferences()
            .data_broadcast_enabled
    );
    evaluate(engine, "settings.close(); true")?;
    check_data_broadcast_prefetch(app, engine)?;
    check_data_broadcast(app, engine)?;
    super::data_broadcast::run(app, engine)?;
    check_data_broadcast_transitions(app, engine)?;
    evaluate(engine, "player.stop(); true")?;
    for automatic in [true, false] {
        evaluate(
            engine,
            "player.configure_data_broadcast(false); player.configure_data_broadcast_prefetch(true); true",
        )?;
        let broadcast = Server::new(Traffic::DataBroadcast { automatic })?;
        assert!(evaluate(
            engine,
            &format!(
                "player.connect_server({})",
                serde_json::to_string(&broadcast.url)?
            )
        )?);
        wait_for(
            app,
            engine,
            "player.server_configured && !player.loading && player.selected >= 0",
        )?;
        evaluate(engine, "player.select(0); player.play(); true")?;
        wait_for(
            app,
            engine,
            "player.playing && player.media_active && !player.recording",
        )?;
        // Even an automatic-start PMT plus a saved prefetch preference cannot
        // bypass the opt-in. Re-enable on the same input without reconnecting.
        check_data_broadcast_disabled(app, engine)?;
        let connections = broadcast.streams.load(Ordering::Acquire);
        toggle_data_broadcast_setting(app, engine, "dataBroadcastEnabledSetting")?;
        evaluate(
            engine,
            "settings.close(); player.configure_data_broadcast_prefetch(false); true",
        )?;
        super::data_broadcast::run_entry(app, engine, automatic)?;
        evaluate(engine, "root.setDataBroadcast(true); true")?;
        wait_for(
            app,
            engine,
            "root.showDataBroadcast && player.data_broadcast_connected()",
        )?;
        let endpoint = ffi::evaluate_root(
            engine.pin_mut(),
            &QString::from("player.data_broadcast_endpoint"),
        )?
        .value::<QString>()
        .ok_or("data broadcast endpoint")?
        .to_string();
        let endpoint = url::Url::parse(&endpoint)?;
        let socket =
            std::net::SocketAddr::from(([127, 0, 0, 1], endpoint.port().ok_or("loopback port")?));
        toggle_data_broadcast_setting(app, engine, "dataBroadcastEnabledSetting")?;
        evaluate(engine, "settings.close(); true")?;
        wait_for(
            app,
            engine,
            "!settings.visible && inputContext.navigationEnabled && !inputContext.dataBroadcastFocused",
        )?;
        check_data_broadcast_disabled(app, engine)?;
        assert!(
            !crate::settings::Loaded::open(crate::settings::settings_path()?)?
                .preferences()
                .data_broadcast_enabled
        );
        assert!(
            std::net::TcpStream::connect(socket).is_err(),
            "disabled receiver still owns its listener"
        );
        assert_eq!(
            broadcast.streams.load(Ordering::Acquire),
            connections,
            "toggling the feature restarted playback"
        );
        evaluate(engine, "player.stop(); true")?;
    }
    Ok(())
}

fn check_data_broadcast_disabled(
    app: &QGuiApplication,
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
) -> TestResult {
    evaluate(engine, "setup.close(); surface.forceActiveFocus(); true")?;
    ffi::clickRootKey(engine.pin_mut(), &QString::from("D"))?;
    assert!(evaluate(engine, "!player.data_broadcast_open(true)")?);
    let deadline = Instant::now() + Duration::from_millis(500);
    while Instant::now() < deadline {
        app.process_events();
        assert!(evaluate(
            engine,
            "!player.data_broadcast_enabled && !player.data_broadcast_available && !player.data_broadcast_receiving() && !player.data_broadcast_connected() && !player.data_broadcast_requested && player.data_broadcast_endpoint === '' && dataBroadcast.view === null && !playerControls.navigationItems.find(item => item.objectName === 'dataBroadcastButton').visible && player.playing"
        )?);
        thread::sleep(ACCEPT_POLL);
    }
    Ok(())
}

fn toggle_data_broadcast_setting(
    app: &QGuiApplication,
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
    setting: &str,
) -> TestResult {
    evaluate(engine, "root.openSettings(SettingsPanel.Display); true")?;
    wait_for(app, engine, "settings.opened && settings.pageReveal === 1")?;
    evaluate(
        engine,
        &format!(
            "settings.pageFields.find(item => item.objectName === '{setting}').forceActiveFocus(Qt.TabFocusReason); true"
        ),
    )?;
    wait_for(
        app,
        engine,
        &format!("root.activeFocusItem.objectName === '{setting}'"),
    )?;
    ffi::clickRootItem(engine.pin_mut(), &QString::from(setting))?;
    Ok(())
}

fn check_data_broadcast_prefetch(
    app: &QGuiApplication,
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
) -> TestResult {
    assert!(evaluate(
        engine,
        "!player.data_broadcast_prefetch && !player.data_broadcast_receiving()"
    )?);
    evaluate(engine, "setup.close(); true")?;
    toggle_data_broadcast_setting(app, engine, "dataBroadcastPrefetchSetting")?;
    wait_for(
        app,
        engine,
        "player.data_broadcast_prefetch && player.data_broadcast_receiving()",
    )?;
    assert!(evaluate(
        engine,
        "!player.data_broadcast_requested && dataBroadcast.view === null && !player.data_broadcast_connected()"
    )?);
    assert!(
        crate::settings::Loaded::open(crate::settings::settings_path()?)?
            .preferences()
            .data_broadcast_prefetch
    );
    evaluate(
        engine,
        "settings.close(); root.setDataBroadcast(true); true",
    )?;
    wait_for(
        app,
        engine,
        "dataBroadcast.view !== null && player.data_broadcast_connected()",
    )?;
    // Changing the prefetch preference while displaying BML must leave its
    // browser, endpoint and decoder running, without reloading the page.
    assert!(evaluate(
        engine,
        "(() => { const view = dataBroadcast.view; const endpoint = player.data_broadcast_endpoint; player.configure_data_broadcast_prefetch(false); return player.data_broadcast_requested && dataBroadcast.view === view && player.data_broadcast_endpoint === endpoint && player.data_broadcast_receiving(); })()"
    )?);
    evaluate(
        engine,
        "player.configure_data_broadcast_prefetch(true); root.setDataBroadcast(false); true",
    )?;
    wait_for(
        app,
        engine,
        "dataBroadcast.view === null && !player.data_broadcast_connected() && player.data_broadcast_receiving()",
    )?;
    toggle_data_broadcast_setting(app, engine, "dataBroadcastPrefetchSetting")?;
    wait_for(
        app,
        engine,
        "!player.data_broadcast_prefetch && !player.data_broadcast_receiving()",
    )?;
    assert!(
        !crate::settings::Loaded::open(crate::settings::settings_path()?)?
            .preferences()
            .data_broadcast_prefetch
    );
    evaluate(engine, "settings.close(); true")?;
    wait_for(app, engine, "!settings.visible")?;
    println!(
        "Data broadcast prefetch: settings persist, no browser until opened, live preference changes retain the open view"
    );
    Ok(())
}

fn check_data_broadcast_transitions(
    app: &QGuiApplication,
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
) -> TestResult {
    const CLOSED: &str = "!player.data_broadcast_requested && player.data_broadcast_endpoint === '' && dataBroadcast.view === null && !root.showDataBroadcast";
    evaluate(
        engine,
        "player.configure_data_broadcast_prefetch(true); true",
    )?;
    // Re-selecting the same service keeps even a hidden standby browser.
    assert!(evaluate(
        engine,
        "root.dataBroadcastSessionOpen && dataBroadcast.view !== null && !root.showDataBroadcast"
    )?);
    assert!(evaluate(
        engine,
        "(() => { const view = dataBroadcast.view; player.select(player.selected); return root.dataBroadcastSessionOpen && dataBroadcast.view === view; })()"
    )?);
    evaluate(engine, "player.select(player.selected === 0 ? 1 : 0); true")?;
    wait_for(app, engine, CLOSED)?;
    wait_for(app, engine, "player.playing && player.media_active")?;
    assert!(evaluate(engine, CLOSED)?);
    assert!(evaluate(engine, "!player.data_broadcast_connected()")?);
    wait_for(app, engine, "player.data_broadcast_receiving()")?;

    // Explicit stop must not reopen data broadcasting on the next Play.
    evaluate(engine, "root.setDataBroadcast(true); true")?;
    wait_for(
        app,
        engine,
        "dataBroadcast.view !== null && player.data_broadcast_connected()",
    )?;
    evaluate(engine, "player.stop(); true")?;
    wait_for(app, engine, CLOSED)?;
    assert!(evaluate(engine, "!player.data_broadcast_available")?);
    assert!(evaluate(engine, "!player.data_broadcast_receiving()")?);
    evaluate(engine, "player.play(); true")?;
    wait_for(app, engine, "player.playing && player.media_active")?;
    assert!(evaluate(engine, CLOSED)?);
    wait_for(app, engine, "player.data_broadcast_receiving()")?;

    // Cover both live -> file and file -> file through the asynchronous loader.
    for name in ["recording-seek.ts", "recording-pid-change.ts"] {
        evaluate(engine, "root.setDataBroadcast(true); true")?;
        wait_for(
            app,
            engine,
            "dataBroadcast.view !== null && player.data_broadcast_connected()",
        )?;
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures")
            .join(name);
        let url = url::Url::from_file_path(path).map_err(|()| "invalid recording fixture path")?;
        assert!(evaluate(
            engine,
            &format!(
                "player.open_recording({})",
                serde_json::to_string(url.as_str())?
            )
        )?);
        wait_for(
            app,
            engine,
            "!player.recording_loading && player.recording && player.playing",
        )?;
        assert!(evaluate(engine, CLOSED)?);
        assert!(evaluate(engine, "!player.data_broadcast_receiving()")?);
    }
    evaluate(
        engine,
        "player.configure_data_broadcast_prefetch(false); true",
    )?;
    println!(
        "Data broadcast mode: standby ends on channel/file changes; Stop cancels automatic reopening"
    );
    Ok(())
}

fn capture_live_bml(
    app: &QGuiApplication,
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
    service: u64,
    output: &std::path::Path,
) -> TestResult {
    // A visible startup document can contain only video. Retain its URL and
    // rectangle beside the screenshot so success requires visual review too.
    let observer =
        "import QtQuick; Item { objectName: 'bmlLiveObserver'; property var snapshot: null; }";
    // Reuse the observer. Deferred deletion is not guaranteed to run in this
    // manual process_events loop; finding an old observer could otherwise
    // accept its previous result and leave the new callback pending at exit.
    const LIVE_OBSERVER: &str =
        "root.contentItem.children.find(child => child.objectName === 'bmlLiveObserver')";
    evaluate(
        engine,
        &format!(
            "(() => {{ const observer = {LIVE_OBSERVER} || Qt.createQmlObject({}, root.contentItem); observer.snapshot = null; dataBroadcast.view.runJavaScript('window.nagamePresentation()', function(value) {{ observer.snapshot = value; }}); return true; }})()",
            serde_json::to_string(observer)?
        ),
    )?;
    wait_for(app, engine, &format!("{LIVE_OBSERVER}.snapshot !== null"))?;
    let mut snapshot = super::screenshots::json(engine, &format!("{LIVE_OBSERVER}.snapshot"))?;
    snapshot["input"] = super::screenshots::json(
        engine,
        "({focus: root.activeFocusItem ? String(root.activeFocusItem) : null, bmlFocus: inputContext.dataBroadcastFocused, videoFocus: inputContext.videoFocused, navigation: inputContext.navigationEnabled, bmlUp: inputContext.bmlAccepts('Up'), popup: inputContext.popupOpen, editing: inputContext.editingText})",
    )?;
    std::fs::write(
        output.with_extension("json"),
        serde_json::to_vec_pretty(&snapshot)?,
    )?;
    tracing::info!(operation = "observe live BML document", service, snapshot = %snapshot);
    let status = std::process::Command::new("import")
        .arg("-window")
        .arg("root")
        .arg(output)
        .status()?;
    if !status.success() {
        return Err(format!(
            "could not capture the private display at {}: {status}",
            output.display()
        )
        .into());
    }
    Ok(())
}

/// Manual integration probe against an explicitly supplied live Mirakurun.
/// The public GUI session validates display, GPU and audio before this runs.
pub(super) fn run_data_broadcast_live(
    app: &QGuiApplication,
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
    server: &str,
    service: u64,
    output: &std::path::Path,
    press_data_again: bool,
) -> TestResult {
    evaluate(engine, "player.configure_data_broadcast(true); true")?;
    assert!(evaluate(
        engine,
        &format!("player.connect_server({})", serde_json::to_string(server)?)
    )?);
    wait_for(
        app,
        engine,
        "player.server_configured && !player.loading && player.channels.count > 0",
    )?;
    evaluate(engine, "setup.close(); true")?;
    let select = format!(
        "(() => {{ for (let i = 0; i < player.channels.count; i++) {{ if (String(player.channels.row(i).serviceId) === '{service}') {{ player.select(i); return true; }} }} return false; }})()"
    );
    if !evaluate(engine, &select)? {
        return Err(format!("service {service} is not in the Mirakurun catalogue").into());
    }
    evaluate(engine, "player.play(); true")?;
    // Live tuner acquisition can take longer than the local fixture's deadline.
    super::startup::wait_for_timeout(
        app,
        engine,
        "player.playing && player.media_active",
        Duration::from_secs(30),
    )?;
    evaluate(
        engine,
        "surface.forceActiveFocus(); root.setDataBroadcast(true); true",
    )?;
    wait_for(
        app,
        engine,
        "dataBroadcast.item !== null && dataBroadcast.item.browserReady",
    )?;
    wait_for(app, engine, "player.data_broadcast_connected()")?;
    // Some broadcast startup scripts initialize empty NVRAM over six 10-second
    // timer cycles. Observe beyond that period before judging the first screen.
    let deadline = Instant::now() + Duration::from_secs(90);
    let mut availability_check = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        app.process_events();
        let now = Instant::now();
        if now >= availability_check {
            if evaluate(
                engine,
                "dataBroadcast.view.presentation === DataBroadcastView.Unavailable",
            )? {
                break;
            }
            availability_check = now + Duration::from_secs(1);
        }
        thread::sleep(Duration::from_millis(10));
    }
    capture_live_bml(app, engine, service, output)?;
    if !evaluate(
        engine,
        "dataBroadcast.view !== null && (dataBroadcast.view.presentation === DataBroadcastView.Presenting || dataBroadcast.view.presentation === DataBroadcastView.Unavailable)",
    )? {
        return Err(format!(
            "BML did not reach a visible, loaded document; capture: {}",
            output.display()
        )
        .into());
    }
    evaluate(engine, "surface.forceActiveFocus(); true")?;
    let input = ffi::evaluate_root(
        engine.pin_mut(),
        &QString::from("JSON.stringify({focus: root.activeFocusItem ? root.activeFocusItem.objectName : null, bmlFocus: inputContext.dataBroadcastFocused, navigation: inputContext.navigationEnabled, guideShortcut: shortcutBindings.entries.find(binding => binding.objectName === 'guideShortcut').enabled})"),
    )?
    .value::<QString>()
    .ok_or("missing data broadcast input state")?;
    if !evaluate(
        engine,
        "!inputContext.navigationEnabled && !shortcutBindings.entries.find(binding => binding.objectName === 'guideShortcut').enabled",
    )? {
        return Err(format!("data broadcast left application shortcuts enabled: {input}").into());
    }
    if press_data_again {
        ffi::clickRootKey(engine.pin_mut(), &QString::from("D"))?;
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            app.process_events();
            thread::sleep(Duration::from_millis(10));
        }
        capture_live_bml(
            app,
            engine,
            service,
            &output.with_extension("after-data-key.png"),
        )?;
        if evaluate(engine, "inputContext.navigationEnabled")? {
            ffi::clickRootKey(engine.pin_mut(), &QString::from("Down"))?;
            wait_for(app, engine, "playerControls.activeFocus")?;
        }
    }
    evaluate(engine, "player.stop(); true")?;
    Ok(())
}

pub(super) fn run(
    app: &QGuiApplication,
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
) -> TestResult {
    let observer_source =
        serde_json::to_string(include_str!("../../../tests/live-timeline-observer.qml"))?;
    assert!(evaluate(
        engine,
        &format!("Qt.createQmlObject({observer_source}, root.contentItem).backend = player; true")
    )?);
    let server = Server::new(Traffic::Broadcast)?;
    assert!(evaluate(
        engine,
        &format!(
            "player.stop(); player.connect_server({})",
            serde_json::to_string(&server.url)?
        )
    )?);
    wait_for(
        app,
        engine,
        "player.server_configured && !player.loading && player.selected >= 0",
    )?;
    evaluate(engine, "root.openSettings(SettingsPanel.Timeshift); true")?;
    wait_for(app, engine, "settings.opened")?;
    evaluate(engine, "settings.close(); true")?;
    for backend in ["memory", "filesystem"] {
        let buffer_ms = if backend == "memory" {
            CUSTOM_LIVE_BUFFER_MS
        } else {
            crate::settings::LiveBuffer::DEFAULT_MS
        };
        assert!(evaluate(
            engine,
            &format!("player.configure_live_buffer({buffer_ms})")
        )?);
        start_near_live(
            app,
            engine,
            &server,
            backend,
            &format!(
                "player.configure_timeshift_options('{backend}', {MEMORY_MIB}, {FILESYSTEM_MIB}, {RETENTION_MINUTES}); player.select(0); player.play(); true"
            ),
        )?;
        if let Err(error) = wait_for(
            app,
            engine,
            &format!(
                "player.timeshift && player.playing && player.seekable && player.timeshift_bytes_per_second > 0 && player.duration_ms > {}",
                MIN_SEEKABLE_HISTORY.as_millis()
            ),
        ) {
            let state = super::bridge::ffi::evaluate_root(
                engine.pin_mut(),
                &cxx_qt_lib::QString::from(
                    "JSON.stringify({timeshift: player.timeshift, playing: player.playing, recording: player.recording, seekable: player.seekable, duration: player.duration_ms, error: player.playback_error})",
                ),
            )?;
            return Err(format!("{error}: {state:?}").into());
        }
        wait_for(
            app,
            engine,
            "JSON.parse(player.live_timeline).live.program !== null",
        )?;
        assert!(evaluate(
            engine,
            &format!("{OBSERVER}.failure.length === 0")
        )?);
        if backend == "memory" {
            // Editing the reserve must leave this stream and its playhead alone.
            // The following explicit return-to-live must use the new value.
            assert!(evaluate(
                engine,
                &format!(
                    "(function() {{ const count = {OBSERVER}.seekRequests; const session = JSON.parse(player.live_timeline).session; return player.configure_live_buffer({UPDATED_LIVE_BUFFER_MS}) && !player.seeking && player.playing && {OBSERVER}.seekRequests === count && JSON.parse(player.live_timeline).session === session; }})()"
                )
            )?);
        }
        return_to_live(app, engine, backend, "already playing")?;
        assert!(evaluate(
            engine,
            &format!(
                "!{OBSERVER}.previousSession.length || (!player.seek_timeline({OBSERVER}.previousSession, 0) && !player.skip_timeline({OBSERVER}.previousSession, 0))"
            )
        )?);
        evaluate(engine, "viewerActions.playbackToggle.trigger(); true")?;
        wait_for(app, engine, "player.paused && !player.seeking")?;
        assert!(evaluate(
            engine,
            "player.request_seek_preview((player.window_start_ms + player.window_end_ms) / 2); player.paused && !player.seeking"
        )?);
        wait_for(
            app,
            engine,
            "player.seek_preview_image.startsWith('data:image/png;base64,')",
        )?;
        assert!(evaluate(
            engine,
            "player.clear_seek_preview(); player.seek_preview_image.length === 0 && player.paused && !player.seeking"
        )?);
        assert!(evaluate(
            engine,
            &format!("{OBSERVER}.saved = JSON.parse(player.live_timeline); true")
        )?);
        let before = super::bridge::ffi::evaluate_root(
            engine.pin_mut(),
            &cxx_qt_lib::QString::from("player.duration_ms"),
        )?
        .value::<f64>()
        .ok_or("retained end")?;
        wait_for(
            app,
            engine,
            &format!(
                "player.paused && player.duration_ms > {}",
                before + PAUSE_RECEIVE_GROWTH.as_millis() as f64
            ),
        )?;
        assert!(evaluate(
            engine,
            &format!(
                "JSON.parse(player.live_timeline).axis.start === {OBSERVER}.saved.axis.start && JSON.parse(player.live_timeline).axis.end === {OBSERVER}.saved.axis.end && JSON.parse(player.live_timeline).viewing.position === {OBSERVER}.saved.viewing.position && JSON.parse(player.live_timeline).live.position > {OBSERVER}.saved.live.position"
            )
        )?);
        assert!(evaluate(
            engine,
            &format!(
                "(function() {{ const session = JSON.parse(player.live_timeline).session; return player.seek_timeline(session, {target}) && player.skip_timeline(session, {step}) && player.seek_target_ms === {forward} && player.skip_timeline(session, -{step}) && player.seek_target_ms === {target}; }})()",
                target = REWIND_TARGET.as_millis(),
                step = REPEATED_SEEK_STEP.as_millis(),
                forward = (REWIND_TARGET + REPEATED_SEEK_STEP).as_millis(),
            )
        )?);
        wait_for(
            app,
            engine,
            &format!(
                "player.paused && !player.seeking && Math.abs(player.position_ms - {}) < {}",
                REWIND_TARGET.as_millis(),
                SEEK_TOLERANCE.as_millis()
            ),
        )?;
        assert!(evaluate(
            engine,
            &format!("{OBSERVER}.failure.length === 0 && {OBSERVER}.notifications > 0")
        )?);
        assert!(evaluate(
            engine,
            "JSON.parse(player.live_timeline).viewing.program !== null && JSON.parse(player.live_timeline).seekTarget === null"
        )?);
        let connections = server.streams.load(Ordering::Acquire);
        assert!(evaluate(
            engine,
            &format!(
                "{OBSERVER}.saved = JSON.parse(player.live_timeline); player.configure_timeshift_options('{backend}', {}, {}, {})",
                MEMORY_MIB * 2,
                FILESYSTEM_MIB * 2,
                RETENTION_MINUTES + 1
            )
        )?);
        wait_for(
            app,
            engine,
            &format!(
                "player.duration_ms > {OBSERVER}.saved.live.position + {}",
                PAUSE_RECEIVE_GROWTH.as_millis()
            ),
        )?;
        assert!(evaluate(
            engine,
            &format!(
                "player.paused && !player.seeking && JSON.parse(player.live_timeline).session === {OBSERVER}.saved.session && Math.abs(player.position_ms - {OBSERVER}.saved.viewing.position) < {}",
                SEEK_TOLERANCE.as_millis()
            )
        )?);
        assert_eq!(
            server.streams.load(Ordering::Acquire),
            connections,
            "limit update reconnected the stream"
        );
        // Bound the catch-up duration independently of earlier UI checks.
        assert!(evaluate(
            engine,
            &format!("player.seek_to(player.window_end_ms - {SPEED_CATCH_UP_DELAY_MS})")
        )?);
        wait_for(
            app,
            engine,
            "player.paused && !player.seeking && player.speed_available",
        )?;
        assert!(evaluate(engine, "player.set_playback_rate(20)")?);
        wait_for(
            app,
            engine,
            "player.paused && !player.seeking && player.playback_rate === 20",
        )?;
        evaluate(engine, "viewerActions.playbackToggle.trigger(); true")?;
        if let Err(error) = wait_for(
            app,
            engine,
            "player.playing && !player.seeking && player.playback_rate === 10 && player.at_live_edge",
        ) {
            let state = super::bridge::ffi::evaluate_root(
                engine.pin_mut(),
                &cxx_qt_lib::QString::from(
                    "JSON.stringify({playing:player.playing,paused:player.paused,seeking:player.seeking,rate:player.playback_rate,requested:player.requested_playback_rate,delay:player.live_delay_ms,position:player.position_ms,end:player.window_end_ms,edge:player.at_live_edge,error:player.playback_error,transport:player.transport_error})",
                ),
            )?;
            return Err(format!("Speed catch-up: {error}: {state:?}").into());
        }
        assert!(evaluate(
            engine,
            "!player.speed_available && !player.set_playback_rate(15)"
        )?);
        check_live_button(engine)?;
        observe_playback(
            app,
            engine,
            "player.playback_rate === 10 && player.at_live_edge",
        )?;
        assert!(evaluate(engine, "player.seek_to(1000)")?);
        wait_for(app, engine, "!player.seeking && player.speed_available")?;
        assert!(evaluate(engine, "player.set_playback_rate(15)")?);
        return_to_live(app, engine, backend, "paused in history")?;
        assert!(evaluate(
            engine,
            "player.playback_rate === 10 && player.requested_playback_rate === 10"
        )?);
        assert!(evaluate(
            engine,
            &format!("{OBSERVER}.previousSession = JSON.parse(player.live_timeline).session; true")
        )?);
        let disk_cache = if backend == "filesystem" {
            let cache = std::path::PathBuf::from(
                std::env::var_os("XDG_CACHE_HOME").ok_or("isolated cache")?,
            )
            .join("nagametv/timeshift");
            assert_eq!(
                anonymous_history_files(&cache)?.len(),
                1,
                "filesystem history must own exactly one anonymous file"
            );
            Some(cache)
        } else {
            None
        };
        assert!(evaluate(
            engine,
            "player.stop(); player.live_timeline === 'null' && !player.timeshift && !player.media_active && player.timeshift_bytes_per_second === 0"
        )?);
        if let Some(cache) = disk_cache {
            let deadline = std::time::Instant::now() + NETWORK_TIMEOUT;
            while !anonymous_history_files(&cache)?.is_empty() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "stopped appsrc retained disk files"
                );
                app.process_events();
                thread::sleep(ACCEPT_POLL);
            }
        }
        eprintln!(
            "Timeshift {backend}: receive while paused, backward seek, return to live and stop passed"
        );
    }
    for storage in ["pause_memory", "pause_filesystem"] {
        pause_on_demand(app, engine, &server, storage)?;
    }
    start_near_live(
        app,
        engine,
        &server,
        "disabled",
        "player.configure_timeshift('off'); player.select(0); player.play(); true",
    )?;
    start_near_live(
        app,
        engine,
        &server,
        "disabled, channel change",
        "player.select(1); true",
    )?;
    wait_for(
        app,
        engine,
        &format!(
            "player.playing && !player.timeshift && !player.seekable && JSON.parse(player.video_stats()).rendered > 0 && ({LIVE_METADATA_READY})"
        ),
    )?;
    check_data_broadcast_disabled(app, engine)?;
    observe_playback(
        app,
        engine,
        &format!(
            "!player.timeshift && !player.seekable && !player.playback_error.length && ({LIVE_METADATA_READY})"
        ),
    )?;
    assert!(evaluate(engine, "!player.pause()")?);
    let connections = server.streams.load(Ordering::Acquire);
    assert!(evaluate(engine, "player.configure_timeshift('memory')")?);
    wait_for(
        app,
        engine,
        "player.playing && player.timeshift && player.seekable",
    )?;
    assert!(evaluate(engine, "player.pause()")?);
    wait_for(app, engine, "player.paused")?;
    assert!(evaluate(engine, "player.configure_timeshift('off')")?);
    wait_for(
        app,
        engine,
        &format!(
            "player.playing && !player.paused && !player.timeshift && !player.seekable && ({LIVE_METADATA_READY})"
        ),
    )?;
    observe_playback(
        app,
        engine,
        &format!(
            "!player.timeshift && !player.seekable && !player.playback_error.length && ({LIVE_METADATA_READY})"
        ),
    )?;
    evaluate(engine, "player.stop(); true")?;
    assert_eq!(
        server.streams.load(Ordering::Acquire),
        connections,
        "enable/disable reconnected the stream"
    );
    eprintln!(
        "Timeshift disabled: playback and program/broadcast clock stay available; pause is rejected; enable/disable applies during playback and pause"
    );
    let pressure = Server::new(Traffic::CapacityPressure)?;
    evaluate(
        engine,
        &format!(
            "player.connect_server({})",
            serde_json::to_string(&pressure.url)?
        ),
    )?;
    wait_for(
        app,
        engine,
        "player.server_configured && !player.loading && player.selected >= 0",
    )?;
    for storage in ["memory", "filesystem", "pause_memory", "pause_filesystem"] {
        evaluate(
            engine,
            &format!(
                "player.configure_timeshift_options('{storage}', {MEMORY_MIB}, {MEMORY_MIB}, {RETENTION_MINUTES}); player.select(0); player.play(); true"
            ),
        )?;
        wait_for(
            app,
            engine,
            "player.playing && !player.seeking && player.pausable && player.position_ms > 0 && JSON.parse(player.live_timeline).viewing !== null && JSON.parse(player.live_timeline).viewing.program !== null",
        )?;
        assert!(evaluate(engine, "player.pause()")?);
        assert!(evaluate(
            engine,
            &format!("{OBSERVER}.saved = JSON.parse(player.live_timeline); true")
        )?);
        wait_for(
            app,
            engine,
            "player.paused && !player.seeking && player.position_ms < player.window_start_ms",
        )?;
        assert!(evaluate(
            engine,
            &format!(
                "JSON.parse(player.live_timeline).axis.start === {OBSERVER}.saved.axis.start && JSON.parse(player.live_timeline).axis.end === {OBSERVER}.saved.axis.end && JSON.parse(player.live_timeline).viewing.position === {OBSERVER}.saved.viewing.position && JSON.stringify(JSON.parse(player.live_timeline).viewing.program) === JSON.stringify({OBSERVER}.saved.viewing.program) && JSON.parse(player.live_timeline).viewing.availability === 'expired' && {OBSERVER}.failure.length === 0"
            )
        )?);
        assert!(evaluate(
            engine,
            &format!(
                "{OBSERVER}.saved = JSON.parse(player.live_timeline); player.configure_timeshift_options('{storage}', {}, {}, {RETENTION_MINUTES})",
                MEMORY_MIB * 2,
                MEMORY_MIB * 2
            )
        )?);
        wait_for(
            app,
            engine,
            &format!(
                "player.duration_ms > {OBSERVER}.saved.live.position + {}",
                PAUSE_RECEIVE_GROWTH.as_millis()
            ),
        )?;
        assert!(evaluate(
            engine,
            &format!(
                "player.paused && !player.seeking && JSON.parse(player.live_timeline).viewing.position === {OBSERVER}.saved.viewing.position"
            )
        )?);
        assert!(evaluate(engine, "player.play(); true")?);
        wait_for(
            app,
            engine,
            "player.playing && !player.paused && !player.seeking && player.position_ms > player.window_start_ms",
        )?;
        observe_playback(
            app,
            engine,
            "player.timeshift && player.position_ms >= player.window_start_ms",
        )?;
        evaluate(engine, "player.stop(); true")?;
        eprintln!(
            "Timeshift {storage}: expired pause resumed and continued under capacity pressure"
        );
    }
    for storage in ["memory", "filesystem"] {
        assert!(evaluate(
            engine,
            &format!(
                "player.configure_timeshift_options('{storage}', {FILESYSTEM_MIB}, {FILESYSTEM_MIB}, {RETENTION_MINUTES}); player.play(); true"
            )
        )?);
        wait_for(
            app,
            engine,
            "player.playing && player.seekable && player.position_ms > 0",
        )?;
        assert!(evaluate(engine, "player.pause()")?);
        // Each shared wait has a five-second deadline; observe both milestones.
        for retained in [RESIZE_HISTORY / 2, RESIZE_HISTORY] {
            wait_for(
                app,
                engine,
                &format!(
                    "player.paused && !player.seeking && player.duration_ms > {}",
                    retained.as_millis()
                ),
            )?;
        }
        let connections = pressure.streams.load(Ordering::Acquire);
        assert!(evaluate(
            engine,
            &format!(
                "{OBSERVER}.saved = JSON.parse(player.live_timeline); player.configure_timeshift_options('{storage}', {MEMORY_MIB}, {MEMORY_MIB}, {RETENTION_MINUTES})"
            )
        )?);
        wait_for(
            app,
            engine,
            &format!(
                "player.paused && !player.seeking && player.window_start_ms > {OBSERVER}.saved.viewing.position && player.position_ms >= player.window_start_ms"
            ),
        )?;
        assert!(evaluate(
            engine,
            &format!(
                "JSON.parse(player.live_timeline).session === {OBSERVER}.saved.session && player.position_ms > {OBSERVER}.saved.viewing.position && player.transport_error.length > 0"
            )
        )?);
        let next_storage = if storage == "memory" {
            "filesystem"
        } else {
            "memory"
        };
        assert!(evaluate(
            engine,
            &format!(
                "{OBSERVER}.saved = JSON.parse(player.live_timeline); player.configure_timeshift('{next_storage}')"
            )
        )?);
        wait_for(
            app,
            engine,
            &format!(
                "player.playing && !player.paused && !player.seeking && player.position_ms >= {OBSERVER}.saved.live.position"
            ),
        )?;
        assert_eq!(
            pressure.streams.load(Ordering::Acquire),
            connections,
            "settings update reconnected the stream"
        );
        assert!(evaluate(engine, "player.transport_error.length > 0")?);
        observe_playback(
            app,
            engine,
            "player.timeshift && !player.playback_error.length",
        )?;
        // Four seconds of playback above leaves time for the six-second notice
        // to expire within the shared five-second wait, without another action.
        wait_for(app, engine, "player.transport_error.length === 0")?;
        evaluate(engine, "player.stop(); true")?;
        eprintln!(
            "Timeshift {storage}: shrinking moves paused playhead; storage switch returns to live without reconnecting; notice expires automatically"
        );
    }
    assert!(evaluate(
        engine,
        &format!("{OBSERVER}.failure.length === 0")
    )?);
    evaluate(engine, &format!("{OBSERVER}.destroy(); true"))?;
    Ok(())
}

fn pause_on_demand(
    app: &QGuiApplication,
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
    server: &Server,
    storage: &str,
) -> TestResult {
    let cache =
        std::path::PathBuf::from(std::env::var_os("XDG_CACHE_HOME").ok_or("isolated cache")?)
            .join("nagametv/timeshift");
    start_near_live(
        app,
        engine,
        server,
        storage,
        &format!(
            "player.configure_timeshift_options('{storage}', {MEMORY_MIB}, {FILESYSTEM_MIB}, {RETENTION_MINUTES}); player.select(0); player.play(); true"
        ),
    )?;
    assert!(evaluate(
        engine,
        "player.pausable && player.playback_action === Player.Pause && !player.timeshift && !player.seekable"
    )?);
    assert!(anonymous_history_files(&cache)?.is_empty());
    let connections = server.streams.load(Ordering::Acquire);
    for catch_up in [false, true] {
        evaluate(engine, "viewerActions.playbackToggle.trigger(); true")?;
        wait_for(
            app,
            engine,
            "player.paused && player.timeshift && player.seekable && !player.seeking",
        )?;
        evaluate(
            engine,
            &format!("{OBSERVER}.saved = JSON.parse(player.live_timeline); true"),
        )?;
        if storage == "pause_filesystem" {
            assert_eq!(anonymous_history_files(&cache)?.len(), 1);
        }
        wait_for(
            app,
            engine,
            &format!("player.paused && player.live_delay_ms > {SPEED_CATCH_UP_DELAY_MS}"),
        )?;
        assert!(evaluate(
            engine,
            &format!(
                "Math.abs(player.position_ms - {OBSERVER}.saved.viewing.position) < {}",
                SEEK_TOLERANCE.as_millis()
            )
        )?);
        evaluate(engine, "viewerActions.playbackToggle.trigger(); true")?;
        assert!(evaluate(
            engine,
            &format!(
                "player.playing && !player.seeking && Math.abs(player.position_ms - {OBSERVER}.saved.viewing.position) < {}",
                SEEK_TOLERANCE.as_millis()
            )
        )?);
        wait_for(
            app,
            engine,
            &format!(
                "player.position_ms > {OBSERVER}.saved.viewing.position + {} && player.speed_available",
                SEEK_TOLERANCE.as_millis()
            ),
        )?;
        if catch_up {
            assert!(evaluate(engine, "player.set_playback_rate(20)")?);
        } else {
            assert!(evaluate(engine, "player.return_to_live()")?);
        }
        wait_for(
            app,
            engine,
            "player.playing && !player.seeking && !player.timeshift && !player.seekable && player.pausable && player.playback_rate === 10",
        )?;
        assert!(anonymous_history_files(&cache)?.is_empty());
        observe_playback(
            app,
            engine,
            "!player.timeshift && !player.seekable && !player.playback_error.length",
        )?;
        assert_eq!(
            server.streams.load(Ordering::Acquire),
            connections,
            "automatic retention reconnected the stream"
        );
    }
    assert!(evaluate(engine, "player.pause()")?);
    wait_for(app, engine, "player.paused && player.timeshift")?;
    evaluate(engine, "player.select(1); true")?;
    wait_for(
        app,
        engine,
        "player.playing && !player.paused && !player.seeking && player.position_ms > 0 && !player.timeshift && !player.seekable",
    )?;
    assert!(anonymous_history_files(&cache)?.is_empty());
    assert!(evaluate(engine, "player.pause()")?);
    wait_for(app, engine, "player.paused && player.timeshift")?;
    evaluate(engine, "player.stop(); true")?;
    let deadline = Instant::now() + NETWORK_TIMEOUT;
    while !anonymous_history_files(&cache)?.is_empty() {
        assert!(
            Instant::now() < deadline,
            "stopped automatic history still owns disk files"
        );
        app.process_events();
        thread::sleep(ACCEPT_POLL);
    }
    eprintln!(
        "Timeshift {storage}: pause/resume, explicit live return, speed catch-up, re-pause, channel change and stop passed"
    );
    Ok(())
}

fn start_near_live(
    app: &QGuiApplication,
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
    server: &Server,
    context: &str,
    start: &str,
) -> TestResult {
    let previous = super::bridge::ffi::evaluate_root(
        engine.pin_mut(),
        &cxx_qt_lib::QString::from(format!("{OBSERVER}.seekRequests")),
    )?
    .value::<i32>()
    .ok_or("seek request count")?;
    let connections = server.streams.load(Ordering::Acquire);
    assert!(evaluate(engine, start)?);
    let headroom = live_buffer_ms(engine)?;
    let started = wait_for(
        app,
        engine,
        &format!(
            "player.playing && !player.seeking && !player.paused && player.position_ms > {} && JSON.parse(player.video_stats()).rendered > 0",
            PAUSE_RECEIVE_GROWTH.as_millis()
        ),
    );
    let startup_state = super::bridge::ffi::evaluate_root(
        engine.pin_mut(),
        &cxx_qt_lib::QString::from(format!(
            "JSON.stringify({{requests: {OBSERVER}.seekRequests, previous: {previous}, target: {OBSERVER}.lastSeekTarget, edge: {OBSERVER}.lastSeekEdge, requestedSession: {OBSERVER}.lastSeekSession, current: JSON.parse(player.live_timeline), playing: player.playing, seeking: player.seeking, paused: player.paused, position: player.position_ms, video: JSON.parse(player.video_stats()), error: player.playback_error}})"
        )),
    )?;
    if let Err(error) = started {
        return Err(format!("{context}: {error}: {startup_state:?}").into());
    }
    wait_for(
        app,
        engine,
        "JSON.parse(player.video_stats()).receive_latency.status === 'measuring'",
    )?;
    assert!(evaluate(
        engine,
        "(() => { const s = JSON.parse(player.video_stats()).receive_latency; return s.samples > 0 && s.latest_ms >= 0 && s.median_ms >= 0 && s.p95_ms >= s.median_ms && s.p95_ms <= 30000; })()"
    )?);
    wait_for(
        app,
        engine,
        "JSON.parse(player.video_stats()).pcr_deviation.status === 'measuring'",
    )?;
    assert!(evaluate(
        engine,
        "(() => { const s = JSON.parse(player.video_stats()).pcr_deviation; return s.samples > 0 && [s.latest_ms, s.median_ms, s.p95_ms].every(Number.isFinite) && s.p95_ms >= s.median_ms; })()"
    )?);
    // A fast startup already inside the live reserve needs no forward seek.
    // Wait for advancing playback above so the first-render decision is over.
    // If alignment was needed, bracket its target by the receive snapshots
    // before/after notification; reception continues during a native flush.
    let requests = super::bridge::ffi::evaluate_root(
        engine.pin_mut(),
        &cxx_qt_lib::QString::from(format!("{OBSERVER}.seekRequests")),
    )?
    .value::<i32>()
    .ok_or("seek request count")?;
    assert!(
        (previous..=previous + 1).contains(&requests),
        "{startup_state:?}"
    );
    assert!(
        evaluate(
            engine,
            &format!(
                "{OBSERVER}.seekRequests === {previous} || ({OBSERVER}.lastSeekTarget + {headroom} >= {OBSERVER}.lastSeekPreviousEdge && {OBSERVER}.lastSeekTarget + {headroom} <= {OBSERVER}.lastSeekEdge && {OBSERVER}.lastSeekSession === JSON.parse(player.live_timeline).session)",
                headroom = headroom
            ),
        )?,
        "{context}: startup did not target this source's live reserve: {startup_state:?}"
    );
    observe_playback(
        app,
        engine,
        &format!(
            "{OBSERVER}.seekRequests === {} && JSON.parse(player.live_timeline).live.position - player.position_ms < {}",
            requests,
            STARTUP_LIVE_EDGE_TOLERANCE.as_millis()
        ),
    )?;
    assert_eq!(
        server.streams.load(Ordering::Acquire),
        connections + 1,
        "{context}: initial alignment reconnected the stream"
    );
    let gap = super::bridge::ffi::evaluate_root(
        engine.pin_mut(),
        &cxx_qt_lib::QString::from(
            "JSON.parse(player.live_timeline).live.position - player.position_ms",
        ),
    )?
    .value::<f64>()
    .ok_or("startup live delay")?;
    eprintln!(
        "Live startup {context}: {} alignment(s), sustained receive-to-playhead gap {gap:.0} ms",
        requests - previous
    );
    Ok(())
}

fn return_to_live(
    app: &QGuiApplication,
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
    backend: &str,
    context: &str,
) -> TestResult {
    let headroom = live_buffer_ms(engine)?;
    // The sampled receive edge can be one fixture delivery behind.
    let target_tolerance = headroom + SEND_INTERVAL.as_millis() as i32;
    // No event processing between the pre-click snapshot and requested target.
    // Ordinary playback and a paused rewind use the same bounded live reserve.
    assert!(evaluate(
        engine,
        &format!("{OBSERVER}.saved = JSON.parse(player.live_timeline); player.return_to_live()"),
    )?);
    assert!(
        evaluate(
            engine,
            &format!(
                "(function() {{ const current = JSON.parse(player.live_timeline); return current.seekTarget !== null && current.seekTarget >= {OBSERVER}.saved.live.position - {} && current.seekTarget <= current.live.position && current.seekTarget >= {OBSERVER}.saved.viewing.position - {}; }})()",
                target_tolerance, headroom
            ),
        )?,
        "{backend}: live return from {context} exceeded the delivery reserve"
    );
    let before = super::bridge::ffi::evaluate_root(
        engine.pin_mut(),
        &cxx_qt_lib::QString::from(format!(
            "{OBSERVER}.saved.live.position - {OBSERVER}.saved.viewing.position"
        )),
    )?
    .value::<f64>()
    .ok_or("previous live delay")?;
    wait_for(
        app,
        engine,
        &format!(
            "player.playing && !player.paused && !player.seeking && player.at_live_edge && player.live_delay_ms < {}",
            LIVE_EDGE_TOLERANCE.as_millis()
        ),
    )?;
    // Check sustained playback too; the first post-seek buffer is not proof
    // that the cursor keeps advancing near live after decoder preroll.
    observe_playback(
        app,
        engine,
        &format!(
            "player.at_live_edge && player.live_delay_ms < {}",
            LIVE_EDGE_TOLERANCE.as_millis()
        ),
    )?;
    check_live_button(engine)?;
    let delay = super::bridge::ffi::evaluate_root(
        engine.pin_mut(),
        &cxx_qt_lib::QString::from("player.live_delay_ms"),
    )?
    .value::<f64>()
    .ok_or("live delay")?;
    eprintln!(
        "Timeshift {backend}: return from {context}, receive-to-playhead gap {before:.0} -> {delay:.0} ms"
    );
    Ok(())
}

fn check_live_button(engine: &mut cxx::UniquePtr<QQmlApplicationEngine>) -> TestResult {
    assert!(
        evaluate(
            engine,
            r#"
        (function() {
            function find(item) {
                if (item.objectName === 'returnToLiveButton') return item;
                for (const child of item.children || []) { const found = find(child); if (found) return found; }
                return null;
            }
            const button = find(playerControls);
            return player.at_live_edge && viewerActions.atLiveEdge && button
                && String(button.iconSource).endsWith('/radio.svg');
        })()
    "#
        )?,
        "live return did not select the icon with the red live indicator"
    );
    Ok(())
}

fn live_buffer_ms(engine: &mut cxx::UniquePtr<QQmlApplicationEngine>) -> TestResult<i32> {
    super::bridge::ffi::evaluate_root(
        engine.pin_mut(),
        &cxx_qt_lib::QString::from("JSON.parse(player.live_buffer_options).milliseconds"),
    )?
    .value::<i32>()
    .ok_or_else(|| "live buffer setting".into())
}

fn observe_playback(
    app: &QGuiApplication,
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
    condition: &str,
) -> TestResult {
    let position = super::bridge::ffi::evaluate_root(
        engine.pin_mut(),
        &cxx_qt_lib::QString::from("player.position_ms"),
    )?
    .value::<f64>()
    .ok_or("position")?;
    let until = Instant::now() + RECOVERY_OBSERVATION;
    while Instant::now() < until {
        app.process_events();
        assert!(
            evaluate(
                engine,
                &format!(
                    "player.playing && !player.paused && !player.seeking && !player.playback_error.length && ({condition})"
                )
            )?,
            "unexpected recovery during playback: {condition}"
        );
        thread::sleep(ACCEPT_POLL);
    }
    assert!(
        evaluate(
            engine,
            &format!(
                "player.position_ms > {}",
                position + PAUSE_RECEIVE_GROWTH.as_millis() as f64
            )
        )?,
        "video must advance after recovery"
    );
    Ok(())
}

#[cfg(target_os = "linux")]
fn anonymous_history_files(cache: &std::path::Path) -> std::io::Result<Vec<std::path::PathBuf>> {
    use std::os::unix::fs::MetadataExt;
    let mut files = Vec::new();
    for entry in std::fs::read_dir("/proc/self/fd")? {
        let path = entry?.path();
        match std::fs::read_link(&path) {
            Ok(target) if target.starts_with(cache) => {
                let metadata = match std::fs::metadata(&path) {
                    Ok(metadata) => metadata,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(error) => return Err(error),
                };
                assert_eq!(
                    metadata.nlink(),
                    0,
                    "timeshift data must have no directory entry"
                );
                files.push(path);
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(files)
}
#[cfg(not(target_os = "linux"))]
fn anonymous_history_files(_cache: &std::path::Path) -> std::io::Result<Vec<std::path::PathBuf>> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "anonymous TS descriptor checks require Linux procfs",
    ))
}
