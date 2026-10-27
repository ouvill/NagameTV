use super::{
    Client, State, Subscription, TIMING, Timing,
    tests::{chunk, requested},
};
use futures_util::FutureExt;
use std::{
    future::Future,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    runtime::Handle,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;
const ARRAY_PATH: &str = "/prefix/api/events/stream?resource=program";
const SSE_PATH: &str = "/prefix/events";

// Keep the executor runnable during loopback I/O: paused time must advance only
// at explicit deadlines, not automatically while the OS delivers a response.
async fn drive<F: Future>(operation: F) -> F::Output {
    tokio::pin!(operation);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let std::task::Poll::Ready(result) = futures_util::poll!(&mut operation) {
            return result;
        }
        assert!(
            Instant::now() < deadline,
            "loopback operation did not finish"
        );
        tokio::task::yield_now().await;
    }
}

async fn missing(listener: &TcpListener, path: &str, status: u16) -> TestResult {
    let (mut socket, request) = requested(listener).await?;
    assert!(request.starts_with(&format!("GET {path} ")), "{request}");
    socket
        .write_all(
            format!(
                "HTTP/1.1 {status} Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            )
            .as_bytes(),
        )
        .await?;
    Ok(())
}

async fn working(
    listener: &TcpListener,
    path: &str,
) -> Result<TcpStream, Box<dyn std::error::Error>> {
    let (mut socket, request) = requested(listener).await?;
    assert!(request.starts_with(&format!("GET {path} ")), "{request}");
    let content_type = if path == ARRAY_PATH {
        "application/json"
    } else {
        "text/event-stream"
    };
    assert!(
        request.contains(&format!("accept: {content_type}")),
        "{request}"
    );
    socket.write_all(format!("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Type: {content_type}\r\n\r\n").as_bytes()).await?;
    Ok(socket)
}

fn unsupported(subscription: &Subscription) -> Arc<super::Error> {
    match subscription.state() {
        State::Unsupported(error) => error,
        state => panic!("expected observable Unsupported, got {state:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn unsupported_rechecks_at_five_minutes_without_refresh_or_duplicate_outcomes() -> TestResult
{
    drive(async {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let url = format!("http://{}{ARRAY_PATH}", listener.local_addr()?);
        // An erroneous dirty gate is immediately observable without advancing std::Instant.
        let timing = Timing {
            refresh: Duration::ZERO,
            ..TIMING
        };
        let subscription =
            Subscription::start_with(&Handle::current(), &Client::new()?, url, timing);
        let mut state = subscription.state.clone();
        missing(&listener, ARRAY_PATH, 404).await?;
        missing(&listener, SSE_PATH, 405).await?;
        state
            .wait_for(|state| matches!(state, State::Unsupported(_)))
            .await?;
        let first = unsupported(&subscription);
        assert!(!subscription.take_refresh());

        for status in [405, 404] {
            state.borrow_and_update();
            let previous = unsupported(&subscription);
            tokio::time::advance(TIMING.reprobe - TIMING.tick).await;
            assert!(Arc::ptr_eq(&previous, &unsupported(&subscription)));
            assert!(!subscription.take_refresh());
            assert!(
                listener.accept().now_or_never().is_none(),
                "re-probed before five minutes"
            );
            tokio::time::advance(TIMING.tick).await;
            missing(&listener, ARRAY_PATH, 404).await?;
            assert!(
                Arc::ptr_eq(&previous, &unsupported(&subscription)),
                "probing is not recovery"
            );
            missing(&listener, SSE_PATH, status).await?;
            state.changed().await?;
            let current = unsupported(&subscription);
            // Controller publishes failures only when this Arc changes.
            assert_eq!(Arc::ptr_eq(&first, &current), status == 405);
            assert!(!subscription.take_refresh());
        }

        tokio::time::advance(TIMING.reprobe).await;
        let mut socket = working(&listener, ARRAY_PATH).await?;
        state
            .wait_for(|state| matches!(state, State::Receiving))
            .await?;
        assert!(
            !subscription.take_refresh(),
            "capability recovery alone must not request programs"
        );
        chunk(&mut socket, br#"[{"resource":"program","type":"update"},"#).await?;
        while !subscription.take_refresh() {
            tokio::task::yield_now().await;
        }
        subscription.stop().wait().await?;
        let mut byte = [0];
        assert_eq!(socket.read(&mut byte).await?, 0);
        Ok(())
    })
    .await
}

#[tokio::test(start_paused = true)]
async fn vanished_working_endpoints_redetect_once_at_the_same_address() -> TestResult {
    drive(async {
        for initial in [ARRAY_PATH, SSE_PATH] {
            for status in [404, 405] {
                let listener = TcpListener::bind("127.0.0.1:0").await?;
                let url = format!("http://{}{ARRAY_PATH}", listener.local_addr()?);
                let timing = Timing {
                    refresh: Duration::ZERO,
                    ..TIMING
                };
                let subscription =
                    Subscription::start_with(&Handle::current(), &Client::new()?, url, timing);
                let mut state = subscription.state.clone();
                if initial == SSE_PATH {
                    missing(&listener, ARRAY_PATH, 404).await?;
                }
                let mut socket = working(&listener, initial).await?;
                state
                    .wait_for(|state| matches!(state, State::Receiving))
                    .await?;
                socket.write_all(b"0\r\n\r\n").await?;
                drop(socket);
                state
                    .wait_for(|state| matches!(state, State::Retrying(_)))
                    .await?;
                assert!(
                    subscription.take_refresh(),
                    "EOF still reconciles missed changes"
                );
                tokio::time::advance(TIMING.retry).await;
                missing(&listener, initial, status).await?;
                let replacement = if initial == SSE_PATH {
                    ARRAY_PATH
                } else {
                    SSE_PATH
                };
                if replacement == SSE_PATH {
                    missing(&listener, ARRAY_PATH, 404).await?;
                }
                let mut socket = working(&listener, replacement).await?;
                state
                    .wait_for(|state| matches!(state, State::Receiving))
                    .await?;
                assert!(
                    !subscription.take_refresh(),
                    "detection itself must not refresh"
                );
                let event = if replacement == ARRAY_PATH {
                    br#"[{"resource":"program","type":"update"},"#.as_slice()
                } else {
                    b"event: epg.programs-updated\ndata: {\"serviceId\":3272402081}\n\n".as_slice()
                };
                chunk(&mut socket, event).await?;
                while !subscription.take_refresh() {
                    tokio::task::yield_now().await;
                }
                assert!(matches!(subscription.state(), State::Receiving));

                // If neither protocol remains, one full detection cycle is enough.
                socket.write_all(b"0\r\n\r\n").await?;
                drop(socket);
                state
                    .wait_for(|state| matches!(state, State::Retrying(_)))
                    .await?;
                assert!(subscription.take_refresh());
                tokio::time::advance(TIMING.retry).await;
                missing(&listener, replacement, status).await?;
                missing(&listener, ARRAY_PATH, 404).await?;
                missing(&listener, SSE_PATH, 405).await?;
                state
                    .wait_for(|state| matches!(state, State::Unsupported(_)))
                    .await?;
                tokio::time::advance(TIMING.retry).await;
                assert!(matches!(subscription.state(), State::Unsupported(_)));
                assert!(!subscription.take_refresh());
                assert!(
                    listener.accept().now_or_never().is_none(),
                    "detection entered a fast loop"
                );
                subscription.stop().wait().await?;
            }
        }
        Ok(())
    })
    .await
}
