use super::*;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

fn finish_probe(fixture: &Fixture, library: &mut Library) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while library.verifying_reservations() {
        library.poll_reservation_verification(&fixture.network);
        assert!(
            Instant::now() < deadline,
            "reservation verification did not finish"
        );
        thread::sleep(Duration::from_millis(1));
    }
}
#[derive(Clone, Default)]
struct Log(Arc<Mutex<Vec<u8>>>);
impl std::io::Write for Log {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn anonymous_probe_shares_the_verified_cookie_without_publishing_or_changing_search() -> TestResult
{
    let fixture = Fixture::new()?;
    fixture.mount(
        Mock::given(method("GET"))
            .and(path("/tv/api/recorded"))
            .and(query_param("limit", "50"))
            .and(query_param("offset", "0"))
            .and(query_param("isHalfWidth", "false"))
            .and(|request: &wiremock::Request| {
                !request.url.query_pairs().any(|(key, _)| key == "keyword")
            })
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("set-cookie", "verified=session; Path=/tv")
                    .set_body_json(json!({"records":[record(7,123)],"total":1})),
            )
            .expect(1),
    );
    fixture.mount(
        Mock::given(method("GET"))
            .and(path("/tv/api/recorded"))
            .and(query_param("keyword", "saved search"))
            .and(header("cookie", "verified=session"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"records":[record(8,456)],"total":1})),
            )
            .expect(1),
    );
    let endpoint = format!("{}/tv", fixture.server.uri());
    let mut library = Library::default();
    assert!(library.verify_for_reservations(Some(&fixture.network), true, &endpoint));
    assert!(!library.verify_for_reservations(Some(&fixture.network), true, &endpoint));
    assert!(!library.busy());
    finish_probe(&fixture, &mut library);
    assert!(library.session.is_some());
    assert!(!library.loaded());
    assert!(library.rows().is_empty());
    assert_eq!(library.total(), 0);
    assert_eq!(library.offset(), 0);
    assert_eq!(library.revision(), 0);
    assert!(library.error().is_none());
    assert!(
        library.poll(&fixture.network).is_none(),
        "background verification must not offer a settings-save result"
    );
    let requests = fixture
        .runtime
        .block_on(fixture.server.received_requests())
        .unwrap();
    assert_eq!(
        requests.len(),
        1,
        "verification does not fetch channel enrichment"
    );
    assert!(!requests[0].headers.contains_key("cookie"));
    let mut manager = reservations::Manager::default();
    library.synchronize_reservations(true, &mut manager, Instant::now());
    assert!(manager.enabled());
    library.open(&fixture.network, &endpoint, "saved search", Login::Current)?;
    fixture.finish(&mut library);
    let rows = library.rows();
    let revision = library.revision();
    assert!(!library.verify_for_reservations(Some(&fixture.network), true, &endpoint));
    assert!(Arc::ptr_eq(&rows, &library.rows()));
    assert_eq!(revision, library.revision());
    let Catalogue::Loaded { query, .. } = &library.catalogue else {
        panic!("recording page lost")
    };
    assert_eq!(query.keyword, "saved search");
    Ok(())
}

#[test]
fn disabled_is_silent_and_auth_failure_logs_once_until_an_explicit_retry() -> TestResult {
    let fixture = Fixture::new()?;
    let allowed = Arc::new(AtomicBool::new(false));
    let permit = allowed.clone();
    fixture.mount(
        Mock::given(method("GET"))
            .and(path("/private-path/api/recorded"))
            .respond_with(move |_: &wiremock::Request| {
                if permit.load(Ordering::SeqCst) {
                    ResponseTemplate::new(200).set_body_json(json!({"records":[],"total":0}))
                } else {
                    ResponseTemplate::new(401).set_body_string("private-response")
                }
            })
            .expect(2),
    );
    let log = Log::default();
    let writer = log.clone();
    let subscriber = tracing_subscriber::fmt()
        .without_time()
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    let endpoint = format!("{}/private-path", fixture.server.uri());
    let mut library = Library::default();
    assert!(!library.verify_for_reservations(Some(&fixture.network), false, &endpoint));
    for _ in 0..10 {
        library.poll_reservation_verification(&fixture.network);
    }
    assert!(
        fixture
            .runtime
            .block_on(fixture.server.received_requests())
            .unwrap()
            .is_empty()
    );
    assert!(library.verify_for_reservations(Some(&fixture.network), true, &endpoint));
    finish_probe(&fixture, &mut library);
    for _ in 0..10 {
        library.poll_reservation_verification(&fixture.network);
    }
    assert!(library.session.is_none());
    assert!(library.error().is_none());
    assert_eq!(library.revision(), 0);
    let output = String::from_utf8(log.0.lock().unwrap().clone())?;
    assert_eq!(
        output
            .matches("EPGStation reservation session remains unverified")
            .count(),
        1
    );
    assert!(output.contains("401") && output.contains("verify EPGStation reservation session"));
    assert!(!output.contains("private-path") && !output.contains("private-response"));
    allowed.store(true, Ordering::SeqCst);
    assert!(library.verify_for_reservations(Some(&fixture.network), true, &endpoint));
    finish_probe(&fixture, &mut library);
    assert!(library.session.is_some());
    assert_eq!(
        String::from_utf8(log.0.lock().unwrap().clone())?
            .matches("EPGStation reservation session remains unverified")
            .count(),
        1
    );
    Ok(())
}

