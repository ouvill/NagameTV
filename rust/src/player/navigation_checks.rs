//! Hardware-free checks use real request validation and logical session ownership.
use super::ffi;
use crate::{
    features::program_info::guide::Guide,
    screen_navigation::{Navigation, Screen},
    settings,
};
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
    if player.screen_navigation() {
        assert_eq!(player.guide_visible(), screen == Screen::Guide);
    }
}
fn observe(player: &ffi::Player) {
    if player.screen_navigation() {
        assert_eq!(
            player.guide_visible(),
            player.screen() == Screen::Guide as i32
        );
        assert!(player.screen() != Screen::Watching as i32 || player.watching_available());
    } else {
        assert_screen(player, Screen::Home, None);
    }
}

pub(super) fn run() -> TestResult {
    guide_requests()?;
    let directory = tempfile::tempdir()?;
    let settings_path = directory.path().join("settings.toml");
    let mut player = ffi::new_player();
    assert!(!player.screen_navigation());
    assert!(!player.watching_available());
    assert!(
        !player
            .pin_mut()
            .configure_screen_navigation(true, Screen::Home as i32)
    );
    {
        // Native checks use a locked launch plan; load only this private settings file.
        let state = player.pin_mut().rust_mut().get_mut();
        state.preferences = settings::Loaded::open(settings_path.clone())?.activate(None, None);
        state.screen_navigation_allowed = true;
        state.epg_enabled = true;
    }
    let changes = Arc::new(Mutex::new(Vec::new()));
    let observed = changes.clone();
    let _screen = player.pin_mut().on_screen_changed(move |player| {
        observe(&player);
        observed
            .lock()
            .unwrap()
            .push((player.screen(), player.back_screen()));
    });
    let _guide = player
        .pin_mut()
        .on_guide_visible_changed(move |mut player| {
            observe(&player);
            if player.guide_visible() {
                assert!(matches!(player.rust().guide, Guide::AwaitingDay));
                player.as_mut().guide_day(0.0, 86_400_000.0);
                assert!(matches!(player.rust().guide, Guide::Showing(_)));
            }
        });
    let _flag = player
        .pin_mut()
        .on_screen_navigation_changed(move |player| observe(&player));
    let _watching = player
        .pin_mut()
        .on_watching_available_changed(move |player| observe(&player));

    for screen in [
        Screen::Home,
        Screen::Watching,
        Screen::Guide,
        Screen::Recordings,
    ] {
        assert!(!player.pin_mut().open_screen(screen as i32));
    }
    assert!(!player.pin_mut().go_back());
    player.pin_mut().guide_open(true);
    assert!(player.guide_visible());
    assert!(
        changes.lock().unwrap().is_empty(),
        "OFF must not publish a history"
    );
    assert!(!player.pin_mut().configure_screen_navigation(true, -9));
    assert!(
        !player
            .pin_mut()
            .configure_screen_navigation(true, Screen::Watching as i32)
    );
    assert!(
        player
            .pin_mut()
            .configure_screen_navigation(true, Screen::Guide as i32)
    );
    assert_screen(&player, Screen::Guide, Some(Screen::Home));
    assert!(
        matches!(player.rust().guide, Guide::Showing(_)),
        "toggle preserves the visible day"
    );
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
    player.pin_mut().guide_open(true);
    player.pin_mut().guide_open(false);
    assert_screen(&player, Screen::Recordings, Some(Screen::Home));
    player.pin_mut().guide_open(false);
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

    player.pin_mut().guide_open(true);
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

    // OFF returns ownership without changing the Guide beneath QML's settings popup.
    player.pin_mut().rust_mut().epg_enabled = true;
    player.pin_mut().open_screen(Screen::Guide as i32);
    assert!(player.pin_mut().configure_screen_navigation(false, -1));
    assert!(!player.screen_navigation() && player.guide_visible());
    assert_screen(&player, Screen::Home, None);
    player.pin_mut().guide_open(false);
    assert!(!player.guide_visible());
    assert!(
        player
            .pin_mut()
            .configure_screen_navigation(true, Screen::Recordings as i32)
    );
    assert_screen(&player, Screen::Recordings, Some(Screen::Home));
    let preferences = settings::Loaded::open(settings_path.clone())?;
    assert!(preferences.preferences().screen_navigation_enabled);
    let mut restored = ffi::new_player();
    let state = restored.pin_mut().rust_mut().get_mut();
    state.preferences = preferences.activate(None, None);
    state.navigation = Navigation::new(state.preferences.preferences().screen_navigation_enabled);
    assert!(restored.screen_navigation());
    assert_screen(&restored, Screen::Home, None);

    // A write error retains the accepted runtime choice through the normal settings API.
    std::fs::remove_file(&settings_path)?;
    std::fs::create_dir(&settings_path)?;
    assert!(player.pin_mut().configure_screen_navigation(false, -1));
    assert!(!player.screen_navigation() && !player.settings_error().is_empty());
    std::fs::remove_dir(&settings_path)?;
    assert!(
        player
            .pin_mut()
            .configure_screen_navigation(true, Screen::Home as i32)
    );
    assert!(player.settings_error().is_empty());
    player.pin_mut().open_screen(Screen::Guide as i32);
    assert!(player.pin_mut().open_recording(missing));
    assert_screen(&player, Screen::Watching, Some(Screen::Guide));
    let before_shutdown = changes.lock().unwrap().len();
    assert!(player.pin_mut().shutdown());
    assert_eq!(
        &changes.lock().unwrap()[before_shutdown..],
        &[(Screen::Home as i32, -1)]
    );
    assert!(!player.watching_available() && !player.guide_visible());
    assert_screen(&player, Screen::Home, None);
    println!(
        "Screen navigation: flag gates, distinct history, Guide coherence, accepted loading/error/cancellation, toggle, persistence and shutdown passed"
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
        state.screen_navigation_allowed = true;
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
    assert!(
        player
            .pin_mut()
            .configure_screen_navigation(true, Screen::Home as i32)
    );
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
