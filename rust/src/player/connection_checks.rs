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
        assert!(*player.loading());
        player.pin_mut().poll_channels()?;
        self.received.recv_timeout(Duration::from_secs(5))?;
        Ok(())
    }
    fn finish(self, player: &mut cxx::UniquePtr<ffi::Player>) -> TestResult {
        self.release.send(())?;
        self.worker.join().map_err(|_| "HTTP fixture panicked")??;
        let deadline = Instant::now() + Duration::from_secs(5);
        while *player.loading() {
            assert!(Instant::now() < deadline, "connection did not finish");
            player.pin_mut().poll_channels()?;
            thread::sleep(Duration::from_millis(1));
        }
        assert!(!player.rust().connection_pending);
        Ok(())
    }
}

fn saved_server(path: &Path) -> String {
    settings::Session::open(path.to_owned())
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
    player.pin_mut().rust_mut().preferences = settings::Session::open(path.clone())?;
    assert!(
        player.rust().playback.is_none(),
        "test must not initialize playback"
    );
    assert!(player.server().is_empty());
    let outcomes = Arc::new(Mutex::new(Vec::new()));
    let recorded = outcomes.clone();
    let _signal = player
        .pin_mut()
        .on_connection_finished(move |_, success, count| {
            recorded.lock().unwrap().push((success, count));
        });

    // A pending candidate must never leak into unrelated preference saves.
    let response = Response::new(503, "unavailable")?;
    response.begin(&mut player)?;
    assert!(!path.exists());
    assert!(outcomes.lock().unwrap().is_empty());
    player
        .pin_mut()
        .rust_mut()
        .preferences
        .preferences_mut()
        .volume = 42.0.into();
    player.pin_mut().save_settings();
    assert_eq!(saved_server(&path), "");
    response.finish(&mut player)?;
    player.pin_mut().save_settings();
    assert_eq!(saved_server(&path), "");
    assert_eq!(*outcomes.lock().unwrap(), [(false, 0)]);
    assert!(!player.rust().channel_refresh.enabled());

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
        assert_eq!(outcomes.lock().unwrap().last(), Some(&(true, count)));
        assert!(!*player.playing());
        assert!(!*player.connecting());
    }
    let good_server = saved_server(&path);

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
    assert_eq!(saved_server(&path), expected);
    println!(
        "Connection checks passed: pending/failed saves, empty catalog, success, save failure, shutdown"
    );
    Ok(())
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