#[test]
fn invalid_responses_and_redirects_never_verify_or_expose_response_secrets() -> TestResult {
    for response in [
        ResponseTemplate::new(503),
        ResponseTemplate::new(200).set_body_json(json!({"records":[],"total":"private-value"})),
        ResponseTemplate::new(200).set_body_bytes(vec![b' '; MAX_RESPONSE_BYTES + 1]),
        ResponseTemplate::new(302)
            .insert_header("location", "http://example.invalid/private-token"),
    ] {
        let fixture = Fixture::new()?;
        fixture.mount(
            Mock::given(method("GET"))
                .and(path("/api/recorded"))
                .respond_with(response)
                .expect(1),
        );
        let log = Log::default();
        let writer = log.clone();
        let subscriber = tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);
        let mut library = Library::default();
        library.verify_for_reservations(Some(&fixture.network), true, &fixture.server.uri());
        finish_probe(&fixture, &mut library);
        assert!(library.session.is_none() && library.error().is_none());
        let output = String::from_utf8(log.0.lock().unwrap().clone())?;
        assert_eq!(
            output
                .matches("EPGStation reservation session remains unverified")
                .count(),
            1
        );
        assert!(!output.contains("private-value") && !output.contains("private-token"));
    }
    Ok(())
}

#[test]
fn disable_and_shutdown_discard_pending_proof_and_reenable_waits_for_cancellation() -> TestResult {
    let fixture = Fixture::new()?;
    let started = Arc::new(AtomicBool::new(false));
    let observed = started.clone();
    fixture.mount(
        Mock::given(method("GET"))
            .and(path("/slow/api/recorded"))
            .respond_with(move |_: &wiremock::Request| {
                observed.store(true, Ordering::SeqCst);
                ResponseTemplate::new(200)
                    .set_body_json(json!({"records":[],"total":0}))
                    .set_delay(Duration::from_secs(10))
            })
            .expect(1),
    );
    fixture.mount(
        Mock::given(method("GET"))
            .and(path("/api/recorded"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"records":[],"total":0})))
            .expect(1),
    );
    let mut library = Library::default();
    library.verify_for_reservations(
        Some(&fixture.network),
        true,
        &format!("{}/slow", fixture.server.uri()),
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    while !started.load(Ordering::SeqCst) {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    library.verify_for_reservations(Some(&fixture.network), false, &fixture.server.uri());
    assert!(library.session.is_none());
    assert!(library.verify_for_reservations(Some(&fixture.network), true, &fixture.server.uri()));
    finish_probe(&fixture, &mut library);
    assert_eq!(
        library.session.as_ref().unwrap().0.as_str(),
        fixture.server.uri()
    );
    library.session = None;
    library.close_reservation_verification();
    assert!(!library.verify_for_reservations(Some(&fixture.network), true, &fixture.server.uri()));
    finish_probe(&fixture, &mut library);
    assert!(library.session.is_none());
    Ok(())
}

#[test]
fn login_discards_anonymous_proof_even_when_it_has_already_arrived() -> TestResult {
    let fixture = Fixture::new()?;
    fixture.mount(
        Mock::given(method("GET"))
            .and(path("/api/recorded"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"records":[],"total":0})))
            .expect(2),
    );
    fixture.mount(
        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(
                ResponseTemplate::new(200).insert_header("set-cookie", "auth=logged-in; Path=/"),
            )
            .expect(1),
    );
    fixture.mount(
        Mock::given(method("GET"))
            .and(path("/api/auth/media-token"))
            .and(header("cookie", "auth=logged-in"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"token":"private-token"})),
            )
            .expect(1),
    );
    let mut library = Library::default();
    library.verify_for_reservations(Some(&fixture.network), true, &fixture.server.uri());
    let deadline = Instant::now() + Duration::from_secs(3);
    while !matches!(&library.verification, super::super::verification::Verification::Fetching { job, .. } if job.is_finished())
    {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    library.open(
        &fixture.network,
        &fixture.server.uri(),
        "",
        Login::password("viewer".into(), "password".into())?,
    )?;
    assert!(!library.verify_for_reservations(Some(&fixture.network), true, &fixture.server.uri()));
    fixture.finish(&mut library);
    finish_probe(&fixture, &mut library);
    assert!(
        library
            .session
            .as_ref()
            .unwrap()
            .1
            .video_url(123)
            .contains("token=private-token")
    );
    Ok(())
}
