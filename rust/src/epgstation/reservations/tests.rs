use super::*;
use crate::epgstation::{Library, Login};
use serde_json::{Value, json};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    thread,
};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_json, header, method, path, query_param},
};
type TestResult = Result<(), Box<dyn std::error::Error>>;
const NOW: u64 = 1000;
fn program() -> Program {
    Program::new(100, 10, 10_000, 20_000).unwrap()
}
fn reserve(id: u64, program_id: u64) -> Value {
    json!({"id":id,"programId":program_id,"channelId":10,"startAt":10000,"endAt":20000,"isSkip":false,"isConflict":false,"isOverlap":false,"isTimeSpecified":false})
}
struct Fixture {
    runtime: tokio::runtime::Runtime,
    network: Network,
    server: MockServer,
}
impl Fixture {
    fn new() -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        Self {
            server: runtime.block_on(MockServer::start()),
            network: Network::new().unwrap(),
            runtime,
        }
    }
    fn mount(&self, mock: Mock) {
        self.runtime.block_on(mock.mount(&self.server));
    }
    fn connect(&self, authenticated: bool) -> Library {
        self.mount(
            Mock::given(method("GET"))
                .and(path("/api/recorded"))
                .respond_with(
                    ResponseTemplate::new(200).set_body_json(json!({"records":[],"total":0})),
                ),
        );
        self.mount(
            Mock::given(method("GET"))
                .and(path("/api/channels"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!([]))),
        );
        let login = if authenticated {
            self.mount(
                Mock::given(method("POST"))
                    .and(path("/api/auth/login"))
                    .respond_with(
                        ResponseTemplate::new(200)
                            .insert_header(
                                "set-cookie",
                                "reservation-session=private-cookie; Path=/",
                            )
                            .set_body_json(json!({})),
                    ),
            );
            self.mount(
                Mock::given(method("GET"))
                    .and(path("/api/auth/media-token"))
                    .respond_with(
                        ResponseTemplate::new(200).set_body_json(json!({"token":"private-token"})),
                    ),
            );
            Login::password("viewer".into(), "private-password".into()).unwrap()
        } else {
            Login::Current
        };
        let mut library = Library::default();
        library
            .open(&self.network, &self.server.uri(), "", login)
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while library.busy() {
            library.poll(&self.network);
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        }
        assert!(library.error().is_none());
        library
    }
    fn finish(&self, manager: &mut Manager) {
        let deadline = Instant::now() + Duration::from_secs(3);
        manager.poll(&self.network, Instant::now());
        while manager.busy() {
            manager.poll(&self.network, Instant::now());
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        }
    }
    fn empty_recording(&self) {
        self.mount(
            Mock::given(method("GET"))
                .and(path("/api/recording"))
                .respond_with(
                    ResponseTemplate::new(200).set_body_json(json!({"records":[],"total":0})),
                ),
        );
    }
}
#[test]
fn disabled_connected_owner_makes_no_requests_and_authenticated_writes_are_serial() -> TestResult {
    let fixture = Fixture::new();
    let library = fixture.connect(true);
    let reserved = Arc::new(AtomicBool::new(false));
    let listing = reserved.clone();
    fixture.mount(
        Mock::given(method("GET"))
            .and(path("/api/reserves"))
            .and(query_param("type", "all"))
            .and(query_param("limit", "100"))
            .and(header("cookie", "reservation-session=private-cookie"))
            .respond_with(move |_: &wiremock::Request| {
                let rows = if listing.load(Ordering::SeqCst) {
                    vec![reserve(7, 100)]
                } else {
                    vec![]
                };
                ResponseTemplate::new(200)
                    .set_body_json(json!({"total":rows.len(),"reserves":rows}))
            }),
    );
    fixture.empty_recording();
    fixture.mount(
        Mock::given(method("GET"))
            .and(path("/api/schedules/detail/100"))
            .and(query_param("isHalfWidth", "false"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"id":100,"channelId":10,"startAt":10000,"endAt":20000})),
            ),
    );
    let created = reserved.clone();
    fixture.mount(
        Mock::given(method("POST"))
            .and(path("/api/reserves"))
            .and(header("cookie", "reservation-session=private-cookie"))
            .and(body_json(json!({"programId":100,"allowEndLack":false})))
            .respond_with(move |_: &wiremock::Request| {
                created.store(true, Ordering::SeqCst);
                ResponseTemplate::new(201).set_body_json(json!({"reserveId":7}))
            })
            .expect(1),
    );
    fixture.mount(
        Mock::given(method("GET"))
            .and(path("/api/reserves/7"))
            .respond_with(ResponseTemplate::new(200).set_body_json(reserve(7, 100))),
    );
    let removed = reserved.clone();
    fixture.mount(
        Mock::given(method("DELETE"))
            .and(path("/api/reserves/7"))
            .and(header("cookie", "reservation-session=private-cookie"))
            .respond_with(move |_: &wiremock::Request| {
                removed.store(false, Ordering::SeqCst);
                ResponseTemplate::new(200)
            })
            .expect(1),
    );
    let mut manager = Manager::default();
    library.synchronize_reservations(false, &mut manager, Instant::now());
    fixture.finish(&mut manager);
    assert!(!manager.reserve(&fixture.network, program(), NOW));
    assert!(!manager.refresh(&fixture.network));
    assert!(
        fixture
            .runtime
            .block_on(fixture.server.received_requests())
            .unwrap()
            .iter()
            .all(|request| !request.url.path().contains("reserves"))
    );
    library.synchronize_reservations(true, &mut manager, Instant::now());
    fixture.finish(&mut manager);
    assert!(manager.can_reserve(program(), NOW));
    assert!(manager.reserve(&fixture.network, program(), NOW));
    assert!(!manager.reserve(&fixture.network, program(), NOW));
    fixture.finish(&mut manager);
    assert_eq!(manager.snapshot().status(program()), Some(Status::Reserved));
    assert!(manager.can_cancel(program(), NOW));
    assert!(manager.cancel(&fixture.network, program(), NOW));
    fixture.finish(&mut manager);
    assert_eq!(manager.snapshot().status(program()), None);
    assert!(manager.error().is_none());
    Ok(())
}
#[test]
fn status_precedence_uses_program_channel_and_start_without_cancelling_rule_or_recording()
-> TestResult {
    let mut conflict = reserve(1, 100);
    conflict["isConflict"] = true.into();
    let mut skip = reserve(2, 101);
    skip["isSkip"] = true.into();
    let mut rule = reserve(3, 102);
    rule["ruleId"] = 3.into();
    let snapshot = Snapshot::from_rows(
        serde_json::from_value(json!([conflict, skip, rule, reserve(4, 103)]))?,
        serde_json::from_value(
            json!([{"id":1,"programId":103,"channelId":10,"startAt":10000,"isRecording":true}]),
        )?,
    )?;
    for (id, status) in [
        (100, Status::Conflict),
        (101, Status::Skipped),
        (102, Status::Reserved),
        (103, Status::Recording),
    ] {
        assert_eq!(
            snapshot.status(Program::new(id, 10, 10000, 20000).unwrap()),
            Some(status)
        );
    }
    assert_eq!(
        snapshot.cancellation(Program::new(102, 10, 10000, 20000).unwrap(), NOW),
        None
    );
    assert_eq!(
        snapshot.cancellation(Program::new(103, 10, 10000, 20000).unwrap(), NOW),
        None
    );
    assert_eq!(
        snapshot.status(Program::new(100, 11, 10000, 20000).unwrap()),
        None
    );
    assert_eq!(
        snapshot.status(Program::new(100, 10, 30000, 40000).unwrap()),
        None
    );
    assert_eq!(snapshot.cancellation(program(), 10000), None);
    Ok(())
}
#[test]
fn disabling_discards_late_results_and_reenable_fetches_again() {
    let fixture = Fixture::new();
    let library = fixture.connect(false);
    fixture.mount(
        Mock::given(method("GET"))
            .and(path("/api/reserves"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_secs(10))
                    .set_body_json(json!({"reserves":[reserve(7,100)],"total":1})),
            ),
    );
    fixture.empty_recording();
    let mut manager = Manager::default();
    library.synchronize_reservations(true, &mut manager, Instant::now());
    manager.poll(&fixture.network, Instant::now());
    assert!(manager.busy());
    library.synchronize_reservations(false, &mut manager, Instant::now());
    assert!(!manager.enabled());
    assert!(manager.busy());
    fixture.finish(&mut manager);
    assert!(!manager.busy());
    assert_eq!(manager.snapshot().status(program()), None);
    fixture.runtime.block_on(fixture.server.reset());
    fixture.mount(
        Mock::given(method("GET"))
            .and(path("/api/reserves"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"reserves":[],"total":0})),
            ),
    );
    fixture.empty_recording();
    library.synchronize_reservations(true, &mut manager, Instant::now());
    fixture.finish(&mut manager);
    assert!(manager.can_reserve(program(), NOW));
}
#[test]
fn malformed_oversized_and_denied_lists_never_enable_writes_or_echo_response_values() {
    for response in [
        ResponseTemplate::new(401).set_body_string("private-password"),
        ResponseTemplate::new(200)
            .set_body_bytes(vec![b' '; crate::epgstation::MAX_RESPONSE_BYTES + 1]),
        ResponseTemplate::new(200).set_body_json(json!({"total":0})),
        ResponseTemplate::new(200).set_body_json(json!({"reserves":[],"total":10001})),
        ResponseTemplate::new(200).set_body_json(json!({"reserves":[],"total":"private-token"})),
        ResponseTemplate::new(302).insert_header("location", "http://example.test/private-token"),
    ] {
        let fixture = Fixture::new();
        let library = fixture.connect(false);
        fixture.mount(
            Mock::given(method("GET"))
                .and(path("/api/reserves"))
                .respond_with(response),
        );
        fixture.empty_recording();
        let mut manager = Manager::default();
        library.synchronize_reservations(true, &mut manager, Instant::now());
        fixture.finish(&mut manager);
        assert!(!manager.can_reserve(program(), NOW));
        let error = manager.error().expect("list error").to_string();
        assert!(!error.contains("private-password") && !error.contains("private-token"));
    }
}
#[test]
fn changed_program_never_sends_a_reservation() {
    let fixture = Fixture::new();
    let library = fixture.connect(false);
    fixture.mount(
        Mock::given(method("GET"))
            .and(path("/api/reserves"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"reserves":[],"total":0})),
            ),
    );
    fixture.empty_recording();
    fixture.mount(
        Mock::given(method("GET"))
            .and(path("/api/schedules/detail/100"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"id":100,"channelId":10,"startAt":11000,"endAt":21000})),
            ),
    );
    fixture.mount(
        Mock::given(method("POST"))
            .and(path("/api/reserves"))
            .respond_with(ResponseTemplate::new(500))
            .expect(0),
    );
    let mut manager = Manager::default();
    library.synchronize_reservations(true, &mut manager, Instant::now());
    fixture.finish(&mut manager);
    assert!(manager.reserve(&fixture.network, program(), NOW));
    fixture.finish(&mut manager);
    assert!(matches!(
        manager.error(),
        Some(FetchError::Parse(Error::ProgramChanged))
    ));
    assert!(!manager.can_reserve(program(), NOW));
}
proptest::proptest! {
    #[test]
    fn state_is_independent_of_response_order(flags in proptest::collection::vec((proptest::bool::ANY,proptest::bool::ANY),1..30)) {
        let rows = |reverse: bool| {
            let mut rows: Vec<_> = flags.iter().enumerate().map(|(i,(conflict,skip))| {
                let mut row = reserve(i as u64,100); row["isConflict"] = (*conflict).into(); row["isSkip"] = (*skip).into(); row
            }).collect();
            if reverse { rows.reverse(); }
            serde_json::from_value(json!(rows)).unwrap()
        };
        let a = Snapshot::from_rows(rows(false),vec![]).unwrap(); let b = Snapshot::from_rows(rows(true),vec![]).unwrap();
        proptest::prop_assert_eq!(a.status(program()),b.status(program()));
        if flags.len() > 1 { proptest::prop_assert_eq!(a.cancellation(program(),NOW),None); }
    }
}

