use super::*;
use std::sync::atomic::AtomicUsize;

fn empty_lists(fixture: &Fixture) {
    fixture.empty_recording();
    fixture.mount(
        Mock::given(method("GET"))
            .and(path("/api/reserves"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"reserves":[],"total":0})),
            ),
    );
}
fn config_done(fixture: &Fixture, manager: &mut Manager) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !matches!(
        manager.encode,
        encode::Catalogue::Fetched(_) | encode::Catalogue::Unavailable
    ) {
        manager.poll(&fixture.network, Instant::now());
        assert!(Instant::now() < deadline, "config fetch did not finish");
        thread::sleep(Duration::from_millis(1));
    }
}
fn write(fixture: &Fixture, manager: &mut Manager, mode: &str, expected: Value) {
    fixture.mount(
        Mock::given(method("GET"))
            .and(path("/api/schedules/detail/100"))
            .and(query_param("isHalfWidth", "false"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"id":100,"channelId":10,"startAt":10000,"endAt":20000})),
            ),
    );
    fixture.mount(
        Mock::given(method("POST"))
            .and(path("/api/reserves"))
            .and(header("cookie", "reservation-session=private-cookie"))
            .and(body_json(expected))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({"reserveId":7})))
            .expect(1),
    );
    assert!(manager.can_reserve(program(), NOW));
    assert!(manager.reserve(&fixture.network, program(), NOW, mode));
    fixture.finish(manager);
    assert!(manager.error().is_none());
}

#[test]
fn only_a_listed_saved_mode_is_sent_and_originals_are_always_kept() {
    for mode in ["HD", "SD", "absent", ""] {
        let fixture = Fixture::new();
        let library = fixture.connect(true);
        empty_lists(&fixture);
        fixture.mount(
            Mock::given(method("GET"))
                .and(path("/api/config"))
                .and(header("cookie", "reservation-session=private-cookie"))
                .respond_with(
                    ResponseTemplate::new(200).set_body_json(json!({"encode":["HD","SD","HD"]})),
                )
                .expect(1),
        );
        let mut manager = fixture.ready(&library);
        config_done(&fixture, &mut manager);
        assert_eq!(manager.encode_modes(), ["HD", "SD"]);
        let mut expected = json!({"programId":100,"allowEndLack":false});
        if matches!(mode, "HD" | "SD") {
            expected["encodeOption"] = json!({"mode1":mode,"isDeleteOriginalAfterEncode":false});
        }
        write(&fixture, &mut manager, mode, expected);
    }
}

#[test]
fn unavailable_config_is_optional_and_never_becomes_a_reservation_error() {
    for response in [
        ResponseTemplate::new(500),
        ResponseTemplate::new(401),
        ResponseTemplate::new(200).set_body_json(json!({})),
        ResponseTemplate::new(200).set_body_json(json!({"encode":["HD",false]})),
        ResponseTemplate::new(200).set_body_json(json!({"encode":[""]})),
        ResponseTemplate::new(200).set_body_json(json!({"encode":["x".repeat(257)]})),
        ResponseTemplate::new(200).set_body_json(json!({"encode":vec!["HD";65]})),
        ResponseTemplate::new(200)
            .set_body_bytes(vec![b' '; crate::epgstation::MAX_RESPONSE_BYTES + 1]),
    ] {
        let fixture = Fixture::new();
        let library = fixture.connect(true);
        empty_lists(&fixture);
        fixture.mount(
            Mock::given(method("GET"))
                .and(path("/api/config"))
                .respond_with(response)
                .expect(1),
        );
        let mut manager = fixture.ready(&library);
        config_done(&fixture, &mut manager);
        assert!(manager.encode_modes().is_empty());
        assert!(manager.error().is_none());
        for _ in 0..3 {
            manager.poll(&fixture.network, Instant::now());
        }
        write(
            &fixture,
            &mut manager,
            "HD",
            json!({"programId":100,"allowEndLack":false}),
        );
    }
}

#[test]
fn config_redirect_does_not_forward_the_authenticated_session() {
    let fixture = Fixture::new();
    let target = Fixture::new();
    let library = fixture.connect(true);
    empty_lists(&fixture);
    target.mount(
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0),
    );
    fixture.mount(
        Mock::given(method("GET"))
            .and(path("/api/config"))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("location", format!("{}/private", target.server.uri())),
            )
            .expect(1),
    );
    let mut manager = fixture.ready(&library);
    config_done(&fixture, &mut manager);
    assert!(manager.encode_modes().is_empty());
    assert!(manager.error().is_none());
    write(
        &fixture,
        &mut manager,
        "HD",
        json!({"programId":100,"allowEndLack":false}),
    );
}

