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
        player.pin_mut().poll_channels();
        self.received.recv_timeout(Duration::from_secs(5))?;
        Ok(())
    }
    fn finish(self, player: &mut cxx::UniquePtr<ffi::Player>) -> TestResult {
        self.release.send(())?;
        self.worker.join().map_err(|_| "HTTP fixture panicked")??;
        let deadline = Instant::now() + Duration::from_secs(5);
        while player.loading() || player.rust().request.is_busy() {
            assert!(Instant::now() < deadline, "connection did not finish");
            player.pin_mut().poll_channels();
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
        .set(features::LaunchPlan::Restricted("none".parse()?))
        .map_err(|_| "test plan already initialized")?;
    let app = QCoreApplication::new();
    assert!(!app.is_null());
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("settings.toml");
    let mut player = ffi::new_player();
    assert_eq!(
        player.build_info(),
        QString::from(crate::build_info::json())
    );
    player.pin_mut().rust_mut().preferences =
        settings::Loaded::open(path.clone())?.activate(None, None);
    assert!(
        player.rust().media.playback().is_none(),
        "test must not initialize playback"
    );
    // Preference notifications must observe the committed value; this touches
    // no decoder even when the feature is enabled for this settings-only check.
    player.pin_mut().rust_mut().subtitles_enabled = true;
    let outline_changes = Arc::new(Mutex::new(Vec::new()));
    let observed_outline = outline_changes.clone();
    let _outline_signal = player
        .pin_mut()
        .on_subtitle_force_outline_changed(move |player| {
            assert_eq!(
                player.subtitle_force_outline(),
                player
                    .rust()
                    .preferences
                    .preferences()
                    .subtitle_force_outline
            );
            observed_outline
                .lock()
                .unwrap()
                .push(player.subtitle_force_outline());
        });
    for value in [true, true, false] {
        player.pin_mut().configure_subtitle_outline(value);
    }
    assert_eq!(*outline_changes.lock().unwrap(), [true, false]);
    assert!(
        !settings::Loaded::open(path.clone())?
            .preferences()
            .subtitle_force_outline
    );
    player.pin_mut().rust_mut().subtitles_enabled = false;
    std::fs::remove_file(&path)?;
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
    check_viewing_channel()?;
    crate::channel_model::checks::run()?;
    super::recording_library_checks::run()?;
    check_playback_actions(&mut player)?;
    check_recording_input(&mut player)?;
    check_recording_notifications(&mut player)?;
    check_guide_state();
    check_autoplay()?;
    check_live_buffer()?;
    check_timeshift_options()?;
    check_transport_messages();
    check_screenshot_directory()?;
    check_comment_presentation()?;
    super::remote_checks::run()?;

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

    const SAVED_WINDOW_WIDTH: i32 = 850;
    const SAVED_WINDOW_HEIGHT: i32 = 610;
    assert!(
        player
            .pin_mut()
            .remember_window_size(SAVED_WINDOW_WIDTH, SAVED_WINDOW_HEIGHT)
    );
    assert!(
        !player
            .pin_mut()
            .remember_window_size(0, SAVED_WINDOW_HEIGHT)
    );
    assert!(
        !player
            .pin_mut()
            .remember_window_size(SAVED_WINDOW_WIDTH, -1)
    );
    assert_eq!(
        settings::Loaded::open(path.clone())?
            .preferences()
            .window_size,
        settings::WindowSize::checked(SAVED_WINDOW_WIDTH, SAVED_WINDOW_HEIGHT)
    );

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

fn check_transport_messages() {
    use super::transport::{Message, NOTICE_DURATION};
    use crate::playback::timeline::Notice;
    use cxx_qt_lib::QQmlApplicationEngine;

    let mut engine = QQmlApplicationEngine::new();
    assert!(crate::qt::ffi::initialize_ui_language(
        engine.pin_mut(),
        &"en".into()
    ));
    let mut player = ffi::new_player();
    assert!(player.rust().media.playback().is_none());
    assert!(player.transport_error().is_empty());
    assert_eq!(player.playback_rate(), 10);
    assert_eq!(player.requested_playback_rate(), 10);
    assert_eq!(player.minimum_playback_rate(), 5);
    assert_eq!(player.maximum_playback_rate(), 20);
    assert!(!player.speed_available());
    assert!(!player.at_live_edge());
    for rate in [0, 4, 11, 21] {
        assert!(!player.pin_mut().set_playback_rate(rate));
        assert_eq!(player.playback_rate(), 10);
        assert_eq!(player.requested_playback_rate(), 10);
    }
    player.pin_mut().set_transport_message(Message::None);
    let observed = Arc::new(Mutex::new(Vec::new()));
    let messages = observed.clone();
    let _signal = player.pin_mut().on_transport_error_changed(move |player| {
        messages
            .lock()
            .unwrap()
            .push(player.transport_error().to_string());
    });
    for (notice, english, japanese) in [
        (
            Notice::CaughtUp,
            "Caught up with live playback. Returned to normal speed.",
            "ライブに追いついたため、等速に戻しました。",
        ),
        (
            Notice::ReceptionStalled,
            "Reception is waiting. Returned to normal speed.",
            "受信待ちになったため、等速に戻しました。",
        ),
        (
            Notice::Expired,
            "The playback position expired and was moved into the retained range.",
            "保持期限を過ぎたため、再生位置を保持範囲内へ移動しました",
        ),
        (
            Notice::SettingsClamped,
            "Timeshift settings changed. The playback position was moved into the retained range.",
            "タイムシフト設定の変更により、再生位置を保持範囲内へ移動しました",
        ),
        (
            Notice::SettingsReturnedToLive,
            "Timeshift settings changed. Playback returned to the live edge.",
            "タイムシフト設定の変更により、ライブの最新位置へ戻りました",
        ),
    ] {
        observed.lock().unwrap().clear();
        let shown_at = Instant::now();
        player
            .pin_mut()
            .set_transport_message(Message::notice(notice, shown_at));
        assert_eq!(player.transport_error(), QString::from(english));
        assert!(player.pin_mut().request_language("ja".into()));
        assert_eq!(player.transport_error(), QString::from(japanese));
        assert!(player.pin_mut().request_language("en".into()));
        assert_eq!(player.transport_error(), QString::from(english));
        assert_eq!(*observed.lock().unwrap(), [english, japanese, english]);
        let deadline = shown_at + NOTICE_DURATION;
        player
            .pin_mut()
            .expire_transport_notice(deadline - Duration::from_nanos(1));
        assert_eq!(player.transport_error(), QString::from(english));
        player.pin_mut().expire_transport_notice(deadline);
        assert!(player.transport_error().is_empty());
        assert_eq!(*observed.lock().unwrap(), [english, japanese, english, ""]);
        player
            .pin_mut()
            .expire_transport_notice(deadline + NOTICE_DURATION);
        assert_eq!(*observed.lock().unwrap(), [english, japanese, english, ""]);
    }

    // Repeating the same notice replaces its deadline as well as its content.
    let shown_at = Instant::now();
    player
        .pin_mut()
        .set_transport_message(Message::notice(Notice::Expired, shown_at));
    let repeated_at = shown_at + NOTICE_DURATION / 2;
    player
        .pin_mut()
        .set_transport_message(Message::notice(Notice::Expired, repeated_at));
    player
        .pin_mut()
        .expire_transport_notice(shown_at + NOTICE_DURATION);
    assert!(!player.transport_error().is_empty());
    player
        .pin_mut()
        .expire_transport_notice(repeated_at + NOTICE_DURATION);
    assert!(player.transport_error().is_empty());

    player
        .pin_mut()
        .set_transport_message(Message::notice(Notice::Expired, shown_at));
    // Even a diagnostic that happens to match a translation key stays literal.
    player
        .pin_mut()
        .set_transport_message(Message::Failure("Paused".into()));
    assert!(player.pin_mut().request_language("ja".into()));
    // An old notice deadline must not clear a later error.
    player
        .pin_mut()
        .expire_transport_notice(shown_at + NOTICE_DURATION);
    assert_eq!(player.transport_error(), QString::from("Paused"));
    player.pin_mut().set_transport_message(Message::None);
    assert!(player.pin_mut().request_language("en".into()));
    assert!(player.transport_error().is_empty());
    assert!(player.pin_mut().shutdown());
    println!(
        "Transport notices retranslate, expire once, renew on repetition and preserve later errors"
    );
}

fn check_stream_state(player: &mut cxx::UniquePtr<ffi::Player>) -> TestResult {
    use super::stream_state::{Attempt, State};
    use crate::playback::input::Retention;
    let observed = Arc::new(Mutex::new(Vec::new()));
    let transport = Arc::new(Mutex::new(Vec::new()));
    let transport_observer = transport.clone();
    let _transport_signal = player.pin_mut().on_timeshift_changed(move |player| {
        transport_observer.lock().unwrap().push((
            player.timeshift(),
            player.playing(),
            player.media_active(),
            player.recording(),
        ));
    });
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
    let attempt = Attempt::new(
        &player.rust().catalog.channels()[0],
        Retention::Memory.into(),
    );
    player
        .pin_mut()
        .update_stream_state(State::Connecting(attempt));
    let attempt = player
        .pin_mut()
        .rust_mut()
        .stream_state
        .take_retry()
        .unwrap();
    player
        .pin_mut()
        .update_stream_state(State::Connecting(attempt).started());
    player.pin_mut().play();
    assert!(player.playing());
    assert!(
        player
            .pin_mut()
            .rust_mut()
            .stream_state
            .take_retry()
            .is_none()
    );
    player.pin_mut().end_stream()?;
    assert_eq!(
        *observed.lock().unwrap(),
        [(true, false), (false, true), (false, true), (false, false)]
    );
    assert_eq!(
        *transport.lock().unwrap(),
        [(true, true, true, false), (false, false, false, false)]
    );
    Ok(())
}

fn check_viewing_channel() -> TestResult {
    use super::stream_state::{Attempt, State};
    use crate::playback::{input::Retention, timeline::Phase};
    const TWO_CHANNELS: &str = r#"[
        {"id":1,"name":"First","type":1,"remoteControlKeyId":1,"channel":{"type":"GR"}},
        {"id":18446744073709551615,"name":"Viewed","type":1,"remoteControlKeyId":2,"channel":{"type":"GR"}}
    ]"#;
    const VIEWED_ONLY: &str = r#"[
        {"id":18446744073709551615,"name":"Viewed","type":1,"remoteControlKeyId":2,"channel":{"type":"GR"}}
    ]"#;
    let directory = tempfile::tempdir()?;
    let mut player = ffi::new_player();
    player.pin_mut().rust_mut().preferences =
        settings::Loaded::open(directory.path().join("settings.toml"))?.activate(None, None);
    let response = Response::new(200, TWO_CHANNELS)?;
    response.begin(&mut player)?;
    response.finish(&mut player)?;
    assert_eq!(player.viewing_channel(), -1);

    let observed = Arc::new(Mutex::new(Vec::new()));
    let changes = observed.clone();
    let _signal = player.pin_mut().on_viewing_channel_changed(move |p| {
        let index = p.viewing_channel();
        if index >= 0 {
            assert!(p.media_active() && !p.recording());
            assert_eq!(p.rust().catalog.channels()[index as usize].id, u64::MAX);
            let row = p
                .rust()
                .channel_model
                .row(index)
                .value::<cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>>()
                .unwrap();
            assert_eq!(
                row.get(&QString::from("label"))
                    .unwrap()
                    .value::<QString>()
                    .unwrap()
                    .to_string(),
                p.rust().catalog.channels()[index as usize].label
            );
        }
        changes.lock().unwrap().push(index);
    });
    let attempt = Attempt::new(
        &player.rust().catalog.channels()[1],
        Retention::Memory.into(),
    );
    player
        .pin_mut()
        .update_stream_state(State::Connecting(attempt));
    assert_eq!(player.viewing_channel(), -1);
    player.pin_mut().change_stream_state(State::started);
    assert_eq!(player.viewing_channel(), 1);
    player.pin_mut().set_selected(0);
    assert_eq!(
        player.viewing_channel(),
        1,
        "saved selection is not the viewed input"
    );
    player
        .pin_mut()
        .change_stream_state(|state| state.transport(Phase::Paused));
    assert_eq!(
        player.viewing_channel(),
        1,
        "paused live viewing retains its indicator"
    );

    for (body, expected) in [(VIEWED_ONLY, 0), ("[]", -1), (VIEWED_ONLY, 0)] {
        let response = Response::new(200, body)?;
        // Refresh the catalog through its real HTTP/publication path without
        // reconfiguring the active stream. Each fixture owns its local endpoint.
        let server = crate::services::ServerUrl::parse(&response.url)?;
        player.pin_mut().rust_mut().request.request(server);
        player.pin_mut().poll_channels();
        response.received.recv_timeout(Duration::from_secs(5))?;
        response.finish(&mut player)?;
        assert_eq!(player.viewing_channel(), expected);
    }
    player.pin_mut().end_stream()?;
    assert_eq!(player.viewing_channel(), -1);
    assert_eq!(*observed.lock().unwrap(), [1, 0, -1, 0, -1]);

    let file = crate::playback::recording::Recording::open(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../tests/fixtures/subtitle-clock.ts"
    )))?;
    player
        .pin_mut()
        .update_stream_state(State::Connecting(Attempt::File(file)).started());
    assert!(player.media_active());
    assert_eq!(player.viewing_channel(), -1);
    assert!(player.pin_mut().shutdown());
    println!("Viewed channel follows live input identity across pause and catalog replacement");
    Ok(())
}

