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
    time::Duration,
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

struct Server {
    url: String,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new() -> std::io::Result<Self> {
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
                            let _ = serve(socket, &stopped);
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
fn serve(mut socket: TcpStream, stopped: &AtomicBool) -> std::io::Result<()> {
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
        write!(
            socket,
            "HTTP/1.1 200 OK\r\nContent-Type: video/mp2t\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            ts.len()
        )?;
        for chunk in ts.chunks(chunk_size) {
            if stopped.load(Ordering::Acquire) {
                break;
            }
            socket.write_all(chunk)?;
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
    let server = Server::new()?;
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
                "player.timeshift && player.playing && player.seekable && player.duration_ms > {}",
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
        assert!(evaluate(engine, "player.pause()")?);
        wait_for(app, engine, "player.paused && !player.seeking")?;
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
            &format!("player.seek_to({})", REWIND_TARGET.as_millis())
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
            "player.stop(); !player.timeshift && !player.media_active"
        )?);
        if backend == "filesystem" {
            let cache = std::path::PathBuf::from(
                std::env::var_os("XDG_CACHE_HOME").ok_or("isolated cache")?,
            )
            .join("mirakurun-viewer/timeshift");
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
        "player.playing && !player.timeshift && !player.seekable && JSON.parse(player.video_stats()).rendered > 0",
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
        "player.playing && !player.paused && !player.timeshift && !player.seekable",
    )?;
    evaluate(engine, "player.stop(); true")?;
    eprintln!(
        "Timeshift disabled: common live input plays and rejects pause; enable/disable applies during live playback and pause"
    );
    Ok(())
}