#[test]
fn slow_config_does_not_block_writes_and_disabled_sessions_discard_late_modes() {
    let fixture = Fixture::new();
    let library = fixture.connect(true);
    empty_lists(&fixture);
    let started = Arc::new(AtomicBool::new(false));
    let observed = started.clone();
    fixture.mount(
        Mock::given(method("GET"))
            .and(path("/api/config"))
            .respond_with(move |_: &wiremock::Request| {
                observed.store(true, Ordering::SeqCst);
                ResponseTemplate::new(200)
                    .set_body_json(json!({"encode":["late"]}))
                    .set_delay(Duration::from_secs(10))
            })
            .expect(1),
    );
    let mut manager = Manager::default();
    library.synchronize_reservations(false, &mut manager, Instant::now());
    fixture.finish(&mut manager);
    assert!(!started.load(Ordering::SeqCst));
    library.synchronize_reservations(true, &mut manager, Instant::now());
    fixture.finish(&mut manager);
    let deadline = Instant::now() + Duration::from_secs(3);
    while !started.load(Ordering::SeqCst) {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    assert!(manager.encode_modes().is_empty());
    write(
        &fixture,
        &mut manager,
        "late",
        json!({"programId":100,"allowEndLack":false}),
    );
    library.synchronize_reservations(false, &mut manager, Instant::now());
    let deadline = Instant::now() + Duration::from_secs(3);
    while !matches!(manager.encode, encode::Catalogue::Unknown) {
        manager.poll(&fixture.network, Instant::now());
        assert!(manager.encode_modes().is_empty());
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn only_explicit_refresh_reloads_modes_and_connection_generations_discard_them() {
    let fixture = Fixture::new();
    let mut library = fixture.connect(true);
    empty_lists(&fixture);
    let count = Arc::new(AtomicUsize::new(0));
    let calls = count.clone();
    fixture.mount(
        Mock::given(method("GET"))
            .and(path("/api/config"))
            .respond_with(move |_: &wiremock::Request| {
                let mode = format!("mode-{}", calls.fetch_add(1, Ordering::SeqCst));
                ResponseTemplate::new(200).set_body_json(json!({"encode":[mode]}))
            })
            .expect(4),
    );
    let mut manager = fixture.ready(&library);
    config_done(&fixture, &mut manager);
    assert_eq!(manager.encode_modes(), ["mode-0"]);
    manager.poll(&fixture.network, Instant::now() + REFRESH_INTERVAL);
    fixture.finish(&mut manager);
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert!(manager.refresh(&fixture.network));
    assert!(manager.encode_modes().is_empty());
    fixture.finish(&mut manager);
    config_done(&fixture, &mut manager);
    assert_eq!(manager.encode_modes(), ["mode-1"]);
    library.synchronize_reservations(false, &mut manager, Instant::now());
    assert!(manager.encode_modes().is_empty());
    library.synchronize_reservations(true, &mut manager, Instant::now());
    fixture.finish(&mut manager);
    config_done(&fixture, &mut manager);
    assert_eq!(manager.encode_modes(), ["mode-2"]);
    library
        .open(
            &fixture.network,
            &fixture.server.uri(),
            "",
            Login::password("viewer".into(), "private-password".into()).unwrap(),
        )
        .unwrap();
    library.synchronize_reservations(true, &mut manager, Instant::now());
    assert!(manager.encode_modes().is_empty());
    let deadline = Instant::now() + Duration::from_secs(3);
    while library.busy() {
        library.poll(&fixture.network);
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    library.synchronize_reservations(true, &mut manager, Instant::now());
    fixture.finish(&mut manager);
    config_done(&fixture, &mut manager);
    assert_eq!(manager.encode_modes(), ["mode-3"]);
    let other = Fixture::new();
    empty_lists(&other);
    other.mount(
        Mock::given(method("GET"))
            .and(path("/api/config"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"encode":["other"]})))
            .expect(1),
    );
    // Mount login-independent catalogue endpoints before switching the existing owner.
    let _other_library = other.connect(false);
    library
        .open(&fixture.network, &other.server.uri(), "", Login::Current)
        .unwrap();
    library.synchronize_reservations(true, &mut manager, Instant::now());
    assert!(manager.encode_modes().is_empty());
    let deadline = Instant::now() + Duration::from_secs(3);
    while library.busy() {
        library.poll(&fixture.network);
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    library.synchronize_reservations(true, &mut manager, Instant::now());
    fixture.finish(&mut manager);
    config_done(&fixture, &mut manager);
    assert_eq!(manager.encode_modes(), ["other"]);
    assert!(!manager.accepts_encode_mode("mode-3"));
}
