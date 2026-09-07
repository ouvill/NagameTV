use super::*;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
fn runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}
fn endpoints(listener: &TcpListener) -> std::io::Result<Endpoints> {
    let address = listener.local_addr()?;
    Ok(Endpoints {
        threads: format!("http://{address}/threads"),
        comments: format!("ws://{address}/comments"),
    })
}
async fn request(listener: &TcpListener) -> TestResult<tokio::net::TcpStream> {
    let (mut stream, _) = listener.accept().await?;
    let mut request = Vec::new();
    while !request.ends_with(b"\r\n\r\n") {
        let byte = stream.read_u8().await?;
        request.push(byte);
        assert!(request.len() < 8192);
    }
    Ok(stream)
}
async fn terminal(connection: &Connection) -> State {
    loop {
        let state = connection.state();
        if matches!(state, State::Ended | State::Failed(_)) {
            return state;
        }
        tokio::task::yield_now().await;
    }
}

#[test]
fn receives_history_and_live_comments_without_losing_terminal_status_under_overload() -> TestResult
{
    runtime()?.block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let urls = endpoints(&listener)?;
        let server = tokio::spawn(async move {
            let mut http = request(&listener).await?;
            let body = r#"[{"id":18446744073709551615,"status":"ACTIVE"}]"#;
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
            let message = socket.next().await.ok_or("no subscription")??;
            let request: serde_json::Value = serde_json::from_str(message.to_text()?)?;
            assert_eq!(request[2]["thread"]["thread"], "18446744073709551615");
            socket
                .send(Message::Ping(b"heartbeat".as_slice().into()))
                .await?;
            assert!(matches!(
                socket.next().await.ok_or("no pong")??,
                Message::Pong(_)
            ));
            socket.send(Message::Text("broken JSON".into())).await?;
            for index in 0..300 {
                if index == 1 {
                    socket
                        .send(Message::Text(r#"{"ping":{"content":"rf:0"}}"#.into()))
                        .await?;
                }
                socket
                    .send(Message::Text(
                        format!(r#"{{"chat":{{"content":"{index}"}}}}"#).into(),
                    ))
                    .await?;
            }
            socket.close(None).await?;
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        });
        let connection = Connection::start(&Handle::current(), reqwest::Client::new(), urls);
        let state = timeout(Duration::from_secs(5), terminal(&connection)).await?;
        assert!(matches!(state, State::Ended), "{state:?}");
        let comments: Vec<_> = std::iter::from_fn(|| connection.try_next()).collect();
        assert_eq!(comments.len(), QUEUE_CAPACITY);
        assert_eq!(connection.dropped(), 300 - QUEUE_CAPACITY as u64);
        assert_eq!(comments[0].phase, crate::Phase::History);
        assert_eq!(comments[1].phase, crate::Phase::Live);
        server.await??;
        connection.stop().wait().await?;
        Ok(())
    })
}

#[test]
fn rejects_oversized_http_body_before_websocket_connection() -> TestResult {
    runtime()?.block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let urls = endpoints(&listener)?;
        let server = tokio::spawn(async move {
            let mut http = request(&listener).await?;
            http.write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    MAX_THREAD_LIST_BYTES + 1
                )
                .as_bytes(),
            )
            .await?;
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        });
        let connection = Connection::start(&Handle::current(), reqwest::Client::new(), urls);
        let State::Failed(error) = timeout(Duration::from_secs(5), terminal(&connection)).await?
        else {
            panic!("expected capacity failure");
        };
        assert!(matches!(
            &*error,
            Error::Protocol(crate::Error::TooLarge { .. })
        ));
        assert!(connection.try_next().is_none());
        server.await??;
        connection.stop().wait().await?;
        Ok(())
    })
}

#[test]
fn cancellation_joins_a_task_waiting_for_http_headers() -> TestResult {
    runtime()?.block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let urls = endpoints(&listener)?;
        let connection = Connection::start(&Handle::current(), reqwest::Client::new(), urls);
        let mut http = timeout(Duration::from_secs(5), request(&listener)).await??;
        let stopping = connection.stop();
        timeout(Duration::from_secs(2), stopping.wait()).await??;
        let mut byte = [0];
        assert_eq!(
            timeout(Duration::from_secs(2), http.read(&mut byte)).await??,
            0
        );
        Ok(())
    })
}

async fn accept_comments(
    listener: &TcpListener,
) -> TestResult<tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>> {
    let mut http = request(listener).await?;
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
    assert!(matches!(
        socket.next().await.ok_or("no subscription")??,
        Message::Text(_)
    ));
    Ok(socket)
}

#[test]
fn cancellation_releases_an_idle_websocket() -> TestResult {
    runtime()?.block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let connection = Connection::start(
            &Handle::current(),
            reqwest::Client::new(),
            endpoints(&listener)?,
        );
        let mut socket = timeout(Duration::from_secs(5), accept_comments(&listener)).await??;
        timeout(Duration::from_secs(2), connection.stop().wait()).await??;
        // Cancellation drops the transport; it does not wait for a close handshake.
        let result = timeout(Duration::from_secs(2), socket.next()).await?;
        assert!(result.is_none() || matches!(result, Some(Err(_))));
        Ok(())
    })
}

#[test]
fn websocket_size_limit_fails_before_json_decoding() -> TestResult {
    runtime()?.block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let connection = Connection::start(
            &Handle::current(),
            reqwest::Client::new(),
            endpoints(&listener)?,
        );
        let mut socket = timeout(Duration::from_secs(5), accept_comments(&listener)).await??;
        socket
            .send(Message::Text("x".repeat(MAX_MESSAGE_BYTES + 1).into()))
            .await?;
        let State::Failed(error) = timeout(Duration::from_secs(5), terminal(&connection)).await?
        else {
            panic!("expected transport limit failure");
        };
        assert!(matches!(
            &*error,
            Error::WebSocket(tungstenite::Error::Capacity(_))
        ));
        assert!(connection.try_next().is_none());
        connection.stop().wait().await?;
        Ok(())
    })
}

#[test]
#[ignore = "manual public-service reception; requires COMMENT_THREADS_URL and COMMENT_STREAM_URL"]
fn real_service_reception_and_stop() -> TestResult {
    let endpoints = Endpoints {
        threads: std::env::var("COMMENT_THREADS_URL")?,
        comments: std::env::var("COMMENT_STREAM_URL")?,
    };
    runtime()?.block_on(async {
        let client = reqwest::Client::builder().timeout(IO_TIMEOUT).build()?;
        let connection = Connection::start(&Handle::current(), client, endpoints);
        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        let mut receiving = false;
        let mut history = 0;
        let mut live = 0;
        let mut failure = None;
        while tokio::time::Instant::now() < deadline {
            match connection.state() {
                State::Connecting => {},
                State::Receiving => receiving = true,
                State::Ended => { failure = Some("comment stream ended".to_owned()); break; },
                State::Failed(error) => { failure = Some(error.to_string()); break; },
            }
            // Match the application's bounded drain; retain only counts, never text.
            for _ in 0..64 {
                let Some(comment) = connection.try_next() else { break; };
                match comment.phase {
                    crate::Phase::History => history += 1,
                    crate::Phase::Live => live += 1,
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let dropped = connection.dropped();
        timeout(Duration::from_secs(5), connection.stop().wait()).await??;
        eprintln!("REAL_COMMENTS receiving={receiving} history={history} live={live} dropped={dropped} stopped=true");
        if let Some(failure) = failure { return Err(failure.into()); }
        assert!(receiving, "service never entered reception");
        Ok(())
    })
}
