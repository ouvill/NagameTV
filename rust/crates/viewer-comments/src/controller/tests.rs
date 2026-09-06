use super::*;
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
            let client = reqwest::Client::new();
            let now = Instant::now();
            let mut controller = Controller::default();
            controller.configure(Some(endpoint(&a)?));
            assert!(controller.poll(&Handle::current(), &client, now).is_empty());
            let (mut old, _) = timeout(Duration::from_secs(2), a.accept()).await??;
            let mut request = [0; 4096];
            assert!(timeout(Duration::from_secs(2), old.read(&mut request)).await?? > 0);
            controller.configure(Some(endpoint(&b)?));
            controller.configure(Some(endpoint(&c)?));
            assert!(matches!(controller.phase, Phase::Stopping { .. }));
            // On this current-thread runtime the aborted task cannot finish
            // until we yield. Poll must not start C during that interval.
            controller.poll(&Handle::current(), &client, now);
            assert!(matches!(controller.phase, Phase::Stopping { .. }));
            timeout(Duration::from_secs(2), async {
                loop {
                    controller.poll(&Handle::current(), &client, now);
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
                    assert!(controller.poll(&Handle::current(), &client, now).is_empty());
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
            let client = reqwest::Client::new();
            let now = Instant::now();
            let mut controller = Controller::default();
            controller.configure(Some(target.clone()));
            controller.poll(&Handle::current(), &client, now);
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
                    controller.poll(&Handle::current(), &client, now);
                    tokio::task::yield_now().await;
                }
            })
            .await?;
            assert!(matches!(
                controller.status(),
                Status::Retrying(State::Failed(_))
            ));
            controller.configure(Some(target));
            controller.poll(
                &Handle::current(),
                &client,
                now + RETRY_DELAY - Duration::from_millis(1),
            );
            assert!(matches!(controller.phase, Phase::Waiting(_)));
            controller.poll(&Handle::current(), &client, now + RETRY_DELAY);
            let (_retry, _) = timeout(Duration::from_secs(2), listener.accept()).await??;
            assert!(matches!(controller.phase, Phase::Running(_)));
            controller.configure(None);
            Ok(())
        })
}

#[test]
fn normal_close_drains_final_comments_in_bounded_batches() -> TestResult {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let listener = TcpListener::bind("127.0.0.1:0").await?;
            let client = reqwest::Client::new();
            let now = Instant::now();
            let mut controller = Controller::default();
            controller.configure(Some(endpoint(&listener)?));
            controller.poll(&Handle::current(), &client, now);
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
            socket.close(None).await?;
            timeout(Duration::from_secs(2), async {
                loop {
                    if let Phase::Running(connection) = &controller.phase
                        && matches!(connection.state(), State::Ended)
                    {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await?;
            let mut all = Vec::new();
            for size in [64, 64, 2] {
                let batch = controller.poll(&Handle::current(), &client, now);
                assert_eq!(batch.len(), size);
                all.extend(batch);
            }
            assert_eq!(&*all[0].text, "0");
            assert_eq!(&*all[129].text, "129");
            assert!(matches!(
                controller.status(),
                Status::Retrying(State::Ended)
            ));
            controller.configure(None);
            timeout(Duration::from_secs(2), async {
                while !controller.is_stopped() {
                    controller.poll(&Handle::current(), &client, now);
                    tokio::task::yield_now().await;
                }
            })
            .await?;
            Ok(())
        })
}
