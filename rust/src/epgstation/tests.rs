use super::*;
use crate::services::NetworkError;
use serde_json::{Value, json};
use std::{
    thread,
    time::{Duration, Instant},
};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_json, header, method, path, query_param},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;
fn record(id: u64, video: u64) -> Value {
    json!({"id":id,"name":"録画番組","startAt":1000,"endAt":2000,"isRecording":false,
        "videoFiles":[{"id":video,"type":"ts","size":1880}]})
}
struct Fixture {
    network: Network,
    server: MockServer,
    runtime: tokio::runtime::Runtime,
}
impl Fixture {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()?;
        Ok(Self {
            network: Network::new()?,
            server: runtime.block_on(MockServer::start()),
            runtime,
        })
    }
    fn mount(&self, mock: Mock) {
        self.runtime.block_on(mock.mount(&self.server));
    }
    fn finish(&self, library: &mut Library) -> Option<VerifiedEndpoint> {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut verified = None;
        while library.busy() {
            if let Some(result) = library.poll(&self.network) {
                verified = Some(result);
            }
            assert!(Instant::now() < deadline, "catalogue request timed out");
            thread::sleep(Duration::from_millis(1));
        }
        verified
    }
}

#[test]
fn common_api_search_paging_and_video_ids() -> TestResult {
    let fixture = Fixture::new()?;
    let endpoint = format!("{}/tv", fixture.server.uri());
    let keyword = "アニメ & ニュース + #";
    fixture.mount(
        Mock::given(method("GET"))
            .and(path("/tv/api/recorded"))
            .and(query_param("isHalfWidth", "false"))
            .and(query_param("keyword", keyword))
            .and(query_param("offset", "0"))
            .and(query_param("limit", "50"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"records":[record(7,123)],"total":51})),
            )
            .expect(2),
    );
    fixture.mount(
        Mock::given(method("GET"))
            .and(path("/tv/api/recorded"))
            .and(query_param("offset", "50"))
            .and(query_param("keyword", keyword))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"records":[record(8,456)],"total":51})),
            )
            .expect(1),
    );
    let mut library = Library::default();
    library.open(&fixture.network, &endpoint, keyword, Login::Current)?;
    assert!(library.playback_url(7).is_none());
    let verified = fixture.finish(&mut library).ok_or("no verified server")?;
    assert_eq!(verified.as_str(), endpoint);
    assert_eq!(
        library.playback_url(7),
        Some(format!("{endpoint}/api/videos/123"))
    );
    assert!(
        library.playback_url(123).is_none(),
        "a file ID is not a recording ID"
    );
    assert!(library.has_next());
    library.next(&fixture.network);
    fixture.finish(&mut library);
    assert!(library.has_previous());
    assert!(!library.has_next());
    assert_eq!(library.offset(), 50);
    assert!(
        library.playback_url(7).is_none(),
        "old pages cannot be played"
    );
    library.previous(&fixture.network);
    fixture.finish(&mut library);
    assert_eq!(library.offset(), 0);
    fixture.runtime.block_on(fixture.server.verify());
    Ok(())
}

#[test]
fn authenticated_session_token_and_server_isolation() -> TestResult {
    let fixture = Fixture::new()?;
    fixture.mount(
        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .and(body_json(json!({"name":"viewer","password":"test secret"})))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header(
                        "set-cookie",
                        "epgstation_session=session-fixture; Path=/; HttpOnly",
                    )
                    .set_body_json(json!({"user":{"id":1,"name":"viewer"}})),
            )
            .expect(1),
    );
    fixture.mount(
        Mock::given(method("GET"))
            .and(path("/api/auth/media-token"))
            .and(header("cookie", "epgstation_session=session-fixture"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"token":"media+&?token"})),
            )
            .expect(1),
    );
    fixture.mount(
        Mock::given(method("GET"))
            .and(path("/api/recorded"))
            .and(header("cookie", "epgstation_session=session-fixture"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"records":[record(7,123)],"total":1})),
            )
            .expect(2),
    );
    let mut library = Library::default();
    library.open(
        &fixture.network,
        &fixture.server.uri(),
        "",
        Login::password("viewer".into(), "test secret".into())?,
    )?;
    assert!(fixture.finish(&mut library).is_some());
    let url = url::Url::parse(&library.playback_url(7).ok_or("missing playback URL")?)?;
    assert_eq!(
        url.query_pairs().find(|(key, _)| key == "token").unwrap().1,
        "media+&?token"
    );
    library.open(
        &fixture.network,
        &fixture.server.uri(),
        "another search",
        Login::Current,
    )?;
    assert!(
        fixture.finish(&mut library).is_some(),
        "search reuses the login"
    );
    let other = Fixture::new()?;
    other.mount(
        Mock::given(method("GET"))
            .and(path("/api/recorded"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"records":[record(7,456)],"total":1})),
            )
            .expect(1),
    );
    library.open(&other.network, &other.server.uri(), "", Login::Current)?;
    assert!(other.finish(&mut library).is_some());
    assert!(!library.playback_url(7).unwrap().contains("token"));
    let requests = other
        .runtime
        .block_on(other.server.received_requests())
        .unwrap();
    assert!(requests[0].headers.get("cookie").is_none());
    fixture.runtime.block_on(fixture.server.verify());
    Ok(())
}

