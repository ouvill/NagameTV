use super::*;
use std::time::Duration;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    time::timeout,
};
type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;
fn endpoint(listener: &TcpListener) -> std::io::Result<Endpoints> {
    let address = listener.local_addr()?;
    Ok(Endpoints {
        threads: format!("http://{address}/threads"),
        comments: format!("ws://{address}/comments"),
    })
}

#[test]
fn rapid_changes_keep_only_last_target_and_disable_cancels_it() -> TestResult {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let a = TcpListener::bind("127.0.0.1:0").await?;
            let b = TcpListener::bind("127.0.0.1:0").await?;
            let c = TcpListener::bind("127.0.0.1:0").await?;
            let client = Client::new()?;
            let now = Instant::now();
            let mut controller = Controller::default();
            controller.configure(Some(endpoint(&a)?));
            assert!(
                controller
                    .poll(&Handle::current(), &client, now)
                    .is_ok_and(|batch| batch.is_empty())
            );
            let (mut old, _) = timeout(Duration::from_secs(2), a.accept()).await??;
            let mut request = [0; 4096];
            assert!(timeout(Duration::from_secs(2), old.read(&mut request)).await?? > 0);
            controller.configure(Some(endpoint(&b)?));
            controller.configure(Some(endpoint(&c)?));
            assert!(matches!(controller.phase, Phase::Stopping { .. }));
            // On this current-thread runtime the aborted task cannot finish
            // until we yield. Poll must not start C during that interval.
            assert!(controller.poll(&Handle::current(), &client, now).is_ok());
            assert!(matches!(controller.phase, Phase::Stopping { .. }));
            timeout(Duration::from_secs(2), async {
                loop {
                    assert!(controller.poll(&Handle::current(), &client, now).is_ok());
                    if matches!(controller.phase, Phase::Running(_)) {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await?;
            // A must be closed before C is connected. B must never be started.
            assert_eq!(
                timeout(Duration::from_secs(2), old.read(&mut request)).await??,
                0
            );
            let (_latest, _) = timeout(Duration::from_secs(2), c.accept()).await??;
            assert!(
                timeout(Duration::from_millis(20), b.accept())
                    .await
                    .is_err()
            );
            controller.configure(None);
            timeout(Duration::from_secs(2), async {
                while !matches!(controller.phase, Phase::Idle) {
                    assert!(
                        controller
                            .poll(&Handle::current(), &client, now)
                            .is_ok_and(|batch| batch.is_empty())
                    );
                    tokio::task::yield_now().await;
                }
            })
            .await?;
            assert!(matches!(controller.status(), Status::Disabled));
            Ok(())
        })
}

#[test]
fn reconnect_waits_for_deadline_and_repeated_configuration_does_not_reset_it() -> TestResult {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let listener = TcpListener::bind("127.0.0.1:0").await?;
            let target = endpoint(&listener)?;
            let client = Client::new()?;
            let now = Instant::now();
            let mut controller = Controller::default();
            controller.configure(Some(target.clone()));
            assert!(controller.poll(&Handle::current(), &client, now).is_ok());
            let (mut http, _) = timeout(Duration::from_secs(2), listener.accept()).await??;
            let mut buffer = [0; 4096];
            assert!(http.read(&mut buffer).await? > 0);
            http.write_all(
                b"HTTP/1.1 503 Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .await?;
            drop(http);
            timeout(Duration::from_secs(2), async {
                while !matches!(controller.phase, Phase::Waiting(_)) {
                    assert!(controller.poll(&Handle::current(), &client, now).is_ok());
                    tokio::task::yield_now().await;
                }
            })
            .await?;
            assert!(matches!(
                controller.status(),
                Status::Retrying(State::Failed(_))
            ));
            controller.configure(Some(target));
            let Phase::Waiting(deadline) = controller.phase else {
                panic!("retry deadline")
            };
            controller.poll(
                &Handle::current(),
                &client,
                deadline - Duration::from_millis(1),
            )?;
            assert!(matches!(controller.phase, Phase::Waiting(_)));
            controller.poll(&Handle::current(), &client, deadline)?;
            let (_retry, _) = timeout(Duration::from_secs(2), listener.accept()).await??;
            assert!(matches!(controller.phase, Phase::Running(_)));
            controller.configure(None);
            Ok(())
        })
}

#[test]
fn normal_close_drains_final_comments_in_bounded_batches() -> TestResult {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::{
        Message,
        protocol::{CloseFrame, frame::coding::CloseCode},
    };
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let listener = TcpListener::bind("127.0.0.1:0").await?;
            let client = Client::new()?;
            let now = Instant::now();
            let mut controller = Controller::default();
            controller.configure(Some(endpoint(&listener)?));
            assert!(controller.poll(&Handle::current(), &client, now).is_ok());
            let (mut http, _) = timeout(Duration::from_secs(2), listener.accept()).await??;
            let mut buffer = [0; 4096];
            assert!(http.read(&mut buffer).await? > 0);
            let body = r#"[{"id":1,"status":"ACTIVE"}]"#;
            http.write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            )
            .await?;
            drop(http);
            let (tcp, _) = timeout(Duration::from_secs(2), listener.accept()).await??;
            let mut socket = tokio_tungstenite::accept_async(tcp).await?;
            socket.next().await.ok_or("missing subscription")??;
            for index in 0..130 {
                socket
                    .send(Message::Text(
                        format!(r#"{{"chat":{{"content":"{index}"}}}}"#).into(),
                    ))
                    .await?;
            }
            socket
                .close(Some(CloseFrame {
                    code: CloseCode::Normal,
                    reason: "thread ended".into(),
                }))
                .await?;
            timeout(Duration::from_secs(2), async {
                loop {
                    if let Phase::Running(connection) = &controller.phase
                        && matches!(connection.state(), State::Ended(_))
                    {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await?;
            let mut all = Vec::new();
            for size in [64, 64, 2] {
                let batch = controller.poll(&Handle::current(), &client, now)?;
                assert_eq!(batch.len(), size);
                all.extend(batch);
            }
            assert_eq!(&*all[0].text, "0");
            assert_eq!(&*all[129].text, "129");
            assert!(matches!(
                controller.status(),
                Status::Retrying(State::Ended(_))
            ));
            controller.configure(None);
            timeout(Duration::from_secs(2), async {
                while !controller.is_stopped() {
                    assert!(controller.poll(&Handle::current(), &client, now).is_ok());
                    tokio::task::yield_now().await;
                }
            })
            .await?;
            Ok(())
        })
}

#[test]
fn thread_end_reconnects_to_the_current_thread_and_repeated_short_closes_back_off() -> TestResult {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::{
        WebSocketStream,
        tungstenite::{
            Message,
            protocol::{CloseFrame, frame::coding::CloseCode},
        },
    };
    const TEST_TIMEOUT: Duration = Duration::from_secs(3);
    const FIRST_THREAD: u64 = 11;
    const NEXT_THREAD: u64 = 12;
    async fn subscribe(
        listener: &TcpListener,
        thread: u64,
    ) -> Result<WebSocketStream<tokio::net::TcpStream>, Box<dyn std::error::Error + Send + Sync>>
    {
        let (mut http, _) = listener.accept().await?;
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            request.push(http.read_u8().await?);
        }
        let body = format!(r#"[{{"id":{thread},"status":"ACTIVE"}}]"#);
        http.write_all(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .as_bytes(),
        )
        .await?;
        let (tcp, _) = listener.accept().await?;
        let mut socket = tokio_tungstenite::accept_async(tcp).await?;
        let subscription = socket.next().await.ok_or("missing subscription")??;
        let value: serde_json::Value = serde_json::from_str(subscription.to_text()?)?;
        assert_eq!(value[2]["thread"]["thread"], thread.to_string());
        Ok(socket)
    }
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            timeout(TEST_TIMEOUT, async {
                let listener = TcpListener::bind("127.0.0.1:0").await?;
                let client = Client::new()?;
                let mut controller = Controller::default();
                let mut now = Instant::now();
                controller.configure(Some(endpoint(&listener)?));
                for (thread, expected_minimum) in [
                    (FIRST_THREAD, Duration::from_secs(5)),
                    (NEXT_THREAD, Duration::from_secs(10)),
                ] {
                    controller.poll(&Handle::current(), &client, now)?;
                    let mut socket = subscribe(&listener, thread).await?;
                    socket
                        .send(Message::Text(
                            format!(r#"{{"chat":{{"content":"thread-{thread}"}}}}"#).into(),
                        ))
                        .await?;
                    socket
                        .close(Some(CloseFrame {
                            code: CloseCode::Normal,
                            reason: "thread ended".into(),
                        }))
                        .await?;
                    let mut delivered = Vec::new();
                    let until = loop {
                        delivered.extend(controller.poll(&Handle::current(), &client, now)?);
                        if let Phase::Waiting(until) = controller.phase {
                            break until;
                        }
                        tokio::task::yield_now().await;
                    };
                    assert_eq!(delivered.len(), 1);
                    assert_eq!(delivered[0].text.as_ref(), format!("thread-{thread}"));
                    assert_eq!(delivered[0].phase, crate::Phase::History);
                    assert!(until >= now + expected_minimum);
                    assert!(matches!(
                        controller.status(),
                        Status::Retrying(State::Ended(_))
                    ));
                    controller.poll(
                        &Handle::current(),
                        &client,
                        until - Duration::from_millis(1),
                    )?;
                    assert!(matches!(controller.phase, Phase::Waiting(_)));
                    now = until;
                }
                controller.configure(None);
                Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
            })
            .await?
        })
}

#[test]
fn websocket_disconnect_drains_final_comments_and_reconnects_with_fresh_history() -> TestResult {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::{WebSocketStream, tungstenite::Message};

    async fn accept_subscription(
        listener: &TcpListener,
    ) -> Result<WebSocketStream<tokio::net::TcpStream>, Box<dyn std::error::Error + Send + Sync>>
    {
        let (mut http, _) = listener.accept().await?;
        let mut request = [0; 4096];
        assert!(http.read(&mut request).await? > 0);
        let body = r#"[{"id":1,"status":"ACTIVE"}]"#;
        http.write_all(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .as_bytes(),
        )
        .await?;
        drop(http);
        let (tcp, _) = listener.accept().await?;
        let mut socket = tokio_tungstenite::accept_async(tcp).await?;
        let subscription = socket.next().await.ok_or("missing subscription")??;
        assert!(subscription.is_text());
        Ok(socket)
    }

    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let listener = TcpListener::bind("127.0.0.1:0").await?;
            let client = Client::new()?;
            let now = Instant::now();
            let mut controller = Controller::default();
            controller.configure(Some(endpoint(&listener)?));
            controller.poll(&Handle::current(), &client, now)?;
            let mut socket =
                timeout(Duration::from_secs(2), accept_subscription(&listener)).await??;
            socket
                .send(Message::Text(
                    r#"{"chat":{"content":"old history","date":0}}"#.into(),
                ))
                .await?;
            socket
                .send(Message::Text(r#"{"ping":{"content":"rf:0"}}"#.into()))
                .await?;
            // More than one GUI poll's capacity, below the connection queue limit.
            for index in 0..70 {
                socket
                    .send(Message::Text(
                        format!(r#"{{"chat":{{"content":"old live {index}","date":0}}}}"#).into(),
                    ))
                    .await?;
            }
            drop(socket); // Abrupt transport loss, without a WebSocket close handshake.
            // Wait for the reader to finish without draining through the controller.
            // This makes the terminal queue larger than a single poll deterministically.
            timeout(Duration::from_secs(2), async {
                loop {
                    if let Phase::Running(connection) = &controller.phase
                        && matches!(connection.state(), State::Failed(_))
                    {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await?;
            let mut received = controller.poll(&Handle::current(), &client, now)?;
            assert_eq!(received.len(), MAX_POLL_COMMENTS);
            assert!(matches!(controller.phase, Phase::Running(_)));
            timeout(Duration::from_secs(2), async {
                loop {
                    let batch = controller.poll(&Handle::current(), &client, now)?;
                    assert!(batch.len() <= MAX_POLL_COMMENTS);
                    received.extend(batch);
                    if matches!(controller.phase, Phase::Waiting(_)) {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
                Ok::<_, Error>(())
            })
            .await??;
            assert_eq!(received.len(), 71);
            assert_eq!(received[0].phase, crate::Phase::History);
            for (index, comment) in received[1..].iter().enumerate() {
                assert_eq!(comment.text.as_ref(), format!("old live {index}"));
                assert_eq!(comment.phase, crate::Phase::Live);
            }
            assert!(matches!(
                controller.status(),
                Status::Retrying(State::Failed(_))
            ));
            // Injected monotonic time: verify the deadline including jitter.
            let Phase::Waiting(retry) = controller.phase else {
                panic!("retry deadline")
            };
            controller.poll(
                &Handle::current(),
                &client,
                retry - Duration::from_millis(1),
            )?;
            assert!(matches!(controller.phase, Phase::Waiting(_)));
            controller.poll(&Handle::current(), &client, retry)?;
            let mut socket =
                timeout(Duration::from_secs(2), accept_subscription(&listener)).await??;
            for packet in [
                r#"{"chat":{"content":"new history","date":0}}"#,
                r#"{"ping":{"content":"rf:0"}}"#,
                r#"{"chat":{"content":"new live","date":0}}"#,
            ] {
                socket.send(Message::Text(packet.into())).await?;
            }
            received.clear();
            timeout(Duration::from_secs(2), async {
                while received.len() < 2 {
                    received.extend(controller.poll(&Handle::current(), &client, retry)?);
                    tokio::task::yield_now().await;
                }
                Ok::<_, Error>(())
            })
            .await??;
            assert_eq!(received.len(), 2);
            assert_eq!(received[0].text.as_ref(), "new history");
            assert_eq!(received[0].phase, crate::Phase::History);
            assert_eq!(received[1].text.as_ref(), "new live");
            assert_eq!(received[1].phase, crate::Phase::Live);
            assert!(matches!(
                controller.status(),
                Status::Connection(State::Receiving)
            ));
            let epoch_before_loss = controller.reception_epoch().unwrap();
            for _ in 0..=crate::connection::QUEUE_CAPACITY {
                socket
                    .send(Message::Text(
                        r#"{"chat":{"content":"burst","date":1}}"#.into(),
                    ))
                    .await?;
            }
            timeout(Duration::from_secs(2), async {
                loop {
                    if let Phase::Running(connection) = &controller.phase
                        && connection.dropped() > 0
                    {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await?;
            assert_eq!(controller.reception_epoch(), None);
            controller.poll(&Handle::current(), &client, retry)?;
            assert_ne!(controller.reception_epoch().unwrap(), epoch_before_loss);
            controller.configure(None);
            timeout(Duration::from_secs(2), async {
                while !controller.is_stopped() {
                    assert!(
                        controller
                            .poll(&Handle::current(), &client, retry)?
                            .is_empty()
                    );
                    tokio::task::yield_now().await;
                }
                Ok::<_, Error>(())
            })
            .await??;
            Ok(())
        })
}