fn check_playback_actions(player: &mut cxx::UniquePtr<ffi::Player>) -> TestResult {
    use super::stream_state::{Attempt, State};
    use crate::playback::input::Retention;
    let selected = player.selected();
    let observed = Arc::new(Mutex::new(Vec::new()));
    let changes = observed.clone();
    let _signal = player.pin_mut().on_playback_action_changed(move |p| {
        let expected: ffi::PlaybackAction = p
            .rust()
            .stream_state
            .playback_action(p.selected() >= 0)
            .into();
        assert!(p.playback_action() == expected);
        changes
            .lock()
            .unwrap()
            .push((p.playback_action().repr, p.playing()));
    });
    let restored_catalog = player.rust().catalog.clone();
    let server = player.server().to_string();
    player
        .pin_mut()
        .replace_catalog(crate::channels::catalog::Catalog::default(), "");
    assert!(player.playback_action() == ffi::PlaybackAction::Unavailable);
    player.pin_mut().toggle_playback();
    assert!(!player.connecting());
    player.pin_mut().replace_catalog(restored_catalog, &server);
    assert!(player.playback_action() == ffi::PlaybackAction::Play);
    // A previously displayed Play action must use the new state when triggered.
    let attempt = Attempt::new(
        &player.rust().catalog.channels()[selected as usize],
        Retention::Off.into(),
    );
    player
        .pin_mut()
        .update_stream_state(State::Connecting(attempt).started());
    assert!(player.playback_action() == ffi::PlaybackAction::Stop);
    player.pin_mut().toggle_playback();
    assert!(!player.playing() && !player.connecting());
    assert_eq!(
        *observed.lock().unwrap(),
        [
            (ffi::PlaybackAction::Unavailable.repr, false),
            (ffi::PlaybackAction::Play.repr, false),
            (ffi::PlaybackAction::Stop.repr, true),
            (ffi::PlaybackAction::Play.repr, false),
        ]
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

fn check_timeshift_options() -> TestResult {
    use crate::playback::input::{Limits, Policy, Retention};
    const MEMORY_MIB: i32 = 64;
    const FILESYSTEM_MIB: i32 = 512;
    const MINUTES: i32 = 10;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("settings.toml");
    let mut player = ffi::new_player();
    player.pin_mut().rust_mut().preferences =
        settings::Loaded::open(path.clone())?.activate(None, None);
    let observed = Arc::new(Mutex::new(Vec::new()));
    let snapshots = observed.clone();
    let _signal = player
        .pin_mut()
        .on_timeshift_storage_changed(move |player| {
            let limits: serde_json::Value =
                serde_json::from_str(&player.timeshift_limits().to_string()).unwrap();
            snapshots.lock().unwrap().push((
                player.timeshift_storage().to_string(),
                limits["memory_mib"].as_i64().unwrap(),
            ));
        });
    assert!(!player.pin_mut().configure_timeshift_options(
        QString::from("off"),
        -1,
        FILESYSTEM_MIB,
        MINUTES
    ));
    assert_eq!(player.timeshift_storage().to_string(), "off");
    assert!(observed.lock().unwrap().is_empty());
    assert!(player.pin_mut().configure_timeshift_options(
        QString::from("filesystem"),
        MEMORY_MIB,
        FILESYSTEM_MIB,
        MINUTES
    ));
    assert_eq!(
        *observed.lock().unwrap(),
        [("filesystem".to_owned(), i64::from(MEMORY_MIB))]
    );
    let expected = Policy::new(
        Retention::Filesystem,
        Limits::new(MEMORY_MIB as u32, FILESYSTEM_MIB as u32, MINUTES as u32).unwrap(),
    );
    assert_eq!(
        settings::Loaded::open(path)?
            .preferences()
            .timeshift_policy(),
        expected
    );
    assert!(!player.loading() && !player.connecting() && !player.playing());
    Ok(())
}

fn check_live_buffer() -> TestResult {
    use crate::settings::LiveBuffer;
    const CUSTOM_MS: i32 = 150;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("settings.toml");
    let mut player = ffi::new_player();
    player.pin_mut().rust_mut().preferences =
        settings::Loaded::open(path.clone())?.activate(None, None);
    let observed = Arc::new(Mutex::new(Vec::new()));
    let snapshots = observed.clone();
    let _signal = player
        .pin_mut()
        .on_live_buffer_options_changed(move |player| {
            let options: serde_json::Value =
                serde_json::from_str(&player.live_buffer_options().to_string()).unwrap();
            assert_eq!(
                options["milliseconds"].as_i64(),
                Some(i64::from(
                    player
                        .rust()
                        .preferences
                        .preferences()
                        .live_buffer_ms
                        .milliseconds()
                ))
            );
            snapshots
                .lock()
                .unwrap()
                .push(options["milliseconds"].as_i64().unwrap());
        });
    for invalid in [-1, 0, LiveBuffer::MAX_MS + 1] {
        assert!(!player.pin_mut().configure_live_buffer(invalid));
    }
    assert!(observed.lock().unwrap().is_empty());
    assert!(player.pin_mut().configure_live_buffer(CUSTOM_MS));
    assert!(player.pin_mut().configure_live_buffer(CUSTOM_MS));
    assert_eq!(*observed.lock().unwrap(), [i64::from(CUSTOM_MS)]);
    assert_eq!(
        settings::Loaded::open(path)?
            .preferences()
            .live_buffer_ms
            .milliseconds(),
        CUSTOM_MS
    );
    assert!(!player.playing() && !player.connecting());
    Ok(())
}

fn check_recording_input(player: &mut cxx::UniquePtr<ffi::Player>) -> TestResult {
    use super::stream_state::{Attempt, State};
    let channel = &player.rust().catalog.channels()[0];
    let attempt = Attempt::new(channel, Default::default());
    let service = channel.id;
    player
        .pin_mut()
        .update_stream_state(State::Connecting(attempt).started());
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("invalid.ts");
    std::fs::write(&path, b"not a transport stream")?;
    let url = cxx_qt_lib::QUrl::from_local_file(&QString::from(path.to_string_lossy().as_ref()));
    assert!(player.pin_mut().open_recording(url));
    assert!(player.recording_loading());
    assert!(player.file_error().is_empty());
    let deadline = Instant::now() + Duration::from_secs(5);
    while player.recording_loading() {
        assert!(Instant::now() < deadline);
        player.pin_mut().poll_recording();
        thread::sleep(Duration::from_millis(1));
    }
    assert!(player.playing());
    assert_eq!(player.rust().stream_state.active_service(), Some(service));
    assert!(!player.recording());
    assert!(!player.file_error().is_empty());
    assert!(!player.pin_mut().open_recording_transfer(QString::default()));
    assert!(!player.recording_loading());
    assert!(player.playing());
    assert_eq!(player.rust().stream_state.active_service(), Some(service));
    player.pin_mut().end_stream()?;
    Ok(())
}

fn check_recording_notifications(player: &mut cxx::UniquePtr<ffi::Player>) -> TestResult {
    use super::stream_state::{Attempt, State};
    let file = crate::playback::recording::Recording::open(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../tests/fixtures/subtitle-clock.ts"
    )))?;
    let observed = Arc::new(Mutex::new(Vec::new()));
    let sample = |player: std::pin::Pin<&mut ffi::Player>| {
        (
            player.recording(),
            player.recording_name().to_string(),
            player.connecting(),
            player.playing(),
        )
    };
    let changes = observed.clone();
    let _source = player.pin_mut().on_recording_changed(move |player| {
        changes.lock().unwrap().push(sample(player));
    });
    let changes = observed.clone();
    let _name = player.pin_mut().on_recording_name_changed(move |player| {
        changes.lock().unwrap().push(sample(player));
    });
    let changes = observed.clone();
    let _connecting = player.pin_mut().on_connecting_changed(move |player| {
        changes.lock().unwrap().push(sample(player));
    });
    player
        .pin_mut()
        .update_stream_state(State::Connecting(Attempt::File(file)));
    assert_eq!(
        observed.lock().unwrap().as_slice(),
        &vec![(true, "subtitle-clock.ts".to_owned(), true, false); 3]
    );
    player.pin_mut().change_stream_state(State::started);
    let transport = Arc::new(Mutex::new(Vec::new()));
    let changes = transport.clone();
    let _paused = player.pin_mut().on_paused_changed(move |p| {
        changes.lock().unwrap().push((
            p.playing(),
            p.paused(),
            p.seeking(),
            p.ended(),
            p.media_active(),
        ));
    });
    let changes = transport.clone();
    let _seeking = player.pin_mut().on_seeking_changed(move |p| {
        changes.lock().unwrap().push((
            p.playing(),
            p.paused(),
            p.seeking(),
            p.ended(),
            p.media_active(),
        ));
    });
    use crate::playback::timeline::{Phase, Resume};
    player
        .pin_mut()
        .change_stream_state(|state| state.transport(Phase::Seeking(Resume::Paused)));
    assert_eq!(
        transport.lock().unwrap().as_slice(),
        &[(false, true, true, false, true); 2]
    );
    transport.lock().unwrap().clear();
    player.pin_mut().end_stream()?;
    assert_eq!(
        transport.lock().unwrap().as_slice(),
        &[(false, false, false, false, false); 2]
    );
    assert!(
        player.recording(),
        "stop retains replay target in the same state"
    );
    assert_eq!(player.recording_name().to_string(), "subtitle-clock.ts");
    observed.lock().unwrap().clear();
    let attempt = Attempt::new(&player.rust().catalog.channels()[0], Default::default());
    player
        .pin_mut()
        .update_stream_state(State::Connecting(attempt));
    assert_eq!(
        observed.lock().unwrap().as_slice(),
        &vec![(false, String::new(), true, false); 3]
    );
    player.pin_mut().end_stream()?;
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
    let finished = Arc::new(Mutex::new(Vec::new()));
    let results = finished.clone();
    let _finished = player
        .pin_mut()
        .on_screenshot_finished(move |player, file| {
            assert_eq!(file.is_empty(), !player.screenshot_error().is_empty());
            results.lock().unwrap().push(file.clone());
        });
    let formats = Arc::new(Mutex::new(Vec::new()));
    let changes = formats.clone();
    let _format = player
        .pin_mut()
        .on_screenshot_format_changed(move |player| {
            assert_eq!(
                player.screenshot_format().to_string(),
                player
                    .rust()
                    .preferences
                    .preferences()
                    .screenshot_format
                    .key()
            );
            changes
                .lock()
                .unwrap()
                .push(player.screenshot_format().to_string());
        });
    let save = |player: &mut cxx::UniquePtr<ffi::Player>| {
        assert!(player.pin_mut().save_screenshot(&image));
        assert!(player.screenshot_busy());
        let deadline = Instant::now() + Duration::from_secs(5);
        while player.screenshot_busy() {
            assert!(
                Instant::now() < deadline,
                "screenshot worker did not finish"
            );
            player.pin_mut().poll_screenshot();
            thread::sleep(Duration::from_millis(1));
        }
        finished.lock().unwrap().pop().expect("completion signal")
    };
    assert_eq!(player.screenshot_format().to_string(), "png");
    for format in ["png", "jpg", "webp"] {
        assert!(
            player
                .pin_mut()
                .configure_screenshot_format(QString::from(format))
        );
        let saved = save(&mut player)
            .to_local_file()
            .expect("saved local image");
        let bytes = std::fs::read(saved.to_string())?;
        let decoded = QImage::from_data(&bytes, Some(format)).expect("encoded image");
        assert_eq!(decoded.size(), image.size());
        assert!(saved.to_string().ends_with(&format!(".{format}")));
        assert!(player.screenshot_error().is_empty());
        std::fs::remove_file(saved.to_string())?;
    }
    assert_eq!(*formats.lock().unwrap(), ["jpg", "webp"]);
    let option_changes = Arc::new(Mutex::new(0));
    let observed = option_changes.clone();
    let _options = player
        .pin_mut()
        .on_screenshot_options_changed(move |player| {
            let exposed: serde_json::Value =
                serde_json::from_str(&player.screenshot_options().to_string()).unwrap();
            let current = player.rust().preferences.preferences().screenshot_options;
            assert_eq!(exposed["png_compression"], current.png_compression.value());
            assert_eq!(exposed["jpg_quality"], current.jpg_quality.value());
            assert_eq!(exposed["webp_quality"], current.webp_quality.value());
            *observed.lock().unwrap() += 1;
        });
    for (format, value, lossless) in [("png", 3, false), ("jpg", 82, false), ("webp", 76, true)] {
        assert!(player.pin_mut().configure_screenshot_options(
            QString::from(format),
            value,
            lossless
        ));
    }
    let previous = player.screenshot_options();
    assert!(
        !player
            .pin_mut()
            .configure_screenshot_options(QString::from("png"), 10, false)
    );
    assert!(
        !player
            .pin_mut()
            .configure_screenshot_options(QString::from("webp"), 100, false)
    );
    assert_eq!(player.screenshot_options(), previous);
    assert_eq!(*option_changes.lock().unwrap(), 3);
    assert!(
        !player
            .pin_mut()
            .configure_screenshot_format(QString::from("gif"))
    );
    restored.pin_mut().rust_mut().preferences =
        settings::Loaded::open(temporary.path().join("settings.toml"))?.activate(None, None);
    assert_eq!(restored.screenshot_format().to_string(), "webp");
    assert_eq!(restored.screenshot_options(), player.screenshot_options());
    // Losing the destination reports an error; the next request can recover.
    std::fs::remove_dir(&directory)?;
    std::fs::write(&directory, b"keep")?;
    assert!(save(&mut player).is_empty());
    assert!(!player.screenshot_error().is_empty());
    assert_eq!(std::fs::read(&directory)?, b"keep");
    std::fs::remove_file(&directory)?;
    assert!(!save(&mut player).is_empty());
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

fn check_comment_presentation() -> TestResult {
    let mut player = ffi::new_player();
    player.pin_mut().rust_mut().preferences =
        settings::Loaded::transient(settings::Preferences::default()).activate(None, None);
    let changes = Arc::new(Mutex::new(Vec::new()));
    let observed = changes.clone();
    let _display = player.pin_mut().on_comment_display_changed(move |player| {
        observed.lock().unwrap().push((
            player.comment_display().to_string(),
            player.comment_placement().to_string(),
        ));
    });
    let observed = changes.clone();
    let _placement = player
        .pin_mut()
        .on_comment_placement_changed(move |player| {
            observed.lock().unwrap().push((
                player.comment_display().to_string(),
                player.comment_placement().to_string(),
            ));
        });
    assert!(
        player
            .pin_mut()
            .configure_comment_presentation("pop".into(), "random".into())
    );
    assert_eq!(
        *changes.lock().unwrap(),
        [
            ("pop".into(), "random".into()),
            ("pop".into(), "random".into())
        ]
    );
    assert!(
        !player
            .pin_mut()
            .configure_comment_presentation("pop".into(), "collision".into())
    );
    assert!(
        !player
            .pin_mut()
            .configure_comment_presentation("unknown".into(), "random".into())
    );
    assert_eq!(player.comment_display().to_string(), "pop");
    assert_eq!(player.comment_placement().to_string(), "random");
    assert!(
        player
            .pin_mut()
            .configure_comment_presentation("pop".into(), "random".into())
    );
    assert_eq!(changes.lock().unwrap().len(), 2);
    assert_eq!(
        player.evaluation_collision_layout(),
        cfg!(feature = "evaluation-collision-layout")
    );
    assert_eq!(
        player.evaluation_comment_list(),
        cfg!(feature = "evaluation-comment-list")
    );
    assert_eq!(
        player.evaluation_wide_comments(),
        cfg!(feature = "evaluation-wide-comments")
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
