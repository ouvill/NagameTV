use super::*;

fn from_saved(
    server: &str,
    enabled: bool,
) -> Result<
    (
        cxx::UniquePtr<ffi::Player>,
        tempfile::TempDir,
        std::path::PathBuf,
    ),
    Box<dyn std::error::Error>,
> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("settings.toml");
    let preferences = crate::settings::Preferences {
        epgstation_server: server.into(),
        recording_reservations_enabled: enabled,
        ..Default::default()
    };
    std::fs::write(&path, toml::to_string(&preferences)?)?;
    let mut player = ffi::new_player();
    let state = player.pin_mut().rust_mut().get_mut();
    state.preferences = crate::settings::Loaded::open(path.clone())?.activate(None, None);
    state.recording_reservations_allowed = true;
    state.epg_enabled = true;
    // The native suite has a locked launch plan. Exercise the same startup hook
    // after loading this private file instead of changing process-wide settings.
    state.verify_reservation_session();
    Ok((player, directory, path))
}
fn finish_verification(player: &mut cxx::UniquePtr<ffi::Player>) {
    let deadline = Instant::now() + Duration::from_secs(4);
    player.pin_mut().poll_recording_reservations();
    while player.rust().recording_library.verifying_reservations() || player.reservation_busy() {
        player.pin_mut().poll_recording_reservations();
        assert!(
            Instant::now() < deadline,
            "startup verification did not finish"
        );
        thread::sleep(Duration::from_millis(1));
    }
}
pub(super) fn run() -> TestResult {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()?;
    let server = runtime.block_on(MockServer::start());
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let allowed = Arc::new(AtomicBool::new(true));
    let permit = allowed.clone();
    runtime.block_on(
        Mock::given(method("GET"))
            .and(path("/api/recorded"))
            .respond_with(move |_: &wiremock::Request| {
                count.fetch_add(1, Ordering::SeqCst);
                if permit.load(Ordering::SeqCst) {
                    ResponseTemplate::new(200)
                        .set_body_json(json!({"records":[],"total":0}))
                        .set_delay(Duration::from_millis(30))
                } else {
                    ResponseTemplate::new(401)
                }
            })
            .expect(4)
            .mount(&server),
    );
    for (route, body) in [
        ("/api/reserves", json!({"reserves":[],"total":0})),
        ("/api/recording", json!({"records":[],"total":0})),
        ("/api/config", json!({"encode":[]})),
    ] {
        runtime.block_on(
            Mock::given(method("GET"))
                .and(path(route))
                .respond_with(ResponseTemplate::new(200).set_body_json(body))
                .mount(&server),
        );
    }

    let (mut startup, _directory, path) = from_saved(&server.uri(), true)?;
    let before = std::fs::read(&path)?;
    let signals = Arc::new(AtomicUsize::new(0));
    let observer = signals.clone();
    let _guard = startup.pin_mut().on_epgstation_changed(move |_| {
        observer.fetch_add(1, Ordering::SeqCst);
    });
    startup
        .pin_mut()
        .open_screen(crate::screen_navigation::Screen::Guide as i32);
    startup.pin_mut().go_back();
    startup
        .pin_mut()
        .open_screen(crate::screen_navigation::Screen::Guide as i32);
    finish_verification(&mut startup);
    assert!(startup.reservations_enabled());
    assert!(startup.reservation_error().is_empty());
    assert!(!startup.epgstation_loaded() && !startup.epgstation_busy());
    assert!(startup.epgstation_error().is_empty());
    assert_eq!(startup.rust().recording_library.revision(), 0);
    assert_eq!(signals.load(Ordering::SeqCst), 0);
    assert_eq!(
        std::fs::read(&path)?,
        before,
        "verification must not save settings"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "guide-open must not duplicate an in-flight startup check"
    );

    let (mut off, _off_directory, _) = from_saved(&server.uri(), false)?;
    off.pin_mut()
        .open_screen(crate::screen_navigation::Screen::Guide as i32);
    finish_verification(&mut off);
    assert!(!off.reservations_enabled());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    off.pin_mut().configure_recording_reservations(true);
    finish_verification(&mut off);
    assert!(off.reservations_enabled());
    assert_eq!(calls.load(Ordering::SeqCst), 2);

    allowed.store(false, Ordering::SeqCst);
    let (mut denied, _denied_directory, denied_path) = from_saved(&server.uri(), true)?;
    let unchanged = std::fs::read(&denied_path)?;
    finish_verification(&mut denied);
    assert!(!denied.reservations_enabled());
    assert!(denied.reservation_error().is_empty() && denied.epgstation_error().is_empty());
    for _ in 0..10 {
        denied.pin_mut().poll_recording_reservations();
    }
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    allowed.store(true, Ordering::SeqCst);
    denied
        .pin_mut()
        .open_screen(crate::screen_navigation::Screen::Guide as i32);
    finish_verification(&mut denied);
    assert!(denied.reservations_enabled());
    assert!(!denied.epgstation_loaded());
    assert_eq!(calls.load(Ordering::SeqCst), 4);
    assert_eq!(std::fs::read(denied_path)?, unchanged);
    println!(
        "Reservation startup verification: flag gate, shared session, guide retry, no duplicate requests, no recording UI changes or settings writes passed"
    );
    Ok(())
}
