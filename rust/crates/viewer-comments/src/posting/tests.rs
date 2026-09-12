use super::*;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::{
    net::{TcpListener, TcpStream},
    time::timeout,
};
use tokio_tungstenite::{WebSocketStream, tungstenite::Message};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
fn runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}
fn target(listener: &TcpListener, service_id: u64) -> TestResult<Target> {
    Ok(Target {
        service_id,
        watch_url: format!("ws://{}/watch", listener.local_addr()?),
    })
}
async fn accept(listener: &TcpListener) -> TestResult<WebSocketStream<TcpStream>> {
    let (tcp, _) = listener.accept().await?;
    let mut socket = tokio_tungstenite::accept_async(tcp).await?;
    assert_eq!(next(&mut socket).await?["type"], "startWatching");
    Ok(socket)
}
async fn next(socket: &mut WebSocketStream<TcpStream>) -> TestResult<Value> {
    let message = socket.next().await.ok_or("missing client message")??;
    Ok(serde_json::from_str(message.to_text()?)?)
}
async fn send(socket: &mut WebSocketStream<TcpStream>, value: Value) -> TestResult {
    socket.send(Message::Text(value.to_string().into())).await?;
    Ok(())
}
async fn metadata(socket: &mut WebSocketStream<TcpStream>) -> TestResult {
    // NX sends session metadata as well as the clock/room required for posting.
    send(socket, json!({"type":"seat","data":{"keepIntervalSec":30}})).await?;
    send(socket, json!({"type":"schedule","data":{"begin":"2026-09-13T04:00:00+09:00","end":"2026-09-14T04:00:00+09:00"}})).await?;
    // Deliberately reversed arrival order. Client must await both fields.
    send(
        socket,
        json!({"type":"room","data":{"vposBaseTime":"2026-09-13T04:00:00+09:00"}}),
    )
    .await?;
    send(
        socket,
        json!({"type":"serverTime","data":{"currentMs":"2026-09-13T04:01:02.340000+09:00"}}),
    )
    .await?;
    send(socket, json!({"type":"statistics","data":{"viewers":10,"comments":100,"adPoints":0,"giftPoints":0}})).await
}
async fn finished(controller: &mut Controller) -> bool {
    loop {
        let sent = controller.poll(Instant::now());
        if !controller.busy() {
            return sent;
        }
        tokio::task::yield_now().await;
    }
}

#[test]
fn live_echo_is_recognized_before_ack_and_success_notice_expires() -> TestResult {
    runtime()?.block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let mut controller = Controller::default();
        controller.configure(Some(target(&listener, 1)?));
        assert!(controller.submit(&Handle::current(), "same text", Instant::now()));
        let mut socket = accept(&listener).await?;
        send(
            &mut socket,
            json!({"type":"room","data":{
                "vposBaseTime":"2026-09-13T04:00:00+09:00",
                "yourPostKey":"local-client", "threadId":"123"
            }}),
        )
        .await?;
        send(
            &mut socket,
            json!({"type":"serverTime","data":{
                "currentMs":"2026-09-13T04:01:02.340000+09:00"
            }}),
        )
        .await?;
        let post = next(&mut socket).await?;
        let mut decoder = crate::Decoder::default();
        decoder.decode(br#"{"ping":{"content":"rf:0"}}"#)?;
        let payload = json!({"chat":{
            "thread":"123", "vpos":post["data"]["vpos"],
            "user_id":"local-client", "content":"same text"
        }});
        let crate::Event::Comment(mut comment) = decoder.decode(&serde_json::to_vec(&payload)?)?
        else {
            panic!("missing comment");
        };
        // Same text is insufficient, even for another post by the same client.
        comment.identity.as_mut().unwrap().vpos += 1;
        assert!(!controller.is_own_comment(&comment, Instant::now()));
        comment.identity.as_mut().unwrap().vpos -= 1;
        comment.identity.as_mut().unwrap().user_id = "other-client".into();
        assert!(!controller.is_own_comment(&comment, Instant::now()));
        comment.identity.as_mut().unwrap().user_id = "local-client".into();
        comment.phase = crate::Phase::History;
        assert!(!controller.is_own_comment(&comment, Instant::now()));
        comment.phase = crate::Phase::Live;
        assert!(matches!(controller.status(), Status::Sending));
        assert!(controller.is_own_comment(&comment, Instant::now()));
        assert!(!controller.is_own_comment(&comment, Instant::now()));
        send(
            &mut socket,
            json!({"type":"postCommentResult","data":{
                "chat":{"content":"same text", "restricted":false}
            }}),
        )
        .await?;
        assert!(timeout(Duration::from_secs(1), finished(&mut controller)).await?);
        let acknowledged = Instant::now();
        assert!(!controller.poll(acknowledged + Duration::from_secs(2)));
        assert!(matches!(controller.status(), Status::Sent));
        assert!(!controller.poll(acknowledged + Duration::from_secs(4)));
        assert!(matches!(controller.status(), Status::Idle));
        Ok(())
    })
}

