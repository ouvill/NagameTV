use super::*;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

pub(in crate::playback) fn serve_ts(bytes: Vec<u8>) -> MockServer {
    runtime().unwrap().block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/videos/123"))
            .respond_with(move |request: &wiremock::Request| {
                assert_eq!(request.headers[header::ACCEPT_ENCODING], "identity");
                let range = request.headers[header::RANGE]
                    .to_str()
                    .unwrap()
                    .strip_prefix("bytes=")
                    .unwrap();
                let (start, end) = range.split_once('-').unwrap();
                let start: usize = start.parse().unwrap();
                let end = end.parse::<usize>().unwrap();
                if start >= bytes.len() || end >= bytes.len() {
                    return ResponseTemplate::new(416);
                }
                ResponseTemplate::new(206)
                    .insert_header(
                        "Content-Range",
                        format!("bytes {start}-{end}/{}", bytes.len()),
                    )
                    .insert_header("ETag", "\"recording-v1\"")
                    .set_body_bytes(bytes[start..=end].to_vec())
            })
            .mount(&server)
            .await;
        server
    })
}
fn inspect(server: &MockServer) -> Result<Cursor, Error> {
    Verified::inspect(
        Url::parse(&format!("{}/api/videos/123?token=private", server.uri())).unwrap(),
        Arc::new(AtomicBool::new(false)),
    )
}
fn requests(server: &MockServer) -> Vec<wiremock::Request> {
    runtime()
        .unwrap()
        .block_on(server.received_requests())
        .unwrap()
}
fn response(template: ResponseTemplate) -> MockServer {
    runtime().unwrap().block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(template)
            .mount(&server)
            .await;
        server
    })
}

#[test]
fn bounded_random_reads_preserve_offsets_query_and_if_range() {
    const BLOCKS: usize = 8;
    let bytes: Vec<u8> = (0..RANGE_BYTES * BLOCKS).map(|n| (n % 251) as u8).collect();
    let server = serve_ts(bytes.clone());
    let mut cursor = inspect(&server).unwrap();
    let mut block = [0; 1024];
    cursor.read_exact(&mut block).unwrap();
    assert_eq!(block, bytes[..block.len()]);
    cursor.seek(SeekFrom::End(-(block.len() as i64))).unwrap();
    cursor.read_exact(&mut block).unwrap();
    assert_eq!(block, bytes[bytes.len() - block.len()..]);
    assert_eq!(cursor.read(&mut block).unwrap(), 0);
    cursor.seek(SeekFrom::Start(77)).unwrap();
    cursor.read_exact(&mut block).unwrap();
    assert_eq!(block, bytes[77..77 + block.len()]);
    assert!(
        cursor
            .seek(SeekFrom::Current(-((77 + block.len() + 1) as i64)))
            .is_err()
    );
    let requests = requests(&server);
    assert_eq!(requests.len(), 4);
    assert!(
        requests
            .iter()
            .all(|request| request.url.query() == Some("token=private"))
    );
    assert_eq!(requests[1].headers[header::IF_RANGE], "\"recording-v1\"");
    assert_eq!(
        requests[3].headers[header::RANGE],
        format!("bytes=77-{}", 77 + RANGE_BYTES - 1)
    );
}

#[test]
fn epgstation_short_recording_and_final_byte_never_request_beyond_eof() {
    let bytes = include_bytes!("../../../../../tests/fixtures/recording.ts").to_vec();
    let server = serve_ts(bytes.clone());
    let mut cursor = inspect(&server).unwrap();
    cursor.seek(SeekFrom::End(-1)).unwrap();
    let mut last = [0];
    cursor.read_exact(&mut last).unwrap();
    assert_eq!(last[0], bytes[bytes.len() - 1]);
    assert_eq!(cursor.read(&mut last).unwrap(), 0);
    let requests = requests(&server);
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].headers[header::RANGE], "bytes=0-1");
    assert_eq!(
        requests[1].headers[header::RANGE],
        format!("bytes={}-{}", bytes.len() - 2, bytes.len() - 1)
    );
}

