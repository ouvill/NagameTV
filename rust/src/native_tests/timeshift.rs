//! Product Main.qml and real output; the HTTP fixture only replaces the tuner.
use super::startup::{TestResult, evaluate, wait_for};
use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const ACCEPT_POLL: Duration = Duration::from_millis(10);
const NETWORK_TIMEOUT: Duration = Duration::from_secs(2);
const SEND_INTERVAL: Duration = Duration::from_millis(250);
const FIXTURE_SECONDS: usize = 60;
const INTERVALS_PER_SECOND: usize = 4;
const MIN_SEEKABLE_HISTORY: Duration = Duration::from_secs(3);
const PAUSE_RECEIVE_GROWTH: Duration = Duration::from_secs(1);
const REWIND_TARGET: Duration = Duration::from_secs(1);
const SEEK_TOLERANCE: Duration = Duration::from_millis(500);
const LIVE_EDGE_TOLERANCE: Duration = Duration::from_millis(2500);
const MEMORY_MIB: i32 = 16;
const FILESYSTEM_MIB: i32 = 64;
const RETENTION_MINUTES: i32 = 1;
const OBSERVER: &str =
    "root.contentItem.children.find(child => child.objectName === 'liveTimelineObserver')";
const RECOVERY_OBSERVATION: Duration = Duration::from_secs(4);
const LIVE_METADATA_READY: &str = "player.program_status === 'available' && JSON.parse(player.current_program_data) !== null && JSON.parse(player.live_timeline).viewing.utc !== null";

#[derive(Clone, Copy)]
enum Traffic {
    Broadcast,
    CapacityPressure,
}

struct Server {
    url: String,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new(traffic: Traffic) -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let url = format!("http://{}", listener.local_addr()?);
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let worker = thread::spawn(move || {
            let mut clients = Vec::new();
            while !stopped.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((socket, _)) => {
                        let stopped = stopped.clone();
                        clients.push(thread::spawn(move || {
                            let _ = serve(socket, &stopped, traffic);
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
fn serve(mut socket: TcpStream, stopped: &AtomicBool, traffic: Traffic) -> std::io::Result<()> {
    const MAX_HEADER_BYTES: usize = 8 * 1024;
    socket.set_read_timeout(Some(NETWORK_TIMEOUT))?;
    socket.set_write_timeout(Some(NETWORK_TIMEOUT))?;
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") && header.len() < MAX_HEADER_BYTES {
        let mut byte = [0];
        socket.read_exact(&mut byte)?;
        header.extend(byte);
    }
    if header.starts_with(b"GET /api/services/1/stream ") {
        let ts = include_bytes!("../../../tests/fixtures/recording-seek.ts");
        const PACKET: usize = crate::transport::wire::TS_PACKET_SIZE;
        let chunk_size = ts.len() / FIXTURE_SECONDS / INTERVALS_PER_SECOND / PACKET * PACKET;
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
            ts.len() + ts.len().div_ceil(chunk_size) * padding.len()
        )?;
        for chunk in ts.chunks(chunk_size) {
            if stopped.load(Ordering::Acquire) {
                break;
            }
            socket.write_all(chunk)?;
            socket.write_all(&padding)?;
            thread::sleep(SEND_INTERVAL);
        }
    } else {
        let body = if header.starts_with(b"GET /api/services ") {
            r#"[{"id":1,"serviceId":1,"networkId":1,"name":"Timeshift fixture","type":1,"channel":{"type":"GR"}}]"#
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
        assert!(evaluate(
            engine,
            &format!(
                "player.configure_timeshift_options('{backend}', {MEMORY_MIB}, {FILESYSTEM_MIB}, {RETENTION_MINUTES}); player.select(0); player.play(); true"
            )
        )?);
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
        assert!(evaluate(engine, "player.return_to_live()")?);
        wait_for(
            app,
            engine,
            &format!(
                "player.playing && !player.paused && !player.seeking && player.live_delay_ms < {}",
                LIVE_EDGE_TOLERANCE.as_millis()
            ),
        )?;
        assert!(evaluate(
            engine,
            &format!("{OBSERVER}.previousSession = JSON.parse(player.live_timeline).session; true")
        )?);
        assert!(evaluate(
            engine,
            "player.stop(); player.live_timeline === 'null' && !player.timeshift && !player.media_active && player.timeshift_bytes_per_second === 0"
        )?);
        if backend == "filesystem" {
            let cache = std::path::PathBuf::from(
                std::env::var_os("XDG_CACHE_HOME").ok_or("isolated cache")?,
            )
            .join("nagametv/timeshift");
            let deadline = std::time::Instant::now() + NETWORK_TIMEOUT;
            while std::fs::read_dir(&cache)?
                .any(|entry| entry.map_or(true, |entry| entry.file_name() != "registry.lock"))
            {
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
    assert!(evaluate(
        engine,
        "player.configure_timeshift('off'); player.select(0); player.play(); true"
    )?);
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
            "!player.timeshift && !player.seekable && !player.transport_error.length && ({LIVE_METADATA_READY})"
        ),
    )?;
    assert!(evaluate(engine, "!player.pause()")?);
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
            "!player.timeshift && !player.seekable && !player.transport_error.length && ({LIVE_METADATA_READY})"
        ),
    )?;
    evaluate(engine, "player.stop(); true")?;
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
            "player.playing && player.seekable && player.position_ms > 0",
        )?;
        wait_for(
            app,
            engine,
            "JSON.parse(player.live_timeline).viewing !== null && JSON.parse(player.live_timeline).viewing.program !== null",
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
    assert!(evaluate(
        engine,
        &format!("{OBSERVER}.failure.length === 0")
    )?);
    evaluate(engine, &format!("{OBSERVER}.destroy(); true"))?;
    Ok(())
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
