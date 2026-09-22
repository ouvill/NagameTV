use super::spool::Receipt;
use super::*;
use futures_lite::future::block_on;
use std::{
    io::{BufWriter, Read, Write},
    net::{TcpListener, TcpStream},
};

#[test]
fn recording_prefetch_keeps_old_fragments_and_saves_both_origins_ahead_of_the_scan() {
    const START: i64 = 100_000;
    const CACHED_SECONDS: i64 = 8;
    const SCANNED_SECONDS: i64 = 3;
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let owner = store.session().unwrap();
    let mut cached = spool::Receiving::prepare(
        dir.path(),
        Receipt {
            id: "old-fragment".into(),
            channel: 4,
            range: Interval::new(START, START + CACHED_SECONDS).unwrap(),
            fetched: 300_000,
            generation: store.generation().unwrap(),
            plan: None,
        },
    )
    .unwrap();
    cached
        .write(br#"{"packet":[{"chat":{"date":100001,"content":"saved"}}]}"#)
        .unwrap();
    let cached = cached
        .finish()
        .unwrap()
        .validate()
        .map_err(|(_, e)| e)
        .unwrap();
    cached.import(&mut store, || false).unwrap();
    cached.finish().unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        assert!(request(&mut socket).starts_with(&format!(
            "GET /jk4?starttime={}&endtime={}&format=json ",
            START + CACHED_SECONDS,
            START + FALLBACK_SECONDS - 1
        )));
        let packet: Vec<_> = (START + CACHED_SECONDS..START + FALLBACK_SECONDS)
            .map(|time| {
                serde_json::json!({"chat": {
                    "date": time, "content": "ahead", "nx_jikkyo": time % 2
                }})
            })
            .collect();
        let bytes = serde_json::to_vec(&serde_json::json!({"packet": packet})).unwrap();
        write!(
            socket,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
            bytes.len()
        )
        .unwrap();
        socket.write_all(&bytes).unwrap();
    });
    let mut span = ClockSpan {
        key: "clock".into(),
        channel: 4,
        media_start_ms: 0,
        media_end_ms: SCANNED_SECONDS * 1000,
        utc_start_ms: START * 1000,
    };
    let mut demand = Demand {
        source: 1,
        channel: 4,
        fetch: true,
        view: Some(View {
            clock_key: span.key.clone(),
            interval: Interval::new(START + 1 - LOOKBACK_SECONDS, START + 121).unwrap(),
        }),
        source_range: recording(
            START + plan::PROGRAM_PADDING_SECONDS,
            FALLBACK_SECONDS - 2 * plan::PROGRAM_PADDING_SECONDS,
            START,
        ),
    };
    let fetch = |demand: Demand| {
        run(
            dir.path().into(),
            owner.name.clone(),
            demand,
            endpoint.clone(),
            300_001,
            &Cancellation::new(),
        )
        .unwrap()
    };
    assert!(matches!(fetch(demand.clone()), Outcome::Complete));
    server.join().unwrap();
    assert!(store.imported("old-fragment").unwrap());
    assert!(
        Interval::new(START, START + FALLBACK_SECONDS)
            .unwrap()
            .missing(
                store
                    .covered(
                        4,
                        Interval::new(START, START + FALLBACK_SECONDS).unwrap(),
                        300_001
                    )
                    .unwrap()
            )
            .is_empty()
    );
    // Both providers are already on disk beyond the decoder's scanned edge.
    const CHECK_SECONDS: i64 = 120;
    let records = store
        .read(
            &owner.name,
            4,
            &View {
                clock_key: span.key.clone(),
                interval: Interval::new(START + CHECK_SECONDS, START + CHECK_SECONDS * 2).unwrap(),
            },
        )
        .unwrap();
    assert_eq!(records.len(), CHECK_SECONDS as usize);
    for origin in [Origin::Nx, Origin::Niconico] {
        assert_eq!(
            records
                .iter()
                .filter(|record| record.comment.origin == origin)
                .count(),
            CHECK_SECONDS as usize / 2
        );
    }
    // Advancing the scan past the first HTTP spacing period needs no new GET.
    let playhead = START + REQUEST_SPACING_SECONDS + 1;
    span.media_end_ms = (playhead - START + SCANNED_SECONDS) * 1000;
    demand.source_range = recording(
        START + plan::PROGRAM_PADDING_SECONDS,
        FALLBACK_SECONDS - 2 * plan::PROGRAM_PADDING_SECONDS,
        playhead,
    );
    demand.view.as_mut().unwrap().interval =
        Interval::new(playhead - LOOKBACK_SECONDS, playhead + CHECK_SECONDS).unwrap();
    assert!(matches!(fetch(demand), Outcome::Idle));
}

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
            plan: None,
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
        source_range: recording(100_120, 360, 100_016),
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
    let mut db = super::super::test_database::open(dir.path().join("cache.sqlite3")).unwrap();
    assert_eq!(
        block_on(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM comments").fetch_one(&mut db))
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
    let demand = Demand {
        source: 1,
        channel: 1,
        view: Some(View {
            clock_key: "clock".into(),
            interval: Interval::new(100_000, 100_060).unwrap(),
        }),
        fetch: true,
        source_range: recording(100_000, 60, 100_016),
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
        Outcome::RemoteFailed { .. }
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

fn recording(start: i64, duration: i64, utc: i64) -> Source {
    Source::Recording(Recording::Observed {
        current: Program::new(
            ProgramId {
                network: 1,
                transport: 1,
                service: 1,
                event: 1,
            },
            start,
            duration,
        ),
        next: None,
        utc_seconds: Some(utc),
        at_start: false,
    })
}

#[test]
fn recent_program_adds_only_available_tail_when_view_approaches_or_program_finishes() {
    const START: i64 = 100_000;
    const DURATION: i64 = 7200;
    const NOW: i64 = START + 3600;
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let owner = store.session().unwrap();
    let demand = Demand {
        source: 1,
        channel: 4,
        fetch: true,
        view: None,
        source_range: recording(START, DURATION, START),
    };
    let target = plan::Planner::default().update(&demand, NOW).unwrap();
    let mut acquisition = Acquisition {
        target: target.clone(),
        focus: Some(START),
    };
    let Planned::Ready(request) = next(&mut store, &owner.name, &acquisition, NOW).unwrap() else {
        panic!("initial prefix")
    };
    assert_eq!(request.range.end, archive_end(NOW));
    let prefix = request.range;
    let mut receiving = spool::Receiving::prepare(
        dir.path(),
        Receipt {
            id: "recent-prefix".into(),
            channel: 4,
            range: prefix,
            fetched: NOW,
            generation: store.generation().unwrap(),
            plan: Some(request),
        },
    )
    .unwrap();
    receiving.write(br#"{"packet":[]}"#).unwrap();
    let validated = receiving
        .finish()
        .unwrap()
        .validate()
        .map_err(|(_, e)| e)
        .unwrap();
    validated.import(&mut store, || false).unwrap();
    validated.finish().unwrap();
    assert_eq!(store.covered(4, target.range, NOW).unwrap(), vec![prefix]);
    let later = NOW + COLLECTION_SECONDS;
    assert!(matches!(
        next(&mut store, &owner.name, &acquisition, later).unwrap(),
        Planned::Complete
    ));
    acquisition.focus = Some(prefix.end - plan::PROGRAM_PADDING_SECONDS);
    let Planned::Ready(tail) = next(&mut store, &owner.name, &acquisition, later).unwrap() else {
        panic!("near tail")
    };
    assert_eq!(
        tail.range,
        Interval::new(prefix.end, archive_end(later)).unwrap()
    );
    acquisition.focus = Some(START);
    let ended = target.range.end + ARCHIVE_DELAY_SECONDS + COLLECTION_SECONDS;
    let Planned::Ready(tail) = next(&mut store, &owner.name, &acquisition, ended).unwrap() else {
        panic!("completed program tail")
    };
    assert_eq!(
        tail.range,
        Interval::new(prefix.end, target.range.end).unwrap()
    );
}