#[test]
fn successful_write_with_failed_refresh_is_not_repeated() {
    let fixture = Fixture::new();
    let library = fixture.connect(false);
    let written = Arc::new(AtomicBool::new(false));
    let listing = written.clone();
    fixture.mount(
        Mock::given(method("GET"))
            .and(path("/api/reserves"))
            .respond_with(move |_: &wiremock::Request| {
                if listing.load(Ordering::SeqCst) {
                    ResponseTemplate::new(500)
                } else {
                    ResponseTemplate::new(200).set_body_json(json!({"reserves":[],"total":0}))
                }
            }),
    );
    fixture.empty_recording();
    fixture.mount(
        Mock::given(method("GET"))
            .and(path("/api/schedules/detail/100"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"id":100,"channelId":10,"startAt":10000,"endAt":20000})),
            ),
    );
    fixture.mount(
        Mock::given(method("POST"))
            .and(path("/api/reserves"))
            .respond_with(move |_: &wiremock::Request| {
                written.store(true, Ordering::SeqCst);
                ResponseTemplate::new(201).set_body_json(json!({"reserveId":7}))
            })
            .expect(1),
    );
    let mut manager = Manager::default();
    library.synchronize_reservations(true, &mut manager, Instant::now());
    fixture.finish(&mut manager);
    assert!(manager.reserve(&fixture.network, program(), NOW));
    fixture.finish(&mut manager);
    assert!(matches!(
        manager.error(),
        Some(FetchError::Parse(Error::RefreshAfterWrite(_)))
    ));
    assert!(!manager.can_reserve(program(), NOW));
    let later = Instant::now() + REFRESH_INTERVAL;
    manager.poll(&fixture.network, later);
    fixture.finish(&mut manager);
    assert!(manager.error().is_some());
}
#[test]
fn all_pages_are_required_before_reservations_become_actionable() {
    let fixture = Fixture::new();
    let library = fixture.connect(false);
    let rows: Vec<_> = (0..100).map(|id| reserve(id, id + 100)).collect();
    fixture.mount(
        Mock::given(method("GET"))
            .and(path("/api/reserves"))
            .and(query_param("offset", "0"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"reserves":rows,"total":101})),
            )
            .expect(1),
    );
    fixture.mount(
        Mock::given(method("GET"))
            .and(path("/api/reserves"))
            .and(query_param("offset", "100"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"reserves":[reserve(100,200)],"total":101})),
            )
            .expect(1),
    );
    fixture.empty_recording();
    let mut manager = Manager::default();
    library.synchronize_reservations(true, &mut manager, Instant::now());
    fixture.finish(&mut manager);
    assert!(manager.error().is_none());
    assert_eq!(
        manager
            .snapshot()
            .status(Program::new(200, 10, 10000, 20000).unwrap()),
        Some(Status::Reserved)
    );
}

