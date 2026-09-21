use super::*;
use crate::{
    connection::State,
    controller::Controller,
    retry::{LIVE_DELAY, STABLE_CONNECTION},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    time::timeout,
};

type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;
const TEST_DEADLINE: Duration = Duration::from_secs(3);
const MAX_JITTER: Duration = Duration::from_secs(30);
const BEFORE_DEADLINE: Duration = Duration::from_millis(1);

fn runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}
fn endpoints(listener: &TcpListener) -> Endpoints {
    let address = listener.local_addr().unwrap();
    Endpoints {
        threads: format!("http://{address}/threads"),
        comments: format!("ws://{address}/comments"),
    }
}
async fn response(listener: &TcpListener, status: u16, headers: &str, body: &str) {
    let (mut socket, _) = timeout(TEST_DEADLINE, listener.accept())
        .await
        .unwrap()
        .unwrap();
    let mut request = Vec::new();
    while !request.ends_with(b"\r\n\r\n") {
        request.push(socket.read_u8().await.unwrap());
        assert!(request.len() < 8192);
    }
    socket.write_all(format!("HTTP/1.1 {status} Test\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
}
async fn activity_finished(mut request: Request) -> Result<activity::Snapshot, ActivityError> {
    timeout(TEST_DEADLINE, async {
        loop {
            match request.poll() {
                Progress::Pending(pending) => request = pending,
                Progress::Complete(result) => return result,
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap()
}
async fn connection_finished(connection: &Connection) {
    timeout(TEST_DEADLINE, async {
        while matches!(connection.state(), State::Connecting | State::Receiving) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
fn deadline(client: &Client, operation: Operation, url: &str, now: Instant) -> Instant {
    match client.check(operation, url, now) {
        Err(Blocked::Waiting(until)) => until,
        other => panic!("expected waiting: {other:?}"),
    }
}

#[test]
fn retry_after_parses_seconds_and_http_dates_and_rejects_invalid_values() {
    const HOUR: Duration = Duration::from_secs(3600);
    assert_eq!(retry_after(" 3600 ", UNIX_EPOCH), Some(HOUR));
    assert_eq!(
        retry_after("Thu, 01 Jan 1970 01:00:00 GMT", UNIX_EPOCH),
        Some(HOUR)
    );
    assert_eq!(
        retry_after("Thu, 01 Jan 1970 01:00:00 GMT", UNIX_EPOCH + HOUR),
        Some(Duration::ZERO)
    );
    for invalid in ["", "broken", "-1", "1.5"] {
        assert_eq!(retry_after(invalid, UNIX_EPOCH), None);
    }
}

#[test]
fn activity_429_limits_all_endpoints_even_when_its_completed_result_is_cancelled() -> TestResult {
    runtime()?.block_on(async {
        for (headers, minimum) in [
            ("Retry-After: 3600\r\n", Duration::from_secs(3600)),
            ("", Duration::from_secs(60)),
            ("Retry-After: invalid\r\n", Duration::from_secs(60)),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").await?;
            let target = endpoints(&listener);
            let client = Client::new()?;
            let now = Instant::now();
            let request = client.activity(&Handle::current(), target.threads.clone(), now)?;
            response(&listener, 429, headers, "").await;
            timeout(TEST_DEADLINE, async {
                while !request.is_finished() {
                    tokio::task::yield_now().await;
                }
            })
            .await?;
            let until = deadline(&client, Operation::Live, &target.threads, now);
            assert!(until >= now + minimum);
            assert!(until <= Instant::now() + minimum + MAX_JITTER);
            let mut cancelling = request.cancel();
            loop {
                match cancelling.poll() {
                    Progress::Pending(pending) => cancelling = pending,
                    Progress::Complete(()) => break,
                }
                tokio::task::yield_now().await;
            }
            assert!(matches!(
                client.activity(
                    &Handle::current(),
                    target.threads.clone(),
                    until - BEFORE_DEADLINE
                ),
                Err(Blocked::Waiting(_))
            ));
            assert!(matches!(
                client.connect(&Handle::current(), target.clone(), until - BEFORE_DEADLINE),
                Err(Blocked::Waiting(_))
            ));
            assert!(matches!(
                client.post(target.comments.clone(), until - BEFORE_DEADLINE),
                Err(Blocked::Waiting(_))
            ));
            let request = client.activity(&Handle::current(), target.threads.clone(), until)?;
            response(&listener, 200, "", "[]").await;
            activity_finished(request).await?;
        }
        Ok(())
    })
}

#[test]
fn thread_and_websocket_429_share_limits_with_activity_and_posting() -> TestResult {
    runtime()?.block_on(async {
        for websocket in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").await?;
            let target = endpoints(&listener);
            let client = Client::new()?;
            let now = Instant::now();
            let connection = client.connect(&Handle::current(), target.clone(), now)?;
            if websocket {
                response(&listener, 200, "", r#"[{"id":1,"status":"ACTIVE"}]"#).await;
            }
            response(&listener, 429, "Retry-After: 3600\r\n", "").await;
            connection_finished(&connection).await;
            assert!(matches!(connection.state(), State::Failed(_)));
            let until = deadline(&client, Operation::Live, &target.threads, now);
            assert!(until >= now + Duration::from_secs(3600));
            assert!(matches!(
                client.activity(
                    &Handle::current(),
                    "http://127.0.0.1:1/activity".into(),
                    until - BEFORE_DEADLINE
                ),
                Err(Blocked::Waiting(_))
            ));
            assert!(matches!(
                client.post(target.comments, until - BEFORE_DEADLINE),
                Err(Blocked::Waiting(_))
            ));
            connection.stop().wait().await?;
        }
        Ok(())
    })
}

#[test]
fn channel_changes_and_disable_cannot_reset_a_received_limit_before_gui_poll() -> TestResult {
    runtime()?.block_on(async {
        let first = TcpListener::bind("127.0.0.1:0").await?;
        let second = TcpListener::bind("127.0.0.1:0").await?;
        let client = Client::new()?;
        let mut controller = Controller::default();
        let now = Instant::now();
        controller.configure(Some(endpoints(&first)));
        controller.poll(&Handle::current(), &client, now)?;
        response(&first, 429, "", "").await;
        timeout(TEST_DEADLINE, async {
            while client
                .check(Operation::Live, &endpoints(&first).threads, now)
                .is_ok()
            {
                tokio::task::yield_now().await;
            }
        })
        .await?;
        let until = deadline(&client, Operation::Live, &endpoints(&second).threads, now);
        controller.configure(None);
        controller.configure(Some(endpoints(&second)));
        for _ in 0..10 {
            controller.poll(&Handle::current(), &client, until - BEFORE_DEADLINE)?;
            tokio::task::yield_now().await;
        }
        assert!(
            timeout(Duration::from_millis(20), second.accept())
                .await
                .is_err()
        );
        controller.poll(&Handle::current(), &client, until)?;
        response(&second, 403, "", "").await;
        controller.configure(None);
        Ok(())
    })
}

#[test]
fn short_sessions_keep_the_failure_streak_and_stable_sessions_reset_it() -> TestResult {
    const ENDPOINT: &str = "http://127.0.0.1:1/threads";
    const INITIAL_FAILURES: usize = 4;
    for uptime in [Duration::ZERO, STABLE_CONNECTION] {
        let client = Client::new()?;
        let mut now = Instant::now();
        for _ in 0..INITIAL_FAILURES {
            client.worker_failed(ENDPOINT, now);
            now = deadline(&client, Operation::Live, ENDPOINT, now);
        }
        let (_, attempt) = client
            .authorize(Operation::Live, ENDPOINT.into(), (), now)?
            .into_parts();
        drop(Reception {
            attempt: &attempt,
            since: Instant::now() - uptime,
        });
        attempt.failed(Some(Failure::Temporary), "closed".into());
        let until = deadline(&client, Operation::Live, ENDPOINT, now);
        if uptime == Duration::ZERO {
            assert!(until >= now + Duration::from_secs(80));
        } else {
            assert!(until >= now + LIVE_DELAY);
            assert!(until < now + LIVE_DELAY + MAX_JITTER + Duration::from_secs(1));
        }
    }
    Ok(())
}

#[test]
fn redirects_are_not_followed_and_permanent_errors_stop_repeated_requests() -> TestResult {
    runtime()?.block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let redirected = TcpListener::bind("127.0.0.1:0").await?;
        let target = endpoints(&listener);
        let client = Client::new()?;
        let now = Instant::now();
        let request = client.activity(&Handle::current(), target.threads.clone(), now)?;
        response(
            &listener,
            302,
            &format!("Location: {}\r\n", endpoints(&redirected).threads),
            "",
        )
        .await;
        assert!(activity_finished(request).await.is_err());
        assert!(matches!(
            client.activity(
                &Handle::current(),
                target.threads,
                now + Duration::from_secs(86400)
            ),
            Err(Blocked::Stopped(_))
        ));
        assert!(
            timeout(Duration::from_millis(20), redirected.accept())
                .await
                .is_err()
        );
        Ok(())
    })
}

#[test]
fn repeated_http_failures_without_retry_after_never_restart_at_five_seconds() -> TestResult {
    runtime()?.block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let target = endpoints(&listener);
        let client = Client::new()?;
        let mut now = Instant::now();
        for seconds in [5, 10, 20, 40, 80, 160, 300, 300] {
            let connection = client.connect(&Handle::current(), target.clone(), now)?;
            response(&listener, 503, "", "").await;
            connection_finished(&connection).await;
            let until = deadline(&client, Operation::Live, &target.threads, now);
            let minimum = Duration::from_secs(seconds);
            assert!(until >= now + minimum);
            assert!(until < now + minimum + MAX_JITTER + Duration::from_secs(1));
            assert!(matches!(
                client.connect(&Handle::current(), target.clone(), until - BEFORE_DEADLINE),
                Err(Blocked::Waiting(_))
            ));
            connection.stop().wait().await?;
            now = until;
        }
        Ok(())
    })
}

#[test]
fn activity_failures_back_off_and_a_success_resets_only_its_next_failure() -> TestResult {
    runtime()?.block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let url = endpoints(&listener).threads;
        let client = Client::new()?;
        let mut now = Instant::now();
        for seconds in [60, 120, 240, 300, 300] {
            let request = client.activity(&Handle::current(), url.clone(), now)?;
            response(&listener, 503, "", "").await;
            assert!(activity_finished(request).await.is_err());
            let until = deadline(&client, Operation::Activity, &url, now);
            let minimum = Duration::from_secs(seconds);
            assert!(until >= now + minimum);
            assert!(until < now + minimum + MAX_JITTER + Duration::from_secs(1));
            now = until;
        }
        let request = client.activity(&Handle::current(), url.clone(), now)?;
        response(&listener, 200, "", "[]").await;
        activity_finished(request).await?;
        now = deadline(&client, Operation::Activity, &url, now);
        let request = client.activity(&Handle::current(), url.clone(), now)?;
        response(&listener, 503, "", "").await;
        assert!(activity_finished(request).await.is_err());
        let until = deadline(&client, Operation::Activity, &url, now);
        assert!(
            until < now + crate::retry::ACTIVITY_INTERVAL + MAX_JITTER + Duration::from_secs(1)
        );
        Ok(())
    })
}

#[test]
fn watch_handshake_limits_background_requests_without_resending_a_post() -> TestResult {
    runtime()?.block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let target = endpoints(&listener);
        let client = Client::new()?;
        let now = Instant::now();
        let mut posting = crate::posting::Controller::default();
        posting.configure(Some(crate::posting::Target {
            service_id: 1,
            watch_url: target.comments.clone(),
        }));
        assert!(posting.submit(&Handle::current(), &client, "test", now));
        response(&listener, 429, "Retry-After: 3600\r\n", "").await;
        timeout(TEST_DEADLINE, async {
            while posting.busy() {
                assert!(!posting.poll(now));
                tokio::task::yield_now().await;
            }
        })
        .await?;
        assert!(matches!(
            posting.status(),
            crate::posting::Status::Failed(_)
        ));
        let until = deadline(&client, Operation::Posting, &target.comments, now);
        assert!(until >= now + Duration::from_secs(3600));
        assert!(matches!(
            client.activity(
                &Handle::current(),
                target.threads.clone(),
                until - BEFORE_DEADLINE
            ),
            Err(Blocked::Waiting(_))
        ));
        assert!(matches!(
            client.connect(&Handle::current(), target, until - BEFORE_DEADLINE),
            Err(Blocked::Waiting(_))
        ));
        posting.poll(until);
        assert!(
            timeout(Duration::from_millis(20), listener.accept())
                .await
                .is_err()
        );
        Ok(())
    })
}

#[test]
fn authorization_is_rechecked_after_another_endpoint_returns_a_limit() -> TestResult {
    runtime()?.block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let target = endpoints(&listener);
        let client = Client::new()?;
        let now = Instant::now();
        // Admit the live task but do not let the current-thread runtime send it.
        let connection = client.connect(&Handle::current(), target.clone(), now)?;
        client.policy.lock().unwrap().failed(
            Operation::Activity,
            "another endpoint",
            Failure::Http {
                status: reqwest::StatusCode::TOO_MANY_REQUESTS,
                retry_after: None,
            },
            "limited".into(),
            now,
        );
        connection_finished(&connection).await;
        assert!(matches!(connection.state(), State::Failed(_)));
        assert!(
            timeout(Duration::from_millis(20), listener.accept())
                .await
                .is_err()
        );
        connection.stop().wait().await?;
        Ok(())
    })
}

