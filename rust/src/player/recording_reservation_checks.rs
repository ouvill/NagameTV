//! Hardware-free QML contract and role update checks with real asynchronous owners.
use super::ffi;
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QModelIndex, QString};
use serde_json::json;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;
fn finish(player: &mut cxx::UniquePtr<ffi::Player>) {
    let deadline = Instant::now() + Duration::from_secs(4);
    while player.reservation_busy() || player.epgstation_busy() {
        player.pin_mut().poll_recording_library();
        player.pin_mut().poll_recording_reservations();
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
}
pub(super) fn run() -> TestResult {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()?;
    let server = runtime.block_on(MockServer::start());
    let start =
        u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())? + 3_600_000;
    let end = start + 600_000;
    let reserved = Arc::new(AtomicBool::new(false));
    let row = json!({"id":7,"programId":100,"channelId":10,"startAt":start,"endAt":end,"isSkip":false,"isConflict":false,"isOverlap":false,"isTimeSpecified":false});
    let mount = |mock: Mock| runtime.block_on(mock.mount(&server));
    for endpoint in ["/api/recorded", "/api/recording"] {
        mount(Mock::given(method("GET")).and(path(endpoint)).respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"records":[],"total":0})),
        ));
    }
    mount(
        Mock::given(method("GET"))
            .and(path("/api/channels"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([]))),
    );
    mount(Mock::given(method("GET")).and(path("/api/programs")).respond_with(ResponseTemplate::new(200).set_body_json(json!([{"id":100,"networkId":1,"serviceId":1,"startAt":start,"duration":600000,"name":"Program"}]))));
    mount(
        Mock::given(method("GET"))
            .and(path("/api/schedules/detail/100"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"id":100,"channelId":10,"startAt":start,"endAt":end})),
            ),
    );
    let listing = reserved.clone();
    let listed = row.clone();
    mount(
        Mock::given(method("GET"))
            .and(path("/api/reserves"))
            .respond_with(move |_: &wiremock::Request| {
                let rows = if listing.load(Ordering::SeqCst) {
                    vec![listed.clone()]
                } else {
                    vec![]
                };
                ResponseTemplate::new(200)
                    .set_body_json(json!({"total":rows.len(),"reserves":rows}))
            }),
    );
    let added = reserved.clone();
    mount(
        Mock::given(method("POST"))
            .and(path("/api/reserves"))
            .respond_with(move |_: &wiremock::Request| {
                added.store(true, Ordering::SeqCst);
                ResponseTemplate::new(201).set_body_json(json!({"reserveId":7}))
            })
            .expect(1),
    );
    mount(
        Mock::given(method("GET"))
            .and(path("/api/reserves/7"))
            .respond_with(ResponseTemplate::new(200).set_body_json(row)),
    );
    let removed = reserved.clone();
    mount(
        Mock::given(method("DELETE"))
            .and(path("/api/reserves/7"))
            .respond_with(move |_: &wiremock::Request| {
                removed.store(false, Ordering::SeqCst);
                ResponseTemplate::new(200)
            })
            .expect(1),
    );
    let mut player = ffi::new_player();
    assert!(!player.recording_reservations_enabled());
    player.pin_mut().configure_recording_reservations(true);
    assert!(!player.recording_reservations_enabled());
    assert!(
        player
            .pin_mut()
            .browse_epgstation(QString::from(server.uri()), QString::default())
    );
    finish(&mut player);
    assert!(!player.reservations_enabled());
    assert!(
        runtime
            .block_on(server.received_requests())
            .unwrap()
            .iter()
            .all(|r| !r.url.path().contains("reserves"))
    );
    {
        let this = player.pin_mut().rust_mut().get_mut();
        this.recording_reservations_allowed = true;
        this.epg_enabled = true;
        this.catalog.replace(
            crate::channels::parse(
                br#"[{"id":10,"networkId":1,"serviceId":1,"name":"Channel","type":1}]"#,
            )?,
            None,
        );
        this.epg.configure(Some(server.uri()));
        let deadline = Instant::now() + Duration::from_secs(3);
        while this
            .epg
            .poll(this.network.as_ref().unwrap())
            .completed
            .is_none()
        {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        }
        this.guide = crate::features::program_info::guide::Guide::Showing(
            crate::features::program_info::guide::DayWindow::new(start as f64, end as f64)?,
        );
        this.guide_dirty = true;
    }
    player.pin_mut().publish_guide();
    let model = &player.rust().guide_model;
    let role = |name: &str| {
        *model
            .role_names()
            .iter()
            .find(|(_, value)| value.to_string() == name)
            .unwrap()
            .0
    };
    let key_role = role("watchKey");
    let reservation_role = role("reservation");
    let index = model.model_index(0, 0, &QModelIndex::default());
    let key = model.data(&index, key_role).value::<QString>().unwrap();
    let resets = Arc::new(AtomicUsize::new(0));
    let reset_observer = resets.clone();
    let _guard = player
        .pin_mut()
        .rust_mut()
        .guide_model
        .pin_mut()
        .on_changed(move |_| {
            reset_observer.fetch_add(1, Ordering::SeqCst);
        });
    crate::guide_model::ffi::check_guide_model(player.pin_mut().rust_mut().guide_model.pin_mut());
    let watched_key = key.clone();
    let _notification = player.pin_mut().on_reservations_changed(move |p| {
        let model = &p.rust().guide_model;
        let index = model.model_index(0, 0, &QModelIndex::default());
        assert_eq!(
            model
                .data(&index, reservation_role)
                .value::<QString>()
                .unwrap(),
            p.reservation_state(watched_key.clone())
        );
    });
    player.pin_mut().configure_recording_reservations(true);
    assert!(player.reservations_enabled());
    finish(&mut player);
    assert!(player.can_reserve_program(key.clone()));
    assert!(player.pin_mut().reserve_program(key.clone()));
    finish(&mut player);
    assert_eq!(
        player.reservation_state(key.clone()).to_string(),
        "reserved"
    );
    assert!(player.can_cancel_reservation(key.clone()));
    assert!(player.pin_mut().cancel_reservation(key.clone()));
    finish(&mut player);
    assert!(player.reservation_state(key.clone()).is_empty());
    assert_eq!(
        resets.load(Ordering::SeqCst),
        0,
        "reservation changes must not reset or rebuild guide rows"
    );
    player.pin_mut().configure_recording_reservations(false);
    finish(&mut player);
    assert!(!player.reservations_enabled());
    assert!(!player.pin_mut().reserve_program(key));
    assert!(player.pin_mut().request_language(QString::from("en")));
    assert_eq!(
        player.reservation_error().to_string(),
        "Connect to EPGStation and enable recording reservations first."
    );
    assert!(player.pin_mut().request_language(QString::from("ja")));
    assert_eq!(
        player.reservation_error().to_string(),
        "EPGStationに接続し、録画予約を有効にしてください。"
    );
    assert!(player.pin_mut().request_language(QString::from("en")));
    println!(
        "Recording reservations: default-off gate, guide roles, notification coherence and manual writes passed"
    );
    Ok(())
}
