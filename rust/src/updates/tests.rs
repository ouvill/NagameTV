use super::*;
use std::{thread, time::Instant};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn release(tag: &str) -> serde_json::Value {
    serde_json::json!({"tag_name": tag, "prerelease": false, "draft": false})
}

fn parse_release(tag: &str, current: &str) -> Outcome {
    parse(&serde_json::to_vec(&release(tag)).unwrap(), current).unwrap()
}

#[test]
fn only_newer_stable_versions_are_offered() {
    for (tag, current, available) in [
        ("v0.3.2", "0.3.1", true),
        ("v0.10.0", "0.9.0", true),
        ("v1.0.0", "0.99.0", true),
        ("0.3.2", "0.3.1", true),
        ("v0.3.1", "0.3.1", false),
        ("v0.3.0", "0.3.1", false),
        ("v0.3.1+build.2", "0.3.1+build.1", false),
        ("v0.3.1", "0.3.1-rc.1", true),
        ("v0.3.1", "0.4.0-dev", false),
    ] {
        assert_eq!(
            matches!(parse_release(tag, current), Outcome::Available(_)),
            available,
            "{tag} / {current}"
        );
    }
}

#[test]
fn prereleases_and_drafts_are_excluded_even_when_returned_by_the_endpoint() {
    for (tag, prerelease, draft) in [
        ("latest-build", true, false),
        ("v99.0.0", true, false),
        ("v99.0.0", false, true),
        ("v99.0.0-rc.1", false, false),
    ] {
        let bytes = serde_json::to_vec(
            &serde_json::json!({"tag_name":tag, "prerelease":prerelease, "draft":draft}),
        )
        .unwrap();
        assert_eq!(parse(&bytes, "0.3.1").unwrap(), Outcome::NoRelease);
    }
}

#[test]
fn malformed_responses_cannot_report_success_or_supply_external_links() {
    for bytes in [
        b"not json".as_slice(),
        br#"{"tag_name":"v1.0.0"}"#,
        br#"{"tag_name":42,"prerelease":false,"draft":false}"#,
        br#"{"tag_name":"latest-build","prerelease":false,"draft":false}"#,
        br#"{"tag_name":"v1.0.0/../../other","prerelease":false,"draft":false}"#,
        br#"{"tag_name":"v1.0.0","prerelease":false,"draft":false} []"#,
    ] {
        assert!(parse(bytes, "0.3.1").is_err());
    }
    let mut response = release("v1.0.0");
    response["html_url"] = "https://untrusted.example/download".into();
    let Outcome::Available(release) =
        parse(&serde_json::to_vec(&response).unwrap(), "0.3.1").unwrap()
    else {
        panic!("new release expected");
    };
    assert_eq!(
        release.url,
        "https://github.com/ouvill/NagameTV/releases/tag/v1.0.0"
    );
}