#[test]
fn echo_matching_expires_and_cannot_cross_channel_changes() {
    use crate::{Comment, CommentIdentity, Origin, Phase, Style};
    let now = Instant::now();
    let mut controller = Controller::default();
    controller.configure(Some(Target {
        service_id: 1,
        watch_url: "ws://localhost/watch".into(),
    }));
    let mut comment = Comment {
        identity: Some(CommentIdentity {
            thread_id: 1,
            user_id: "self".into(),
            vpos: 100,
        }),
        text: "my comment".into(),
        origin: Origin::Nx,
        phase: Phase::Live,
        unix_seconds: 0,
        style: Style::default(),
    };
    let tx = controller.echoes.start(now);
    tx.send(echo::Echo {
        identity: comment.identity.clone().unwrap(),
        text: comment.text.clone(),
        created: now,
    })
    .unwrap();
    comment.identity.as_mut().unwrap().thread_id = 2;
    assert!(!controller.is_own_comment(&comment, now));
    comment.identity.as_mut().unwrap().thread_id = 1;
    assert!(!controller.is_own_comment(&comment, now + Duration::from_secs(61)));
    let tx = controller.echoes.start(now);
    tx.send(echo::Echo {
        identity: comment.identity.clone().unwrap(),
        text: comment.text.clone(),
        created: now,
    })
    .unwrap();
    controller.configure(Some(Target {
        service_id: 2,
        watch_url: "ws://localhost/watch".into(),
    }));
    assert!(!controller.is_own_comment(&comment, now));
    // A previous generation's publisher has no route into the new target.
    assert!(
        tx.send(echo::Echo {
            identity: comment.identity.unwrap(),
            text: comment.text,
            created: now
        })
        .is_err()
    );
}

#[test]
fn text_validation_and_server_clock_preserve_unicode_and_timezones() -> TestResult {
    assert!(matches!(validate_text(" \n\t　"), Err(Error::Empty)));
    validate_text(&"🦀".repeat(1024))?;
    assert!(matches!(
        validate_text(&"🦀".repeat(1025)),
        Err(Error::TooLong)
    ));
    let body: Value = serde_json::from_str(&protocol::request(
        "<b>日本語</b>\n\"🦀\"",
        protocol::timestamp("2026-09-13T04:01:02.349+09:00")?,
        protocol::timestamp("2026-09-12T19:00:00Z")?,
    )?)?;
    assert_eq!(body["data"]["vpos"], 6234);
    assert_eq!(body["data"]["text"], "<b>日本語</b>\n\"🦀\"");
    assert!(protocol::timestamp("not a timestamp").is_err());
    assert!(protocol::request("x", 1, 2).is_err());
    Ok(())
}

#[test]
fn session_metadata_and_unknown_event_payloads_are_ignored() -> TestResult {
    use protocol::Event;
    for message in [
        r#"{"type":"seat","data":{"keepIntervalSec":30}}"#,
        r#"{"type":"schedule","data":{"begin":"2026-09-13T04:00:00+09:00","end":"2026-09-14T04:00:00+09:00"}}"#,
        r#"{"type":"statistics","data":{"viewers":10,"comments":100}}"#,
        r#"{"data":{"nested":[1,{"field":true}]},"type":"futureEvent"}"#,
        r#"{"type":"futureEvent","data":[1,2]}"#,
        r#"{"type":"futureEvent","data":"text"}"#,
        r#"{"type":"futureEvent","data":null}"#,
        r#"{"type":"futureEvent"}"#,
    ] {
        assert!(matches!(
            serde_json::from_str::<Event>(message)?,
            Event::Ignore
        ));
    }
    for message in [r#"{"type":"ping"}"#, r#"{"type":"ping","data":{}}"#] {
        assert!(matches!(
            serde_json::from_str::<Event>(message)?,
            Event::Ping
        ));
    }
    Ok(())
}

