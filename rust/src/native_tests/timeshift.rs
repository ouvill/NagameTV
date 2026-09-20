//! Product Main.qml and real output; the HTTP fixture only replaces the tuner.
use super::startup::{TestResult, evaluate, wait_for};
use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine};
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
        const PACKET: usize = crate::transport::wire::TS_PACKET_SIZE;
        let send_interval = match traffic {
            Traffic::Broadcast => BROADCAST_SEND_INTERVAL,
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
            Traffic::Broadcast => Vec::new(),
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
    evaluate(
        engine,
        "settings.open(); settings.page = SettingsPanel.Timeshift; true",
    )?;
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
                "!{OBSERVER}.previousSession.length || !player.seek_timeline({OBSERVER}.previousSession, 0)"
            )
        )?);
        evaluate(engine, "viewerActions.playbackToggle.trigger(); true")?;
        wait_for(app, engine, "player.paused && !player.seeking")?;
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
                "player.seek_timeline(JSON.parse(player.live_timeline).session, {})",
                REWIND_TARGET.as_millis()
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
    for storage in ["memory", "filesystem"] {
        evaluate(
            engine,
            &format!(
                "player.configure_timeshift_options('{storage}', {MEMORY_MIB}, {MEMORY_MIB}, {RETENTION_MINUTES}); player.select(0); player.play(); true"
            ),
        )?;
        wait_for(
            app,
            engine,
            "player.playing && !player.seeking && player.seekable && player.position_ms > 0 && JSON.parse(player.live_timeline).viewing !== null && JSON.parse(player.live_timeline).viewing.program !== null",
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