#[test]
fn redirects_and_resources_without_validators_remain_seekable() {
    let server = runtime().unwrap().block_on(async {
        let server = MockServer::start().await;
        Mock::given(path("/api/videos/123"))
            .respond_with(
                ResponseTemplate::new(302).insert_header("Location", "/recording?signature=kept"),
            )
            .mount(&server)
            .await;
        Mock::given(path("/recording"))
            .respond_with(|request: &wiremock::Request| {
                assert_eq!(request.url.query(), Some("signature=kept"));
                let range = request.headers[header::RANGE]
                    .to_str()
                    .unwrap()
                    .strip_prefix("bytes=")
                    .unwrap();
                let (start, end) = range.split_once('-').unwrap();
                let start = start.parse::<usize>().unwrap();
                let end = end.parse::<usize>().unwrap();
                const SIZE: usize = 100;
                assert!(end < SIZE);
                ResponseTemplate::new(206)
                    .insert_header("Content-Range", format!("bytes {start}-{end}/{SIZE}"))
                    .set_body_bytes(vec![42; end - start + 1])
            })
            .mount(&server)
            .await;
        server
    });
    let mut cursor = inspect(&server).unwrap();
    cursor.seek(SeekFrom::End(-10)).unwrap();
    let mut bytes = [0; 10];
    cursor.read_exact(&mut bytes).unwrap();
    assert_eq!(bytes, [42; 10]);
    assert_eq!(requests(&server).len(), 4);
}

#[test]
fn range_ignored_malformed_encoded_and_error_responses_fail_without_exposing_url() {
    for template in [
        ResponseTemplate::new(200).set_body_string("login page"),
        ResponseTemplate::new(206)
            .insert_header("Content-Range", "bytes 1-9/10")
            .set_body_bytes(vec![0; 9]),
        ResponseTemplate::new(206)
            .insert_header("Content-Range", "bytes 0-9/*")
            .set_body_bytes(vec![0; 10]),
        ResponseTemplate::new(206)
            .insert_header("Content-Range", "bytes 0-9/10")
            .insert_header("Content-Encoding", "gzip")
            .set_body_bytes(vec![0; 10]),
        ResponseTemplate::new(401),
        ResponseTemplate::new(404),
        ResponseTemplate::new(500),
    ] {
        let server = response(template);
        let error = inspect(&server).err().expect("invalid response accepted");
        assert!(!error.to_string().contains("private"));
        assert!(!format!("{error:?}").contains("private"));
        assert_eq!(requests(&server).len(), 1);
    }
}

#[test]
fn changed_resource_and_short_body_are_not_spliced_into_recording() {
    let server = serve_ts(vec![0; RANGE_BYTES * 2]);
    let original = inspect(&server).unwrap().source;
    for template in [
        ResponseTemplate::new(200),
        ResponseTemplate::new(206)
            .insert_header(
                "Content-Range",
                format!("bytes 0-{}/{}", RANGE_BYTES - 1, RANGE_BYTES * 2),
            )
            .insert_header("ETag", "\"recording-v2\"")
            .set_body_bytes(vec![0; RANGE_BYTES]),
        ResponseTemplate::new(206)
            .insert_header(
                "Content-Range",
                format!("bytes 0-{}/{}", RANGE_BYTES - 1, RANGE_BYTES * 2 + 1),
            )
            .insert_header("ETag", "\"recording-v1\"")
            .set_body_bytes(vec![0; RANGE_BYTES]),
        ResponseTemplate::new(206)
            .insert_header(
                "Content-Range",
                format!("bytes 0-{}/{}", RANGE_BYTES - 1, RANGE_BYTES * 2),
            )
            .insert_header("ETag", "\"recording-v1\"")
            .set_body_bytes(vec![0; 10]),
    ] {
        runtime().unwrap().block_on(async {
            server.reset().await;
            Mock::given(method("GET"))
                .respond_with(template)
                .mount(&server)
                .await;
        });
        assert!(
            original
                .cursor(Arc::new(AtomicBool::new(false)))
                .read(&mut [0; 1])
                .is_err()
        );
    }
}

#[test]
fn cancellation_interrupts_a_stalled_response() {
    use std::{io::Write, net::TcpListener, sync::mpsc, thread};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!(
        "http://{}/api/videos/123",
        listener.local_addr().unwrap()
    ))
    .unwrap();
    let (received, wait) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket.set_read_timeout(Some(REQUEST_TIMEOUT)).unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            socket.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        socket.write_all(b"HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-1/100\r\nContent-Length: 2\r\n\r\n").unwrap();
        received.send(()).unwrap();
        // Cancelling the request must release its TCP connection, without EOF from the server.
        assert_eq!(socket.read(&mut [0; 1]).unwrap(), 0);
    });
    let cancelled = Arc::new(AtomicBool::new(false));
    let flag = cancelled.clone();
    let worker = thread::spawn(move || Verified::inspect(url, flag));
    wait.recv_timeout(REQUEST_TIMEOUT).unwrap();
    cancelled.store(true, Ordering::Release);
    assert!(matches!(worker.join().unwrap(), Err(Error::Cancelled)));
    server.join().unwrap();
}