#[test]
fn websocket_close_codes_preserve_reasons_and_select_shared_wait_or_stop() -> TestResult {
    use crate::termination::Termination;
    use futures_util::StreamExt;
    use tokio_tungstenite::tungstenite::protocol::{CloseFrame, frame::coding::CloseCode};
    const UNKNOWN_CLOSE_CODE: u16 = 4001;
    const REASON: &str = "server supplied reason";
    enum Expected {
        Reconnect,
        Stop,
        Shared(Duration),
    }
    runtime()?.block_on(async {
        for (code, expected) in [
            (CloseCode::Normal, Expected::Reconnect),
            (CloseCode::Protocol, Expected::Reconnect),
            (CloseCode::from(UNKNOWN_CLOSE_CODE), Expected::Reconnect),
            (CloseCode::Policy, Expected::Stop),
            (CloseCode::Unsupported, Expected::Stop),
            (CloseCode::Invalid, Expected::Stop),
            (CloseCode::Error, Expected::Shared(LIVE_DELAY)),
            (CloseCode::Restart, Expected::Shared(LIVE_DELAY)),
            (CloseCode::Again, Expected::Shared(Duration::from_secs(60))),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").await?;
            let target = endpoints(&listener);
            let client = Client::new()?;
            let now = Instant::now();
            let connection = client.connect(&Handle::current(), target.clone(), now)?;
            response(&listener, 200, "", r#"[{"id":1,"status":"ACTIVE"}]"#).await;
            let (tcp, _) = timeout(TEST_DEADLINE, listener.accept()).await??;
            let mut socket = tokio_tungstenite::accept_async(tcp).await?;
            assert!(socket.next().await.ok_or("subscription missing")??.is_text());
            socket.close(Some(CloseFrame { code, reason: REASON.into() })).await?;
            // Losing the peer while acknowledging its close must not replace
            // a received policy rejection with a generic transport error.
            drop(socket);
            connection_finished(&connection).await;
            let reason = match connection.state() {
                State::Ended(reason) => {
                    assert_eq!(code, CloseCode::Normal);
                    reason
                }
                State::Failed(error) => match &*error {
                    crate::connection::Error::Terminated(reason) => reason.clone(),
                    other => panic!("lost server close: {other:?}"),
                },
                other => panic!("missing terminal state: {other:?}"),
            };
            assert_eq!(reason, Termination::Close { code, reason: REASON.into() });
            connection.stop().wait().await?;
            match expected {
                Expected::Stop => {
                    assert!(matches!(client.check(Operation::Live, &target.threads, now + Duration::from_secs(86400)), Err(Blocked::Stopped(message)) if message.contains(REASON)));
                    assert!(client.check(Operation::Live, "another channel", now).is_ok());
                }
                Expected::Reconnect => {
                    let until = deadline(&client, Operation::Live, &target.threads, now);
                    assert!(until >= now + LIVE_DELAY);
                    assert!(client.check(Operation::Activity, "activity", now).is_ok());
                }
                Expected::Shared(minimum) => {
                    let until = deadline(&client, Operation::Live, &target.threads, now);
                    assert!(until >= now + minimum);
                    assert_eq!(deadline(&client, Operation::Activity, "activity", now), until);
                    assert_eq!(deadline(&client, Operation::Posting, "watch", now), until);
                }
            }
        }
        Ok(())
    })
}