#[test]
fn cancellation_replacement_and_failures_never_commit_old_results() -> TestResult {
    let fixture = Fixture::new()?;
    fixture.mount(
        Mock::given(method("GET"))
            .and(path("/api/recorded"))
            .and(query_param("keyword", "slow"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_millis(200))
                    .set_body_json(json!({"records":[record(1,111)],"total":1})),
            ),
    );
    fixture.mount(
        Mock::given(method("GET"))
            .and(path("/api/recorded"))
            .and(query_param("keyword", "new"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"records":[record(2,222)],"total":1})),
            )
            .expect(1),
    );
    fixture.mount(
        Mock::given(method("GET"))
            .and(path("/api/recorded"))
            .and(query_param("keyword", "denied"))
            .respond_with(ResponseTemplate::new(401))
            .expect(1),
    );
    let mut library = Library::default();
    library.open(
        &fixture.network,
        &fixture.server.uri(),
        "slow",
        Login::Current,
    )?;
    library.open(
        &fixture.network,
        &fixture.server.uri(),
        "new",
        Login::Current,
    )?;
    assert!(fixture.finish(&mut library).is_some());
    assert_eq!(library.rows()[0].id, 2);
    library.open(
        &fixture.network,
        &fixture.server.uri(),
        "slow",
        Login::Current,
    )?;
    library.cancel();
    assert!(fixture.finish(&mut library).is_none());
    assert!(!library.loaded());
    library.open(
        &fixture.network,
        &fixture.server.uri(),
        "denied",
        Login::Current,
    )?;
    assert!(fixture.finish(&mut library).is_none());
    assert!(
        matches!(library.error(), Some(FetchError::Network(NetworkError::Http(error))) if error.status() == Some(reqwest::StatusCode::UNAUTHORIZED))
    );
    assert!(library.rows().is_empty());
    Ok(())
}

#[test]
fn failed_login_does_not_follow_redirects_or_request_a_catalogue() -> TestResult {
    for status in [401, 302] {
        let fixture = Fixture::new()?;
        fixture.mount(
            Mock::given(method("POST"))
                .and(path("/api/auth/login"))
                .respond_with(
                    ResponseTemplate::new(status).insert_header("location", "/redirected"),
                )
                .expect(1),
        );
        let mut library = Library::default();
        library.open(
            &fixture.network,
            &fixture.server.uri(),
            "",
            Login::password("name".into(), "secret".into())?,
        )?;
        assert!(fixture.finish(&mut library).is_none());
        assert!(!library.loaded());
        let requests = fixture
            .runtime
            .block_on(fixture.server.received_requests())
            .unwrap();
        assert!(
            !requests
                .iter()
                .any(|r| r.url.path() == "/redirected" || r.url.path() == "/api/recorded")
        );
        assert!(!library.error().unwrap().to_string().contains("secret"));
    }
    Ok(())
}

#[test]
fn malformed_and_unplayable_recordings_are_distinguished() -> TestResult {
    let mut recording = record(u64::MAX, 123);
    recording["isRecording"] = json!(true);
    let mut encoded = record(2, 234);
    encoded["videoFiles"][0]["type"] = json!("encoded");
    let parsed = parse(&serde_json::to_vec(
        &json!({"records":[recording.clone(),encoded],"total":2}),
    )?)?;
    assert_eq!(parsed.rows[0].id, u64::MAX);
    assert_eq!(parsed.rows[0].availability, Availability::Recording);
    assert_eq!(
        parsed.rows[1].availability.files()[0].kind,
        VideoType::Encoded
    );
    for body in [
        json!({"records":[recording.clone(),recording],"total":2}),
        json!({"records":[record(1,2)],"total":0}),
        json!({"records":[{"id":"wrong"}],"total":1}),
    ] {
        assert!(parse(&serde_json::to_vec(&body)?).is_err());
    }
    assert!(parse(b"{\"records\":[],\"total\":0} null").is_err());
    for address in [
        "ftp://server",
        "http://user:secret@server",
        "http://server?token=secret",
    ] {
        assert!(Endpoint::parse(address).is_err());
    }
    Ok(())
}

