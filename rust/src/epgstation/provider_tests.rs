//! Executed only by scripts/epgstation-integration.py against pinned provider code.
//! Missing endpoints are errors, never silently successful/skipped live checks.
use super::*;
use crate::playback::recording::{Loader, Recording as LoadedRecording};
use std::{
    io::{Read, Write},
    net::TcpStream,
    thread,
    time::{Duration, Instant},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;
const DEADLINE: Duration = Duration::from_secs(45);
const POLL_INTERVAL: Duration = Duration::from_millis(1);
const FIRST_START_MS: i64 = 1_700_000_000_000 - 60_000;

enum Authentication {
    Anonymous,
    Password,
}
impl Authentication {
    fn login(&self) -> Login {
        match self {
            Self::Anonymous => Login::Current,
            Self::Password => Login::password("viewer".into(), "fixture-password".into()).unwrap(),
        }
    }
    fn endpoint(&self) -> Result<String, std::env::VarError> {
        std::env::var(match self {
            Self::Anonymous => "NAGAMETV_EPGSTATION_ANONYMOUS",
            Self::Password => "NAGAMETV_EPGSTATION_AUTHENTICATED",
        })
    }
}

fn finish(library: &mut Library, network: &Network) -> TestResult {
    let deadline = Instant::now() + DEADLINE;
    while library.busy() {
        library.poll(network);
        assert!(
            Instant::now() < deadline,
            "real provider operation exceeded deadline"
        );
        thread::sleep(POLL_INTERVAL);
    }
    if let Some(error) = library.error() {
        return Err(format!("real EPGStation catalogue failed: {error}").into());
    }
    assert!(library.loaded());
    Ok(())
}

fn runtime() -> std::io::Result<tokio::runtime::Runtime> {
    // The production metadata worker reuses a client created by the network
    // runtime. Keep that runtime's connection driver running between block_on's.
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
}

// Never include reqwest URLs in diagnostics: video URLs contain access tokens.
async fn send(request: reqwest::RequestBuilder) -> Result<reqwest::Response, reqwest::Error> {
    request.send().await.map_err(reqwest::Error::without_url)
}

#[test]
#[ignore = "requires pinned real provider; run python3 scripts/epgstation-integration.py"]
fn catalogue_pagination_metadata_and_file_bytes() -> TestResult {
    // Player construction initializes GStreamer before any file inspection.
    // Do the same here: a cold plugin scan must not consume the loader's bounded
    // inspection time or depend on another test having initialized it first.
    gstreamer::init()?;
    let network = Network::new()?;
    let runtime = runtime()?;
    let http = reqwest::Client::builder().timeout(DEADLINE).build()?;
    for auth in [Authentication::Anonymous, Authentication::Password] {
        let endpoint = auth.endpoint()?;
        let mut library = Library::default();
        library.open(&network, &endpoint, "", auth.login())?;
        finish(&mut library, &network)?;
        assert_eq!(library.rows().len(), PAGE_SIZE as usize);
        assert_eq!(library.rows()[0].id, 1);
        assert!(library.has_next());
        library.next(&network);
        finish(&mut library, &network)?;
        assert_eq!(library.offset(), PAGE_SIZE);
        assert_eq!(
            library.rows().iter().map(|row| row.id).collect::<Vec<_>>(),
            [51, 52]
        );
        assert!(!library.has_next());
        library.previous(&network);
        finish(&mut library, &network)?;
        assert_eq!(library.rows()[0].id, 1);

        library.open(&network, &endpoint, "EPGStation recording", Login::Current)?;
        finish(&mut library, &network)?;
        assert_eq!(library.rows().len(), 12);
        let row = &library.rows()[0];
        assert_eq!(row.start_ms, FIRST_START_MS);
        assert_eq!(row.channel, "Test TV");
        assert_eq!(
            row.service,
            Some(crate::channels::BroadcastService {
                network_id: 4,
                service_id: 101
            })
        );
        assert_eq!(
            library
                .files(2)
                .iter()
                .map(|file| file.id)
                .collect::<Vec<_>>(),
            [126, 124]
        );

        for (record, video, bytes) in [
            (
                1,
                123,
                include_bytes!("../../../tests/fixtures/recording-seek.ts").as_slice(),
            ),
            (
                2,
                124,
                include_bytes!("../../../tests/fixtures/media-h264.mp4").as_slice(),
            ),
            (
                3,
                125,
                include_bytes!("../../../tests/fixtures/media-hevc.mkv").as_slice(),
            ),
        ] {
            let url = library
                .video_url(record, video)
                .ok_or("missing video URL")?;
            runtime.block_on(async {
                let whole = send(http.get(&url)).await?;
                assert_eq!(whole.status(), reqwest::StatusCode::OK);
                assert_eq!(
                    whole
                        .bytes()
                        .await
                        .map_err(reqwest::Error::without_url)?
                        .as_ref(),
                    bytes
                );
                // Includes the single-byte and beyond-EOF cases that handcrafted
                // servers often implement differently from the real provider.
                for (range, start, end) in [
                    ("bytes=0-0".to_owned(), 0, 0),
                    ("bytes=188-375".to_owned(), 188, 375),
                    ("bytes=-20".to_owned(), bytes.len() - 20, bytes.len() - 1),
                    (
                        format!("bytes={}-", bytes.len() - 20),
                        bytes.len() - 20,
                        bytes.len() - 1,
                    ),
                ] {
                    let response = send(http.get(&url).header("range", range)).await?;
                    assert_eq!(response.status(), reqwest::StatusCode::PARTIAL_CONTENT);
                    assert_eq!(
                        response.headers()["content-range"],
                        format!("bytes {start}-{end}/{}", bytes.len())
                    );
                    assert_eq!(
                        response.headers()["content-length"],
                        (end - start + 1).to_string()
                    );
                    assert_eq!(
                        response
                            .bytes()
                            .await
                            .map_err(reqwest::Error::without_url)?
                            .as_ref(),
                        &bytes[start..=end]
                    );
                }
                // This pinned provider rejects an end beyond EOF instead of
                // clipping it (src/util/HttpRangeUtil.ts).
                for range in [
                    format!("bytes={}-", bytes.len()),
                    format!("bytes={}-{}", bytes.len() - 20, bytes.len() + 100),
                ] {
                    let invalid = send(http.get(&url).header("range", range)).await?;
                    assert_eq!(invalid.status(), reqwest::StatusCode::RANGE_NOT_SATISFIABLE);
                }
                Ok::<_, Box<dyn std::error::Error>>(())
            })?;
        }

        // Exercise the production file loader without decoding or media devices.
        for (record, video) in [(2, 124), (3, 125)] {
            let mut loader = Loader::default();
            loader.begin(
                library
                    .playback_request(record, Some(video))
                    .ok_or("playback request")?,
            );
            let deadline = Instant::now() + DEADLINE;
            let loaded = loop {
                if let Some((_, result)) = loader.poll() {
                    break result.map_err(|error| {
                        format!("EPGStation media inspection failed for video {video}: {error}")
                    })?;
                }
                assert!(
                    Instant::now() < deadline,
                    "real provider media inspection timed out"
                );
                thread::sleep(POLL_INTERVAL);
            };
            assert!(matches!(loaded, LoadedRecording::Media(_)));
        }
        // Explicitly exercise the optional metadata client with the real cookie.
        let access = match auth.login() {
            Login::Current => Access::Anonymous,
            Login::Password(credentials) => Access::Login(credentials),
        };
        let fetched = runtime.block_on(client::fetch(
            Endpoint::parse(&endpoint)?,
            format!("{endpoint}/api/recorded?isHalfWidth=false&limit=50"),
            access,
        ))?;
        let cancelled = std::sync::atomic::AtomicBool::new(false);
        assert_eq!(
            fetched.connection.metadata(124).start_ms(&cancelled),
            Some(FIRST_START_MS - 60_000)
        );

        library.open(&network, &endpoint, "no-such-recording-xyz", Login::Current)?;
        finish(&mut library, &network)?;
        assert!(library.rows().is_empty());
    }
    Ok(())
}

#[test]
#[ignore = "requires pinned real provider; run python3 scripts/epgstation-integration.py"]
fn authentication_rejects_invalid_credentials_and_scopes_tokens() -> TestResult {
    let endpoint = Authentication::Password.endpoint()?;
    let runtime = runtime()?;
    runtime.block_on(async {
        let bare = reqwest::Client::builder().timeout(DEADLINE).build()?;
        for route in [
            "/api/recorded",
            "/api/channels",
            "/api/auth/media-token",
            "/api/videos/124",
        ] {
            assert_eq!(
                send(bare.get(format!("{endpoint}{route}"))).await?.status(),
                reqwest::StatusCode::UNAUTHORIZED
            );
        }
        let denied = send(
            bare.post(format!("{endpoint}/api/auth/login"))
                .header("content-type", "application/json")
                .body(r#"{"name":"viewer","password":"incorrect-password"}"#),
        )
        .await?;
        assert_eq!(denied.status(), reqwest::StatusCode::UNAUTHORIZED);
        let client = reqwest::Client::builder()
            .cookie_store(true)
            .timeout(DEADLINE)
            .build()?;
        let login = send(
            client
                .post(format!("{endpoint}/api/auth/login"))
                .header("content-type", "application/json")
                .body(r#"{"name":"viewer","password":"fixture-password"}"#),
        )
        .await?;
        assert_eq!(login.status(), reqwest::StatusCode::OK);
        let cookie = login.headers()["set-cookie"].to_str()?;
        // Test structure without printing the cookie or token on assertion failure.
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("Path=/protected"));
        let response = send(client.get(format!("{endpoint}/api/auth/media-token"))).await?;
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let token: serde_json::Value = crate::json::from_slice(
            &response
                .bytes()
                .await
                .map_err(reqwest::Error::without_url)?,
        )?;
        let token = token["token"].as_str().ok_or("missing media token")?;
        assert!(!token.is_empty());
        let token_url = |route: &str| -> Result<url::Url, url::ParseError> {
            let mut url = url::Url::parse(&format!("{endpoint}{route}"))?;
            url.query_pairs_mut().append_pair("token", token);
            Ok(url)
        };
        assert_eq!(
            send(bare.get(token_url("/api/videos/124")?))
                .await?
                .status(),
            reqwest::StatusCode::OK
        );
        // A media token must not grant access to the recording catalogue.
        assert_eq!(
            send(bare.get(token_url("/api/recorded")?)).await?.status(),
            reqwest::StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            send(client.get(format!("{endpoint}/api/nonexistent-contract-route")))
                .await?
                .status(),
            reqwest::StatusCode::NOT_FOUND
        );
        Ok::<_, Box<dyn std::error::Error>>(())
    })?;
    let network = Network::new()?;
    let mut library = Library::default();
    library.open(
        &network,
        &endpoint,
        "",
        Login::password("viewer".into(), "incorrect-password".into())?,
    )?;
    assert!(finish(&mut library, &network).is_err());
    assert!(
        matches!(library.error(), Some(FetchError::Network(crate::services::NetworkError::Http(error)))
        if error.status() == Some(reqwest::StatusCode::UNAUTHORIZED))
    );
    library.open(&network, &endpoint, "", Authentication::Password.login())?;
    finish(&mut library, &network)?;
    library.open(
        &network,
        &Authentication::Anonymous.endpoint()?,
        "",
        Login::Current,
    )?;
    finish(&mut library, &network)?;
    assert!(
        url::Url::parse(&library.video_url(1, 123).ok_or("video")?)?
            .query()
            .is_none()
    );
    Ok(())
}

#[test]
#[ignore = "requires pinned real provider; run python3 scripts/epgstation-integration.py"]
fn cancelled_and_fragmented_logins_leave_the_provider_usable() -> TestResult {
    let endpoint = Authentication::Password.endpoint()?;
    let address = url::Url::parse(&endpoint)?.socket_addrs(|| None)?[0];
    let login = br#"{"name":"viewer","password":"fixture-password"}"#;
    let header = format!(
        "POST /protected/api/auth/login HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        login.len()
    );
    let mut incomplete = TcpStream::connect_timeout(&address, DEADLINE)?;
    incomplete.set_write_timeout(Some(DEADLINE))?;
    incomplete.set_read_timeout(Some(Duration::from_millis(100)))?;
    incomplete.write_all(header.as_bytes())?;
    incomplete.write_all(&login[..login.len() - 1])?;
    assert!(matches!(incomplete.read(&mut [0]), Err(error)
        if matches!(error.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut)));
    for _ in 0..5 {
        let mut aborted = TcpStream::connect_timeout(&address, DEADLINE)?;
        aborted.set_write_timeout(Some(DEADLINE))?;
        aborted.write_all(header.as_bytes())?;
        aborted.write_all(&login[..1])?;
        drop(aborted);
        let network = Network::new()?;
        let mut library = Library::default();
        library.open(
            &network,
            &endpoint,
            "EPGStation recording",
            Authentication::Password.login(),
        )?;
        finish(&mut library, &network)?;
    }
    incomplete.set_read_timeout(Some(DEADLINE))?;
    incomplete.write_all(&login[login.len() - 1..])?;
    let mut response = String::new();
    incomplete.read_to_string(&mut response)?;
    assert!(response.starts_with("HTTP/1.1 200"));
    Ok(())
}

#[test]
#[ignore = "requires pinned real provider; run python3 scripts/epgstation-integration.py"]
fn startup_fixture_matches_real_provider_responses() -> TestResult {
    let real = Authentication::Anonymous.endpoint()?;
    let mock = crate::startup_test_server::Server::new()?;
    let runtime = runtime()?;
    runtime.block_on(async {
        let http = reqwest::Client::builder().timeout(DEADLINE).build()?;
        for route in [
            "/api/recorded?isHalfWidth=false&limit=50&offset=0&keyword=EPGStation%20recording",
            "/api/channels",
            "/api/videos/124/metadata",
        ] {
            let mut bodies = Vec::new();
            for base in [&real, &mock.url()] {
                let response = send(http.get(format!("{base}{route}"))).await?;
                assert_eq!(response.status(), reqwest::StatusCode::OK);
                bodies.push(crate::json::from_slice::<serde_json::Value>(
                    &response
                        .bytes()
                        .await
                        .map_err(reqwest::Error::without_url)?,
                )?);
            }
            assert_eq!(
                bodies[0], bodies[1],
                "provider/fixture JSON differs for {route}"
            );
        }
        for video in [123, 124, 125] {
            for range in [
                None,
                Some("bytes=0-0"),
                Some("bytes=188-375"),
                Some("bytes=-20"),
                Some("bytes=100-"),
                Some("bytes=0-99999999"),
                Some("bytes=20-10"),
                Some("invalid"),
            ] {
                let mut results = Vec::new();
                for base in [&real, &mock.url()] {
                    let mut request = http.get(format!("{base}/api/videos/{video}"));
                    if let Some(range) = range {
                        request = request.header("range", range);
                    }
                    let response = send(request).await?;
                    let status = response.status();
                    let headers: Vec<_> = ["content-type", "content-range", "content-length"]
                        .into_iter()
                        .map(|key| response.headers().get(key).cloned())
                        .collect();
                    let bytes = response
                        .bytes()
                        .await
                        .map_err(reqwest::Error::without_url)?;
                    results.push((status, headers, bytes));
                }
                assert_eq!(
                    results[0].0, results[1].0,
                    "provider/fixture status differs: {video}, {range:?}"
                );
                if results[0].0.is_success() {
                    assert_eq!(
                        results[0].1, results[1].1,
                        "provider/fixture headers differ: {video}, {range:?}"
                    );
                    assert!(
                        results[0].2 == results[1].2,
                        "provider/fixture bytes differ: {video}, {range:?}"
                    );
                }
            }
        }
        for base in [&real, &mock.url()] {
            let response = send(http.get(format!("{base}/api/nonexistent-contract-route"))).await?;
            assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);
        }
        Ok::<_, Box<dyn std::error::Error>>(())
    })
}