#[test]
fn remote_unknown_duration_is_not_a_reservable_program() {
    let schedule: data::Schedule = serde_json::from_value(json!({
        "id":100, "channelId":10, "startAt":10000, "endAt":20000,
        "isDurationUndefined":true
    }))
    .unwrap();
    assert!(!schedule.matches(program()));
}

#[test]
fn changing_verified_servers_clears_old_reservations_before_fetching_the_new_session() {
    let first = Fixture::new();
    let second = Fixture::new();
    let mut library = first.connect(false);
    first.mount(
        Mock::given(method("GET"))
            .and(path("/api/reserves"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"reserves":[reserve(7,100)],"total":1})),
            ),
    );
    first.empty_recording();
    second.connect(false);
    second.mount(
        Mock::given(method("GET"))
            .and(path("/api/reserves"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"reserves":[],"total":0})),
            )
            .expect(1),
    );
    second.empty_recording();
    let mut manager = Manager::default();
    library.synchronize_reservations(true, &mut manager, Instant::now());
    first.finish(&mut manager);
    assert_eq!(manager.snapshot().status(program()), Some(Status::Reserved));
    library
        .open(&first.network, &second.server.uri(), "", Login::Current)
        .unwrap();
    library.synchronize_reservations(true, &mut manager, Instant::now());
    assert!(!manager.enabled());
    assert_eq!(manager.snapshot().status(program()), None);
    let deadline = Instant::now() + Duration::from_secs(3);
    while library.busy() {
        library.poll(&first.network);
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    assert!(library.error().is_none());
    library.synchronize_reservations(true, &mut manager, Instant::now());
    first.finish(&mut manager);
    assert!(manager.enabled() && manager.can_reserve(program(), NOW));
}
