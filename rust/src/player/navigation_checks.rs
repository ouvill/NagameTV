//! Hardware-free checks use real request validation and logical session ownership.
use super::ffi;
use crate::{features::program_info::guide::Guide, screen_navigation::Screen, settings};
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QUrl};
use std::{
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;
fn assert_screen(player: &ffi::Player, screen: Screen, back: Option<Screen>) {
    assert_eq!(player.screen(), screen as i32);
    assert_eq!(
        player.back_screen(),
        back.map_or(-1, |screen| screen as i32)
    );
    assert_eq!(player.guide_is_open(), screen == Screen::Guide);
}
fn observe(player: &ffi::Player) {
    assert_eq!(
        player.guide_is_open(),
        player.screen() == Screen::Guide as i32
    );
    assert!(player.screen() != Screen::Watching as i32 || player.watching_available());
}

pub(super) fn run() -> TestResult {
    guide_requests()?;
    let directory = tempfile::tempdir()?;
    let mut player = ffi::new_player();
    // The native test launch plan has no feature entries; navigation is still active.
    assert_screen(&player, Screen::Home, None);
    assert!(!player.watching_available());
    assert!(!player.pin_mut().open_screen(Screen::Watching as i32));
    assert!(!player.pin_mut().open_screen(Screen::Guide as i32));
    assert!(player.pin_mut().open_screen(Screen::Recordings as i32));
    assert_screen(&player, Screen::Recordings, Some(Screen::Home));
    assert!(player.pin_mut().go_back());
    player.pin_mut().rust_mut().epg_enabled = true;
    let changes = Arc::new(Mutex::new(Vec::new()));
    let observed = changes.clone();
    let _screen = player.pin_mut().on_screen_changed(move |player| {
        observe(&player);
        observed
            .lock()
            .unwrap()
            .push((player.screen(), player.back_screen()));
    });
    let _guide = player.pin_mut().on_screen_changed(move |mut player| {
        observe(&player);
        if player.screen() == Screen::Guide as i32 {
            if matches!(player.rust().guide, Guide::AwaitingDay) {
                player.as_mut().guide_day(0.0, 86_400_000.0);
            }
            assert!(matches!(player.rust().guide, Guide::Showing(_)));
        }
    });
    let _watching = player
        .pin_mut()
        .on_watching_available_changed(move |player| observe(&player));

    assert!(!player.pin_mut().go_back());
    assert!(player.pin_mut().open_screen(Screen::Guide as i32));
    assert_screen(&player, Screen::Guide, Some(Screen::Home));
    assert!(matches!(player.rust().guide, Guide::Showing(_)));
    let count = changes.lock().unwrap().len();
    assert!(player.pin_mut().open_screen(Screen::Guide as i32));
    assert_eq!(changes.lock().unwrap().len(), count);
    assert!(player.pin_mut().go_back());
    assert_screen(&player, Screen::Home, None);
    player.pin_mut().guide_release();
    assert!(matches!(player.rust().guide, Guide::Closed));
    assert!(!player.pin_mut().go_back());
    assert!(!player.pin_mut().open_screen(99));

    player.pin_mut().open_screen(Screen::Recordings as i32);
    player.pin_mut().open_screen(Screen::Guide as i32);
    assert!(player.pin_mut().go_back());
    assert_screen(&player, Screen::Recordings, Some(Screen::Home));

    // Accepted inspection is observable before any decoder or media device is touched.
    let missing = QUrl::from_local_file(&QString::from(
        directory
            .path()
            .join("missing.ts")
            .to_string_lossy()
            .as_ref(),
    ));
    assert!(
        !player
            .pin_mut()
            .open_recording(QUrl::from("ftp://invalid/video.ts"))
    );
    assert_screen(&player, Screen::Recordings, Some(Screen::Home));
    player.pin_mut().select(-1);
    assert_screen(&player, Screen::Recordings, Some(Screen::Home));
    assert!(player.pin_mut().open_recording(missing.clone()));
    assert_screen(&player, Screen::Watching, Some(Screen::Recordings));
    assert!(player.watching_available() && player.recording_loading());
    player.pin_mut().cancel_recording_open();
    assert!(!player.watching_available());
    assert_screen(&player, Screen::Recordings, Some(Screen::Home));

    player.pin_mut().open_screen(Screen::Guide as i32);
    assert!(player.pin_mut().open_recording(missing.clone()));
    assert_screen(&player, Screen::Watching, Some(Screen::Guide));
    assert!(player.pin_mut().go_back());
    assert_screen(&player, Screen::Guide, Some(Screen::Recordings));
    let deadline = Instant::now() + Duration::from_secs(4);
    while player.recording_loading() {
        player.pin_mut().poll_recording();
        assert!(Instant::now() < deadline, "file inspection did not finish");
        thread::sleep(Duration::from_millis(1));
    }
    assert!(!player.file_error().is_empty());
    assert!(
        player.watching_available(),
        "accepted failure retains a viewing session"
    );
    assert_screen(&player, Screen::Guide, Some(Screen::Recordings));
    assert!(player.pin_mut().open_screen(Screen::Watching as i32));
    player.pin_mut().open_screen(Screen::Home as i32);
    assert!(player.watching_available());
    assert_screen(&player, Screen::Home, None);
    player.pin_mut().open_screen(Screen::Watching as i32);
    assert_screen(&player, Screen::Watching, Some(Screen::Home));
    player.pin_mut().stop();
    assert!(!player.watching_available());
    assert_screen(&player, Screen::Home, None);

    // Re-select an already-owned attempt: this exercises the real selection path
    // without constructing a decoder or asking for an audio/display device.
    let channels =
        crate::channels::parse(br#"[{"id":1,"name":"TV","type":1,"channel":{"type":"GR"}}]"#)?;
    player
        .pin_mut()
        .rust_mut()
        .catalog
        .replace(channels, Some(1));
    let attempt = super::stream_state::Attempt::new(
        &player.rust().catalog.channels()[0],
        crate::playback::input::Retention::Memory.into(),
    );
    player
        .pin_mut()
        .update_stream_state(super::stream_state::State::Connecting(attempt));
    player.pin_mut().open_screen(Screen::Guide as i32);
    player.pin_mut().select(0);
    assert_screen(&player, Screen::Watching, Some(Screen::Guide));
    assert!(player.watching_available());
    player.pin_mut().open_screen(Screen::Home as i32);
    player
        .pin_mut()
        .rust_mut()
        .catalog
        .replace(Vec::new(), None);
    assert!(
        player.watching_available(),
        "an empty catalog does not close the owned input"
    );
    player.pin_mut().stop();
    assert!(!player.watching_available());

    player.pin_mut().open_screen(Screen::Guide as i32);
    player.pin_mut().open_screen(Screen::Recordings as i32);
    player.pin_mut().rust_mut().epg_enabled = false;
    player.pin_mut().configure_epg();
    assert_screen(&player, Screen::Recordings, Some(Screen::Home));
    assert!(!player.pin_mut().open_screen(Screen::Guide as i32));
    player.pin_mut().rust_mut().epg_enabled = true;
    player.pin_mut().open_screen(Screen::Guide as i32);
    player.pin_mut().rust_mut().epg_enabled = false;
    player.pin_mut().configure_epg();
    assert_screen(&player, Screen::Recordings, Some(Screen::Home));

    player.pin_mut().rust_mut().epg_enabled = true;
    player.pin_mut().open_screen(Screen::Home as i32);
    player.pin_mut().open_screen(Screen::Guide as i32);
    assert!(player.pin_mut().open_recording(missing));
    assert_screen(&player, Screen::Watching, Some(Screen::Guide));
    let before_shutdown = changes.lock().unwrap().len();
    assert!(player.pin_mut().shutdown());
    assert_eq!(
        &changes.lock().unwrap()[before_shutdown..],
        &[(Screen::Home as i32, -1)]
    );
    assert!(!player.watching_available() && !player.guide_is_open());
    assert_screen(&player, Screen::Home, None);
    println!(
        "Screen navigation: always active, distinct history, Guide coherence, accepted loading/error/cancellation and shutdown passed"
    );
    Ok(())
}

fn guide_requests() -> TestResult {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()?;
    let server = runtime.block_on(MockServer::start());
    for (route, body) in [
        ("/api/services", "[]"),
        ("/api/programs", "[]"),
        ("/api/recorded", r#"{"records":[],"total":0}"#),
        ("/api/reserves", r#"{"reserves":[],"total":0}"#),
        ("/api/recording", r#"{"records":[],"total":0}"#),
        ("/api/config", r#"{"encode":[]}"#),
    ] {
        runtime.block_on(
            Mock::given(method("GET"))
                .and(path(route))
                .respond_with(ResponseTemplate::new(200).set_body_string(body))
                .expect(1)
                .mount(&server),
        );
    }
    let mut player = ffi::new_player();
    {
        let state = player.pin_mut().rust_mut().get_mut();
        state.epg_enabled = true;
        state.server = QString::from(server.uri());
        state.channel_refresh.requested(Instant::now());
        state.epg.configure(Some(server.uri()));
        state.preferences = settings::Loaded::transient(settings::Preferences {
            epgstation_server: server.uri(),
            recording_reservations_enabled: true,
            ..Default::default()
        })
        .activate(None, None);
    }
    assert!(player.pin_mut().open_screen(Screen::Guide as i32));
    assert!(matches!(player.rust().guide, Guide::AwaitingDay));
    assert!(player.rust().request.is_busy());
    assert!(player.rust().recording_library.verifying_reservations());
    player.pin_mut().poll_channels();
    // Consume only the EPG and reservation owners; accepting an empty channel result
    // would intentionally disable the EPG before this refresh can be observed.
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        player.pin_mut().poll_epg();
        player.pin_mut().poll_recording_reservations();
        let count = runtime.block_on(server.received_requests()).unwrap().len();
        if count == 6 && player.reservations_enabled() && !player.reservation_busy() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Guide entry did not schedule its existing requests"
        );
        thread::sleep(Duration::from_millis(1));
    }
    assert!(player.pin_mut().open_screen(Screen::Guide as i32));
    assert!(player.pin_mut().go_back());
    assert_screen(&player, Screen::Home, None);
    player.pin_mut().guide_release();
    assert!(matches!(player.rust().guide, Guide::Closed));
    Ok(())
}
