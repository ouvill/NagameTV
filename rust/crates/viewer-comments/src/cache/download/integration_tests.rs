use super::spool::Receipt;
use super::*;
use std::{
    io::{BufWriter, Read, Write},
    net::{TcpListener, TcpStream},
};
#[test]
fn gzip_and_a_progressing_response_longer_than_ten_seconds_are_received_to_completion() {
    for compressed in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            request(&mut socket);
            if compressed {
                let mut gzip =
                    flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
                gzip.write_all(br#"{"packet":[]}"#).unwrap();
                let bytes = gzip.finish().unwrap();
                write!(
                    socket,
                    "HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: {}\r\n\r\n",
                    bytes.len()
                )
                .unwrap();
                socket.write_all(&bytes).unwrap();
            } else {
                socket
                    .write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n")
                    .unwrap();
                for byte in br#"{"packet":[]}"# {
                    socket.write_all(&[*byte]).unwrap();
                    thread::sleep(Duration::from_millis(850));
                }
            }
        });
        let range = Interval::new(100, 200).unwrap();
        let receipt = Receipt {
            id: "progress".into(),
            channel: 1,
            range,
            fetched: 300_000,
            generation: 0,
        };
        let incoming = spool::Receiving::prepare(dir.path(), receipt).unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let start = Instant::now();
        let downloaded = match runtime.block_on(receive(incoming, &endpoint, &Cancellation::new()))
        {
            Ok(file) => file,
            Err(_) => panic!("progressing response failed"),
        };
        assert!(downloaded.validate().is_ok());
        server.join().unwrap();
        if !compressed {
            assert!(start.elapsed() > Duration::from_secs(10));
        }
    }
}
fn request(socket: &mut TcpStream) -> String {
    socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        assert!(header.len() < 8192);
        let mut byte = [0];
        socket.read_exact(&mut byte).unwrap();
        header.push(byte[0]);
    }
    String::from_utf8(header).unwrap()
}
#[test]
fn large_response_without_content_length_is_fetched_once_and_all_rows_reach_disk() {
    const COUNT: u64 = 120_001;
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let owner = store.session().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let server_listener = listener.try_clone().unwrap();
    let endpoint = format!("http://{}/api/kakolog", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut socket, _) = server_listener.accept().unwrap();
        let header = request(&mut socket);
        assert!(
            header.starts_with("GET /api/kakolog/jk1?starttime=100000&endtime=100599&format=json ")
        );
        let mut out = BufWriter::new(socket);
        write!(
            out,
            "HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{{\"packet\":["
        )
        .unwrap();
        let text = "x".repeat(400);
        let mut bytes = 0;
        for id in 0..COUNT {
            let row = format!(
                "{}{{\"chat\":{{\"date\":{},\"thread\":1,\"no\":{},\"content\":\"{}\"}}}}",
                if id == 0 { "" } else { "," },
                100_000 + id / 1000,
                id,
                text
            );
            bytes += row.len();
            out.write_all(row.as_bytes()).unwrap();
        }
        out.write_all(b"]}").unwrap();
        out.flush().unwrap();
        assert!(bytes > 32 * 1024 * 1024);
    });
    let demand = Demand {
        source: 1,
        channel: 1,
        view: Some(View {
            clock_key: "clock".into(),
            interval: Interval::new(100_000, 100_100).unwrap(),
        }),
        fetch: true,
        source_range: Source::Recording(RecordingRange::Known(vec![ClockSpan {
            key: "clock".into(),
            channel: 1,
            media_start_ms: 0,
            media_end_ms: 600_000,
            utc_start_ms: 100_000_000,
        }])),
    };
    let cancel = Cancellation::new();
    assert!(matches!(
        run(
            dir.path().into(),
            owner.name.clone(),
            demand.clone(),
            endpoint.clone(),
            300_000,
            &cancel
        )
        .unwrap(),
        Outcome::Complete
    ));
    server.join().unwrap();
    let db = rusqlite::Connection::open(dir.path().join("cache.sqlite3")).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM comments", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        COUNT as i64
    );
    let records = store
        .read(
            &owner.name,
            1,
            &View {
                clock_key: "clock".into(),
                interval: Interval::new(100_120, 100_121).unwrap(),
            },
            300_001,
        )
        .unwrap();
    assert!(
        records
            .iter()
            .any(|r| r.comment.source_id == Some((1, COUNT - 1)))
    );
    assert!(matches!(
        run(
            dir.path().into(),
            owner.name.clone(),
            demand,
            endpoint,
            300_001,
            &cancel
        )
        .unwrap(),
        Outcome::Idle
    ));
    listener.set_nonblocking(true).unwrap();
    assert!(matches!(listener.accept(),Err(error) if error.kind()==std::io::ErrorKind::WouldBlock));
    assert!(!dir.path().join("response.json").exists());
}
#[test]
fn retry_after_and_http_failure_do_not_become_empty_coverage() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let owner = store.session().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        request(&mut socket);
        socket
            .write_all(
                b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 3600\r\nContent-Length: 0\r\n\r\n",
            )
            .unwrap();
    });
    let span = ClockSpan {
        key: "clock".into(),
        channel: 1,
        media_start_ms: 0,
        media_end_ms: 60_000,
        utc_start_ms: 100_000_000,
    };
    let demand = Demand {
        source: 1,
        channel: 1,
        view: Some(View {
            clock_key: "clock".into(),
            interval: Interval::new(100_000, 100_060).unwrap(),
        }),
        fetch: true,
        source_range: Source::Recording(RecordingRange::Known(vec![span])),
    };
    assert!(matches!(
        run(
            dir.path().into(),
            owner.name.clone(),
            demand,
            endpoint,
            300_000,
            &Cancellation::new()
        )
        .unwrap(),
        Outcome::RemoteFailed(_)
    ));
    server.join().unwrap();
    assert!(
        store
            .covered(1, Interval::new(100_000, 100_060).unwrap(), 300_000)
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        store
            .reserve(
                &store.try_provider().unwrap().unwrap(),
                2,
                Interval::new(100_000, 100_060).unwrap(),
                300_030
            )
            .unwrap(),
        Reservation::Waiting(_)
    ));
}