#[test]
fn malformed_known_events_and_invalid_json_are_still_errors() {
    for message in [
        r#"{"type":"serverTime","data":{"currentMs":123}}"#,
        r#"{"type":"room","data":{}}"#,
        r#"{"type":"postCommentResult","data":{"chat":{}}}"#,
        r#"{"type":"error","data":{"message":false}}"#,
        r#"{"type":"disconnect","data":{}}"#,
        r#"{"type":"room"}"#,
        r#"{"type":123,"data":{}}"#,
        r#"{"data":{}}"#,
        r#"{"type":"futureEvent","data":{"broken":}}"#,
    ] {
        assert!(
            serde_json::from_str::<protocol::Event>(message).is_err(),
            "{message}"
        );
    }
}

#[test]
fn posts_with_nx_startup_notifications_in_service_order() -> TestResult {
    runtime()?.block_on(async {
        timeout(Duration::from_secs(4), async {
            let listener = TcpListener::bind("127.0.0.1:0").await?;
            let mut controller = Controller::default();
            controller.configure(Some(target(&listener, 1)?));
            assert!(controller.submit(&Handle::current(), "local startup test", Instant::now()));
            let mut socket = accept(&listener).await?;
            for message in [
                json!({"type":"serverTime","data":{"currentMs":"2026-09-13T04:01:02.340000+09:00"}}),
                json!({"type":"seat","data":{"keepIntervalSec":30}}),
                json!({"type":"schedule","data":{"begin":"2026-09-13T04:00:00+09:00","end":"2026-09-14T04:00:00+09:00"}}),
                json!({"type":"room","data":{"vposBaseTime":"2026-09-13T04:00:00+09:00","threadId":"123","isFirst":true,"name":"アリーナ","messageServer":{"uri":"ws://127.0.0.1/unused","type":"niwavided"}}}),
                json!({"type":"statistics","data":{"viewers":10,"comments":100,"adPoints":0,"giftPoints":0}}),
            ] {
                send(&mut socket, message).await?;
            }
            let body = next(&mut socket).await?;
            assert_eq!(body["type"], "postComment");
            assert_eq!(body["data"]["text"], "local startup test");
            // Unused notifications must also be harmless while awaiting the receipt.
            send(&mut socket, json!({"type":"futureEvent","data":{"value":[1,2]}})).await?;
            send(&mut socket, json!({"type":"postCommentResult","data":{"chat":{"content":"local startup test","restricted":false}}})).await?;
            assert!(finished(&mut controller).await);
            assert!(matches!(controller.status(), Status::Sent));
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        }).await
    })??;
    Ok(())
}

#[test]
fn posts_once_handles_both_heartbeats_and_requires_matching_acknowledgement() -> TestResult {
    runtime()?.block_on(async { timeout(Duration::from_secs(4), async {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let mut controller = Controller::default();
        controller.configure(Some(target(&listener, 1)?));
        assert!(!controller.submit(&Handle::current(), "　", Instant::now()));
        assert!(controller.submit(&Handle::current(), "日本語\n🦀", Instant::now()));
        assert!(!controller.submit(&Handle::current(), "duplicate", Instant::now()));
        let mut socket = accept(&listener).await?;
        socket.send(Message::Ping(b"transport".as_slice().into())).await?;
        assert!(matches!(socket.next().await.ok_or("missing pong")??, Message::Pong(_)));
        send(&mut socket, json!({"type":"ping"})).await?;
        assert_eq!(next(&mut socket).await?["type"], "pong");
        assert_eq!(next(&mut socket).await?["type"], "keepSeat");
        metadata(&mut socket).await?;
        let body = next(&mut socket).await?;
        assert_eq!(body["type"], "postComment");
        assert_eq!(body["data"]["text"], "日本語\n🦀");
        assert_eq!(body["data"]["isAnonymous"], true);
        assert_eq!(body["data"]["position"], "naka");
        assert_eq!(body["data"]["color"], "white");
        assert!(body["data"]["vpos"].as_u64().is_some_and(|n| (6234..6334).contains(&n)));
        send(&mut socket, json!({"type":"postCommentResult","data":{"chat":{"content":"日本語\n🦀","restricted":false}}})).await?;
        assert!(finished(&mut controller).await);
        assert!(matches!(controller.status(), Status::Sent));
        assert!(!controller.poll(Instant::now()), "success must be consumed once");
        let now = Instant::now();
        assert!(!controller.available(now));
        assert!(controller.available(now + POST_INTERVAL));
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    }).await })??;
    Ok(())
}