#[test]
fn oversized_response_cannot_commit_a_connection() -> TestResult {
    let fixture = Fixture::new()?;
    fixture.mount(
        Mock::given(method("GET"))
            .and(path("/api/recorded"))
            .respond_with(
                ResponseTemplate::new(200).set_body_bytes(vec![b' '; MAX_RESPONSE_BYTES + 1]),
            )
            .expect(1),
    );
    let mut library = Library::default();
    library.open(&fixture.network, &fixture.server.uri(), "", Login::Current)?;
    assert!(fixture.finish(&mut library).is_none());
    assert!(matches!(
        library.error(),
        Some(FetchError::Network(NetworkError::ResponseTooLarge { .. }))
    ));
    assert!(!library.loaded());
    Ok(())
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(24))]
    #[test]
    fn only_the_latest_uncancelled_search_can_publish(actions in proptest::collection::vec(0_u8..3, 1..24)) {
        let fixture = Fixture::new().unwrap();
        fixture.mount(Mock::given(method("GET")).and(path("/api/recorded"))
            .respond_with(|request: &wiremock::Request| {
                let id = request.url.query_pairs().find(|(key,_)| key == "keyword").unwrap().1.parse::<u64>().unwrap();
                ResponseTemplate::new(200).set_body_json(json!({"records":[record(id,id)],"total":1}))
            }));
        let mut library = Library::default();
        let mut expected = None;
        for (id, action) in actions.into_iter().enumerate() {
            match action {
                0 => {
                    library.open(&fixture.network, &fixture.server.uri(), &id.to_string(), Login::Current).unwrap();
                    expected = Some(id as u64);
                },
                1 => { library.cancel(); expected = library.rows().first().map(|row| row.id); },
                2 => { library.poll(&fixture.network); },
                _ => unreachable!(),
            }
            if library.busy() {
                proptest::prop_assert!(library.playback_url(id as u64).is_none());
            }
        }
        fixture.finish(&mut library);
        proptest::prop_assert!(library.error().is_none());
        proptest::prop_assert_eq!(library.rows().first().map(|row| row.id), expected);
    }
}

proptest::proptest! {
    #[test]
    fn search_text_cannot_change_query_structure(keyword in ".{0,256}") {
        let query = Query { endpoint: Endpoint::parse("http://server/sub").unwrap(), keyword: keyword.clone(), offset: 0 };
        let url = url::Url::parse(&query.url()).unwrap();
        proptest::prop_assert_eq!(url.path(), "/sub/api/recorded");
        let actual = url.query_pairs().find(|(key,_)| key == "keyword").map(|(_,value)| value.into_owned()).unwrap_or_default();
        proptest::prop_assert_eq!(actual, keyword);
    }
}

#[test]
fn encoded_candidates_keep_stable_ids_and_reject_stale_or_unrelated_choices() -> TestResult {
    let fixture = Fixture::new()?;
    let mut row = record(7, 123);
    row["videoFiles"] = json!([
        {"id":234,"type":"encoded","name":"HEVC","filename":"日本語.mkv","size":2048},
        {"id":123,"type":"ts","size":1880},
        {"id":345,"type":"encoded","name":"H.264","filename":"日本語.mp4","size":2048},
        {"id":456,"type":"encoded","size":0},
        {"id":567,"type":"future","size":2048}
    ]);
    fixture.mount(
        Mock::given(method("GET"))
            .and(path("/api/recorded"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"records":[row.clone(),record(8,999)],"total":2})),
            ),
    );
    let mut library = Library::default();
    library.open(&fixture.network, &fixture.server.uri(), "", Login::Current)?;
    fixture.finish(&mut library).ok_or("catalogue")?;
    assert_eq!(
        library
            .files(7)
            .iter()
            .map(|file| file.id)
            .collect::<Vec<_>>(),
        [123, 234, 345]
    );
    assert_eq!(library.files(7)[1].filename, "日本語.mkv");
    assert_eq!(
        library.video_url(7, 234),
        Some(format!("{}/api/videos/234", fixture.server.uri()))
    );
    for id in [456, 567, 999] {
        assert!(library.video_url(7, id).is_none());
    }
    library.open(
        &fixture.network,
        &fixture.server.uri(),
        "changed",
        Login::Current,
    )?;
    assert!(library.video_url(7, 234).is_none());
    assert!(library.files(7).is_empty());
    row["videoFiles"] =
        json!([{"id":123,"type":"ts","size":100},{"id":123,"type":"encoded","size":100}]);
    assert!(parse(&serde_json::to_vec(&json!({"records":[row],"total":1}))?).is_err());
    Ok(())
}
