use super::*;
use serde_json::json;
use std::time::{Duration, Instant};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path, query_param},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;
struct Fixture {
    runtime: tokio::runtime::Runtime,
    server: MockServer,
    network: Network,
}
impl Fixture {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()?;
        Ok(Self {
            server: runtime.block_on(MockServer::start()),
            runtime,
            network: Network::new()?,
        })
    }
    fn mount(&self, mock: Mock) {
        self.runtime.block_on(mock.mount(&self.server));
    }
    fn request(&self, id: u64) -> Request {
        Request {
            key: Source::Epgstation {
                server: format!("{}/tv", self.server.uri()),
                recorded_id: id,
                video_file_id: 123,
            },
            seed: None,
            connection: None,
        }
    }
    fn finish(&self, fallback: &mut Fallback) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while fallback.busy() {
            fallback.poll(Some(&self.network));
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
fn record(id: u64) -> serde_json::Value {
    json!({"id":id,"name":"番組","description":"本文","extended":"詳細","channelName":"TV",
        "startAt":1000,"endAt":61000,"genre1":7,"subGenre1":0,"isRecording":false})
}

#[test]
fn resume_detail_is_bounded_asynchronous_and_source_scoped() -> TestResult {
    let fixture = Fixture::new()?;
    fixture.mount(
        Mock::given(method("GET"))
            .and(path("/tv/api/recorded/42"))
            .and(query_param("isHalfWidth", "false"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(record(42))
                    .set_delay(Duration::from_millis(80)),
            )
            .expect(1),
    );
    let mut fallback = Fallback::default();
    fallback.configure(Some(fixture.request(42)));
    fallback.poll(Some(&fixture.network));
    assert!(fallback.busy());
    assert!(fallback.data.is_none());
    fixture.finish(&mut fallback);
    let data: serde_json::Value = serde_json::from_str(fallback.data.as_ref().unwrap().json())?;
    assert_eq!(data["description"], "本文\n\n詳細");
    assert_eq!(data["genres"], json!([[7, 0]]));
    assert_eq!(data["duration"], 60000);
    assert_eq!(data["progressKnown"], false);
    for _ in 0..10 {
        fallback.configure(Some(fixture.request(42)));
        fallback.poll(Some(&fixture.network));
    }
    assert!(!fallback.busy());
    fallback.configure(None);
    assert!(fallback.data.is_none());

    fixture.mount(
        Mock::given(path("/tv/api/recorded/43"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(record(43))
                    .set_delay(Duration::from_millis(80)),
            )
            .expect(1),
    );
    fallback.configure(Some(fixture.request(43)));
    fallback.poll(Some(&fixture.network));
    let deadline = Instant::now() + Duration::from_secs(5);
    while fixture
        .runtime
        .block_on(fixture.server.received_requests())
        .unwrap()
        .len()
        < 2
    {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    let mut replacement = fixture.request(44);
    let mut replacement_record = record(44);
    replacement_record["name"] = json!("Replacement title");
    replacement.seed = Some(Information::from(&super::super::recording(
        serde_json::from_value(replacement_record)?,
    )?));
    fallback.configure(Some(replacement));
    fixture.finish(&mut fallback);
    assert_eq!(fallback.key, Some(fixture.request(44).key));
    assert_eq!(fallback.data.as_ref().unwrap().title(), "Replacement title");
    fallback.configure(None);
    fixture.finish(&mut fallback);
    assert!(fallback.data.is_none());
    fixture.runtime.block_on(fixture.server.verify());
    Ok(())
}

#[test]
fn detail_reuses_cookie_session_and_rejects_redirect_mismatch_and_oversize() -> TestResult {
    let fixture = Fixture::new()?;
    let _runtime_context = fixture.runtime.enter();
    fixture.mount(
        Mock::given(path("/tv/api/session")).respond_with(
            ResponseTemplate::new(200)
                .insert_header("set-cookie", "session=private; Path=/tv; HttpOnly"),
        ),
    );
    fixture.mount(
        Mock::given(path("/tv/api/recorded/1"))
            .and(header("cookie", "session=private"))
            .respond_with(ResponseTemplate::new(200).set_body_json(record(1)))
            .expect(1),
    );
    let connection =
        Connection::anonymous(Endpoint::parse(&format!("{}/tv", fixture.server.uri()))?)?;
    fixture.runtime.block_on(
        connection
            .api_request(reqwest::Method::GET, "session")
            .send(),
    )?;
    fixture
        .runtime
        .block_on(client::recorded(connection.clone(), 1))?;
    fixture.mount(
        Mock::given(path("/tv/api/recorded/2"))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("location", format!("{}/leak", fixture.server.uri())),
            )
            .expect(1),
    );
    fixture.mount(
        Mock::given(path("/leak"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0),
    );
    fixture.mount(
        Mock::given(path("/tv/api/recorded/3"))
            .respond_with(ResponseTemplate::new(200).set_body_json(record(4)))
            .expect(1),
    );
    fixture.mount(
        Mock::given(path("/tv/api/recorded/4"))
            .respond_with(
                ResponseTemplate::new(200).set_body_bytes(vec![b' '; 4 * 1024 * 1024 + 1]),
            )
            .expect(1),
    );
    for id in 2..=4 {
        assert!(
            fixture
                .runtime
                .block_on(client::recorded(connection.clone(), id))
                .is_err()
        );
    }
    fixture.mount(
        Mock::given(path("/tv/api/recorded/5"))
            .respond_with(ResponseTemplate::new(503))
            .expect(1),
    );
    let mut fallback = Fallback::default();
    fallback.configure(Some(fixture.request(5)));
    fixture.finish(&mut fallback);
    for _ in 0..10 {
        fallback.configure(Some(fixture.request(5)));
        fallback.poll(Some(&fixture.network));
    }
    assert!(fallback.data.is_none());
    assert!(!fallback.busy());
    fixture.runtime.block_on(fixture.server.verify());
    Ok(())
}