fn wait(checker: &mut Checker) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while matches!(checker.status(), Status::Checking) {
        assert!(Instant::now() < deadline, "update request did not finish");
        checker.poll();
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn checks_are_manual_bounded_and_deduplicated() -> TestResult {
    let runtime = tokio::runtime::Runtime::new()?;
    let server = runtime.block_on(MockServer::start());
    runtime.block_on(
        Mock::given(method("GET"))
            .and(path("/releases/latest"))
            .and(header("accept", "application/vnd.github+json"))
            .and(header(
                "user-agent",
                concat!("NagameTV/", env!("CARGO_PKG_VERSION")),
            ))
            .and(header("X-GitHub-Api-Version", "2026-03-10"))
            .respond_with(ResponseTemplate::new(200).set_body_json(release("v0.4.0")))
            .expect(1)
            .mount(&server),
    );
    let network = Network::new()?;
    let mut checker = Checker::default();
    assert!(matches!(checker.status(), Status::Idle));
    assert!(!checker.poll());
    assert!(
        runtime
            .block_on(server.received_requests())
            .unwrap()
            .is_empty()
    );
    assert!(checker.start(
        Some(&network),
        format!("{}/releases/latest", server.uri()),
        "0.3.1"
    ));
    assert!(!checker.start(Some(&network), server.uri(), "0.3.1"));
    wait(&mut checker);
    assert!(matches!(
        checker.status(),
        Status::Checked(Outcome::Available(_))
    ));
    runtime.block_on(server.verify());
    Ok(())
}

#[test]
fn no_release_is_distinct_from_http_failure_and_a_failed_check_can_be_retried() -> TestResult {
    let runtime = tokio::runtime::Runtime::new()?;
    let server = runtime.block_on(MockServer::start());
    let network = Network::new()?;
    let mut checker = Checker::default();
    for status in [404, 403, 429, 500, 302, 200] {
        runtime.block_on(server.reset());
        runtime.block_on(
            Mock::given(method("GET"))
                .respond_with(ResponseTemplate::new(status).set_body_json(release("v0.3.1")))
                .expect(1)
                .mount(&server),
        );
        assert!(checker.start(Some(&network), server.uri(), "0.3.1"));
        wait(&mut checker);
        match status {
            404 => assert!(matches!(
                checker.status(),
                Status::Checked(Outcome::NoRelease)
            )),
            200 => assert!(matches!(
                checker.status(),
                Status::Checked(Outcome::UpToDate)
            )),
            _ => assert!(matches!(checker.status(), Status::Failed(_))),
        }
        runtime.block_on(server.verify());
    }
    Ok(())
}

#[test]
fn oversized_and_invalid_json_responses_fail() -> TestResult {
    let runtime = tokio::runtime::Runtime::new()?;
    let server = runtime.block_on(MockServer::start());
    let network = Network::new()?;
    let mut checker = Checker::default();
    for bytes in [vec![b' '; MAX_RESPONSE_BYTES + 1], b"invalid".to_vec()] {
        runtime.block_on(server.reset());
        runtime.block_on(
            Mock::given(method("GET"))
                .respond_with(ResponseTemplate::new(200).set_body_bytes(bytes))
                .mount(&server),
        );
        checker.start(Some(&network), server.uri(), "0.3.1");
        wait(&mut checker);
        assert!(matches!(checker.status(), Status::Failed(_)));
    }
    Ok(())
}

#[test]
fn shutdown_discards_queued_results_and_prevents_new_requests() -> TestResult {
    let network = Network::new()?;
    let job = network.job(async {
        Ok(Outcome::Available(Release {
            version: Version::new(1, 0, 0),
            url: format!("{RELEASES}v1.0.0"),
        }))
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while !job.is_finished() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    let mut checker = Checker {
        state: State::Checking(job),
        open_error: None,
        ..Checker::default()
    };
    checker.stop();
    checker.poll();
    assert!(matches!(checker.state, State::Stopped));
    assert!(!checker.start(Some(&network), "http://unused.invalid".into(), "0.3.1"));
    assert!(!checker.open_release(|_| panic!("cancelled result must not open a browser")));

    let task = network.job(std::future::pending());
    let mut pending = Checker {
        state: State::Checking(task),
        open_error: None,
        ..Checker::default()
    };
    pending.stop();
    while !matches!(pending.state, State::Stopped) {
        assert!(Instant::now() < deadline);
        pending.poll();
        thread::sleep(Duration::from_millis(1));
    }
    Ok(())
}

#[test]
fn browser_opening_requires_an_available_release_and_failures_can_be_retried() {
    let mut checker = Checker::default();
    assert!(!checker.open_release(|_| panic!("unchecked releases cannot be opened")));
    checker.state = State::Checked(parse_release("v1.0.0", "0.3.1"));
    assert!(!checker.open_release(|url| {
        assert_eq!(
            url,
            "https://github.com/ouvill/NagameTV/releases/tag/v1.0.0"
        );
        false
    }));
    assert!(checker.open_error().is_some());
    assert!(matches!(
        checker.status(),
        Status::Checked(Outcome::Available(_))
    ));
    assert!(checker.open_release(|_| true));
    assert!(checker.open_error().is_none());
}

#[test]
fn unavailable_network_is_reported_without_starting_a_request() {
    let mut checker = Checker::default();
    assert!(checker.check(None));
    assert!(matches!(
        checker.status(),
        Status::Failed(FetchError::Parse(Error::Unavailable))
    ));
}

fn scheduled(path: &std::path::Path, enabled: bool) -> Checker {
    Checker {
        history: history::History::open(Ok(path.into())),
        automatic: if enabled {
            Automatic::Enabled
        } else {
            Automatic::Disabled
        },
        ..Checker::default()
    }
}

fn wait_at(checker: &mut Checker, now: SystemTime) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while matches!(checker.status(), Status::Checking) {
        assert!(Instant::now() < deadline);
        checker.poll_at(now);
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn automatic_checks_wait_a_day_across_restarts_and_manual_checks_bypass_the_interval() -> TestResult
{
    let runtime = tokio::runtime::Runtime::new()?;
    let server = runtime.block_on(MockServer::start());
    runtime.block_on(
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(release("v999.0.0")))
            .expect(3)
            .mount(&server),
    );
    let network = Network::new()?;
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("updates.toml");
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    let mut checker = scheduled(&path, false);
    assert!(!checker.tick_endpoint(Some(&network), now, &server.uri()));
    assert!(!path.exists());
    assert!(checker.configure_automatic(true));
    assert!(checker.tick_endpoint(Some(&network), now, &server.uri()));
    wait_at(&mut checker, now);
    assert!(matches!(
        checker.status(),
        Status::Checked(Outcome::Available(_))
    ));
    assert!(checker.last_success().is_some());
    let mut restarted = scheduled(&path, true);
    assert_eq!(checker.last_success(), restarted.last_success());
    let tomorrow = now + history::CHECK_INTERVAL;
    assert!(!restarted.tick_endpoint(
        Some(&network),
        tomorrow - Duration::from_secs(1),
        &server.uri()
    ));
    restarted.configure_automatic(false);
    assert!(!restarted.tick_endpoint(Some(&network), tomorrow, &server.uri()));
    restarted.configure_automatic(true);
    assert!(restarted.tick_endpoint(Some(&network), tomorrow, &server.uri()));
    wait_at(&mut restarted, tomorrow);
    assert!(restarted.start_at(
        Some(&network),
        server.uri(),
        "0.3.1",
        tomorrow + Duration::from_secs(1),
        Trigger::Manual
    ));
    wait_at(&mut restarted, tomorrow + Duration::from_secs(1));
    assert!(!restarted.tick_endpoint(
        Some(&network),
        tomorrow + history::CHECK_INTERVAL,
        &server.uri()
    ));
    runtime.block_on(server.verify());
    Ok(())
}

#[test]
fn failed_automatic_checks_record_attempts_without_claiming_success() -> TestResult {
    let runtime = tokio::runtime::Runtime::new()?;
    let server = runtime.block_on(MockServer::start());
    runtime.block_on(
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(500))
            .expect(1)
            .mount(&server),
    );
    let network = Network::new()?;
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("updates.toml");
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    let mut checker = scheduled(&path, true);
    checker.tick_endpoint(Some(&network), now, &server.uri());
    wait_at(&mut checker, now);
    assert!(matches!(checker.status(), Status::Failed(_)));
    assert_eq!(checker.last_success(), None);
    let mut restarted = scheduled(&path, true);
    assert!(!restarted.tick_endpoint(Some(&network), now + Duration::from_secs(1), &server.uri()));
    assert!(std::fs::read_to_string(path)?.contains("last_attempt_at = 1700000000"));
    runtime.block_on(server.verify());
    Ok(())
}

#[test]
fn history_failures_suspend_automatic_checks_but_keep_manual_checks_available() -> TestResult {
    let runtime = tokio::runtime::Runtime::new()?;
    let server = runtime.block_on(MockServer::start());
    runtime.block_on(
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404))
            .expect(1)
            .mount(&server),
    );
    let network = Network::new()?;
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("state/updates.toml");
    let mut checker = scheduled(&path, true);
    std::fs::write(directory.path().join("state"), "not a directory")?;
    checker.tick_endpoint(Some(&network), SystemTime::now(), &server.uri());
    assert!(checker.history_error().is_some());
    assert!(matches!(checker.status(), Status::Idle));
    assert!(
        runtime
            .block_on(server.received_requests())
            .unwrap()
            .is_empty()
    );
    assert!(checker.start(Some(&network), server.uri(), "0.3.1"));
    wait(&mut checker);
    assert!(matches!(
        checker.status(),
        Status::Checked(Outcome::NoRelease)
    ));
    assert!(checker.history_error().is_some());
    runtime.block_on(server.verify());
    Ok(())
}
