//! Hardware-free regressions against the same HTTP fixture used by Main.qml.
use super::*;
use crate::{
    epgstation::{Library, Login},
    services::Network,
};
use std::{
    io::{Read, Write},
    net::TcpStream,
    thread,
    time::{Duration, Instant},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;
const LOGIN: &[u8] = br#"{"name":"viewer","password":"fixture-password"}"#;

fn connect(server: &Server) -> std::io::Result<TcpStream> {
    let socket = TcpStream::connect_timeout(server.mock.address(), Duration::from_secs(5))?;
    socket.set_read_timeout(Some(Duration::from_secs(5)))?;
    socket.set_write_timeout(Some(Duration::from_secs(5)))?;
    Ok(socket)
}

fn login_header() -> String {
    format!(
        "POST /protected/api/auth/login HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        LOGIN.len()
    )
}

#[test]
fn epgstation_login_consumes_split_body_and_closes_cleanly() -> TestResult {
    let server = Server::new()?;
    let mut socket = connect(&server)?;
    socket.write_all(login_header().as_bytes())?;
    socket.write_all(&LOGIN[..LOGIN.len() - 1])?;
    socket.set_read_timeout(Some(Duration::from_millis(100)))?;
    let mut byte = [0];
    // An incomplete POST must not receive a response. The old fixture replied
    // after the headers and reset the connection with unread credentials.
    assert!(matches!(socket.read(&mut byte), Err(error)
        if matches!(error.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut)));
    socket.set_read_timeout(Some(Duration::from_secs(5)))?;
    socket.write_all(&LOGIN[LOGIN.len() - 1..])?;
    let mut response = String::new();
    socket.read_to_string(&mut response)?;
    assert!(response.starts_with("HTTP/1.1 200"));
    assert!(response.contains("epgstation_session=fixture"));
    let (_, body) = response
        .split_once("\r\n\r\n")
        .ok_or("missing login response body")?;
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(body)?["user"]["name"],
        "viewer"
    );
    Ok(())
}

#[test]
fn epgstation_cancellations_leave_other_requests_usable() -> TestResult {
    let server = Server::new()?;
    drop(connect(&server)?);
    let mut partial_header = connect(&server)?;
    partial_header.write_all(b"GET /api/recorded HTTP/1.1\r\n")?;
    drop(partial_header);
    let mut partial_body = connect(&server)?;
    partial_body.write_all(login_header().as_bytes())?;
    partial_body.write_all(&LOGIN[..1])?;
    drop(partial_body);

    let mut video = connect(&server)?;
    video.write_all(
        b"GET /api/videos/123 HTTP/1.1\r\nHost: localhost\r\nRange: bytes=0-524287\r\n\r\n",
    )?;
    video.read_exact(&mut [0])?;
    // Playback replacement may discard an in-flight range response.
    drop(video);

    let mut response = String::new();
    let mut services = connect(&server)?;
    services
        .write_all(b"GET /api/services HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")?;
    services.read_to_string(&mut response)?;
    assert!(response.starts_with("HTTP/1.1 200"));
    assert_eq!(server.requests(), 1);
    Ok(())
}

#[test]
fn epgstation_repeated_logins_progress_beside_an_incomplete_request() -> TestResult {
    let server = Server::new()?;
    let mut incomplete = connect(&server)?;
    incomplete.write_all(login_header().as_bytes())?;
    incomplete.write_all(&LOGIN[..1])?;
    let network = Network::new()?;
    let mut library = Library::default();
    for _ in 0..100 {
        library.open(
            &network,
            &format!("{}/protected", server.url()),
            "",
            Login::password("viewer".into(), "fixture-password".into())?,
        )?;
        let deadline = Instant::now() + Duration::from_secs(5);
        while library.busy() {
            library.poll(&network);
            assert!(
                Instant::now() < deadline,
                "EPGStation fixture login did not finish"
            );
            thread::sleep(Duration::from_millis(1));
        }
        assert!(library.error().is_none(), "{:?}", library.error());
        assert_eq!(library.rows().len(), LIBRARY_RECORDINGS);
        assert_eq!(
            library.rows()[0].service,
            Some(crate::channels::BroadcastService {
                network_id: 4,
                service_id: 101
            })
        );
        assert!(
            library
                .playback_url(1)
                .ok_or("missing playback URL")?
                .ends_with("/protected/api/videos/123?token=playback-fixture")
        );
    }
    drop(incomplete);
    Ok(())
}

#[test]
fn epgstation_fixture_rejects_unknown_routes_and_missing_authentication() -> TestResult {
    let server = Server::new()?;
    for (path, status) in [
        ("/api/recroded", "404"),
        ("/api/recorded?isHalfWidth=false&unexpected=1", "400"),
        ("/protected/api/recorded?isHalfWidth=false", "401"),
        ("/protected/api/channels", "401"),
        ("/protected/api/videos/124/metadata", "401"),
        ("/protected/api/videos/124", "401"),
    ] {
        let mut socket = connect(&server)?;
        write!(
            socket,
            "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
        )?;
        let mut response = String::new();
        socket.read_to_string(&mut response)?;
        assert!(
            response.starts_with(&format!("HTTP/1.1 {status}")),
            "unexpected status for {path}"
        );
    }
    Ok(())
}