#[test]
fn failures_before_and_after_write_are_distinct_and_never_retried() -> TestResult {
    runtime()?.block_on(async {
        timeout(Duration::from_secs(4), async {
            for mode in [
                "before",
                "after",
                "reject",
                "wrong_ack",
                "restricted",
                "bad_clock",
                "oversize",
            ] {
                let listener = TcpListener::bind("127.0.0.1:0").await?;
                let mut controller = Controller::default();
                controller.configure(Some(target(&listener, 1)?));
                assert!(controller.submit(&Handle::current(), "local test", Instant::now()));
                let mut socket = accept(&listener).await?;
                if mode == "bad_clock" {
                    send(
                        &mut socket,
                        json!({"type":"serverTime","data":{"currentMs":"invalid"}}),
                    )
                    .await?;
                } else if mode == "oversize" {
                    socket
                        .send(Message::Text(
                            "x".repeat(crate::MAX_MESSAGE_BYTES + 1).into(),
                        ))
                        .await?;
                } else if mode != "before" {
                    metadata(&mut socket).await?;
                    assert_eq!(next(&mut socket).await?["type"], "postComment");
                    if mode == "reject" {
                        send(
                            &mut socket,
                            json!({"type":"error","data":{"message":"NOT_ON_AIR"}}),
                        )
                        .await?;
                    } else if mode == "wrong_ack" || mode == "restricted" {
                        send(
                            &mut socket,
                            json!({"type":"postCommentResult","data":{"chat":{
                                "content": if mode == "wrong_ack" {"different"} else {"local test"},
                                "restricted": mode == "restricted"
                            }}}),
                        )
                        .await?;
                    }
                }
                drop(socket);
                assert!(!finished(&mut controller).await);
                if mode == "after" || mode == "wrong_ack" {
                    assert!(
                        matches!(controller.status(), Status::Unknown(_)),
                        "{mode}: {:?}",
                        controller.status()
                    );
                } else {
                    assert!(
                        matches!(controller.status(), Status::Failed(_)),
                        "{mode}: {:?}",
                        controller.status()
                    );
                }
                for _ in 0..10 {
                    controller.poll(Instant::now() + Duration::from_secs(30));
                    tokio::task::yield_now().await;
                }
                assert!(
                    timeout(Duration::from_millis(10), listener.accept())
                        .await
                        .is_err(),
                    "no automatic retry"
                );
            }
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        })
        .await
    })??;
    Ok(())
}

#[test]
fn timeout_includes_handshake_and_acknowledgement_without_resending() -> TestResult {
    runtime()?.block_on(async {
        timeout(Duration::from_secs(3), async {
            for after_write in [false, true] {
                let listener = TcpListener::bind("127.0.0.1:0").await?;
                let url = target(&listener, 1)?.watch_url;
                let task = tokio::spawn(transport::post(
                    url,
                    "timeout test".into(),
                    Duration::from_millis(100),
                    std::sync::mpsc::sync_channel(1).0,
                ));
                let mut socket = accept(&listener).await?;
                if after_write {
                    metadata(&mut socket).await?;
                    next(&mut socket).await?;
                }
                let result = task.await?;
                if after_write {
                    assert!(matches!(result, Outcome::Unknown(Error::Timeout)));
                } else {
                    assert!(matches!(result, Outcome::Failed(Error::Timeout)));
                }
            }
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        })
        .await
    })??;
    Ok(())
}

#[test]
fn selection_disable_and_drop_cancel_without_publishing_old_success() -> TestResult {
    runtime()?.block_on(async { timeout(Duration::from_secs(4), async {
        for after_write in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").await?;
            let mut controller = Controller::default();
            controller.configure(Some(target(&listener, 1)?));
            controller.submit(&Handle::current(), "old draft", Instant::now());
            let mut socket = accept(&listener).await?;
            if after_write {
                metadata(&mut socket).await?; next(&mut socket).await?;
                send(&mut socket, json!({"type":"postCommentResult","data":{"chat":{"content":"old draft","restricted":false}}})).await?;
                tokio::task::yield_now().await;
            }
            // Different broadcast sharing the very same NX endpoint.
            controller.configure(Some(target(&listener, 2)?));
            assert!(controller.busy(), "abort must be joined before a replacement");
            assert!(!controller.submit(&Handle::current(), "new draft", Instant::now()));
            assert!(!finished(&mut controller).await, "old acknowledgement cannot clear a new draft");
            assert!(matches!(controller.status(), Status::Idle));
            controller.configure(None);
            assert!(!controller.available(Instant::now() + POST_INTERVAL));
        }
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let mut controller = Controller::default();
        controller.configure(Some(target(&listener, 1)?));
        controller.submit(&Handle::current(), "drop test", Instant::now());
        let mut socket = accept(&listener).await?;
        drop(controller);
        let closed = socket.next().await;
        assert!(closed.is_none() || matches!(closed, Some(Err(_))));
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    }).await })??;
    Ok(())
}
