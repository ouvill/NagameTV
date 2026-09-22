use super::*;
use futures_lite::future::block_on;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::mpsc,
};

fn demand(source: u64, utc: i64, fetch: bool) -> Demand {
    let key = format!("clock-{source}");
    Demand {
        source,
        channel: 1,
        view: Some(View {
            clock_key: key.clone(),
            interval: Interval::new(utc, utc + 60).unwrap(),
        }),
        fetch,
        source_range: Source::Recording(Recording::Observed {
            current: Program::new(
                ProgramId {
                    network: 1,
                    transport: 1,
                    service: 1,
                    event: 1,
                },
                utc,
                60,
            ),
            next: None,
            utc_seconds: Some(utc),
            at_start: false,
        }),
    }
}

#[test]
fn cached_seek_is_readable_while_an_archive_writer_holds_the_database() {
    const UTC: i64 = 100_000;
    const READ_DEADLINE: Duration = Duration::from_millis(400);
    const POLL_INTERVAL: Duration = Duration::from_millis(5);
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let mut receiving = spool::Receiving::prepare(
        dir.path(),
        spool::Receipt {
            id: "cached-recording".into(),
            channel: 1,
            range: Interval::new(UTC, UTC + 60).unwrap(),
            fetched: wall(),
            generation: store.generation().unwrap(),
            plan: None,
        },
    )
    .unwrap();
    receiving
        .write(br#"{"packet":[{"chat":{"date":100005,"content":"before seek"}},{"chat":{"date":100035,"content":"after seek"}}]}"#)
        .unwrap();
    let file = receiving
        .finish()
        .unwrap()
        .validate()
        .map_err(|(_, e)| e)
        .unwrap();
    file.import(&mut store, || false).unwrap();
    file.finish().unwrap();
    let mut controller =
        Controller::with_endpoint(dir.path().into(), "http://127.0.0.1:1".into()).unwrap();
    let mut current = demand(1, UTC, false);
    current.view.as_mut().unwrap().interval = Interval::new(UTC, UTC + 10).unwrap();
    controller.configure(Some(current.clone()));
    let deadline = Instant::now() + Duration::from_secs(3);
    while controller.snapshot().records.is_empty() {
        assert!(
            Instant::now() < deadline,
            "initial cache read did not finish"
        );
        thread::sleep(POLL_INTERVAL);
    }

    // WAL permits reading the published cache throughout an import. Neither
    // unchanged pins nor last-used bookkeeping may delay the new position.
    let mut writer = super::super::test_database::open(dir.path().join("cache.sqlite3")).unwrap();
    block_on(sqlx::raw_sql("BEGIN IMMEDIATE").execute(&mut writer)).unwrap();
    let mut seeking = current.clone();
    seeking.view = None;
    controller.configure(Some(seeking));
    let deadline = Instant::now() + READ_DEADLINE;
    while controller.snapshot().view.is_some() {
        assert!(
            Instant::now() < deadline,
            "seek start waited for a database writer"
        );
        thread::sleep(POLL_INTERVAL);
    }
    current.view.as_mut().unwrap().interval = Interval::new(UTC + 30, UTC + 60).unwrap();
    current.fetch = true;
    controller.configure(Some(current.clone()));
    let deadline = Instant::now() + READ_DEADLINE;
    loop {
        let snapshot = controller.snapshot();
        if snapshot.view == current.view {
            assert_eq!(snapshot.records.len(), 1);
            assert_eq!(snapshot.records[0].comment.text.as_ref(), "after seek");
            break;
        }
        assert!(
            Instant::now() < deadline,
            "cached seek waited for a database writer: {:?}",
            snapshot.state
        );
        thread::sleep(POLL_INTERVAL);
    }
    block_on(sqlx::raw_sql("ROLLBACK").execute(&mut writer)).unwrap();
    controller.shutdown();
}
#[test]
fn sent_response_finishes_after_source_change_while_live_saving_continues() {
    let dir = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let (started_tx, started_rx) = mpsc::channel();
    let (finish_tx, finish_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut header = Vec::new();
        while !header.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            socket.read_exact(&mut byte).unwrap();
            header.push(byte[0]);
            assert!(header.len() < 8192);
        }
        started_tx.send(()).unwrap();
        finish_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let body = br#"{"packet":[{"chat":{"date":100005,"content":"old archive"}}]}"#;
        write!(
            socket,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
            body.len()
        )
        .unwrap();
        socket.write_all(body).unwrap();
    });
    let mut controller = Controller::with_endpoint(dir.path().into(), endpoint).unwrap();
    controller.configure(Some(demand(1, 100_000, true)));
    started_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    let next = demand(2, 200_000, false);
    controller.configure(Some(next.clone()));
    let span = ClockSpan {
        key: "clock-2".into(),
        channel: 1,
        media_start_ms: 0,
        media_end_ms: 60_000,
        utc_start_ms: 200_000_000,
    };
    let live = Comment {
        identity: None,
        source_id: Some((1, 1)),
        text: "live while downloading".into(),
        origin: Origin::Nx,
        phase: Phase::Live,
        unix_seconds: 200005,
        timestamp_micros: Some(200_005_000_000),
        style: Default::default(),
    };
    controller
        .receive(2, 1, Some(span), vec![(live, true)])
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let snapshot = controller.snapshot();
        if snapshot.source == Some(2)
            && snapshot
                .records
                .iter()
                .any(|r| r.comment.text.as_ref() == "live while downloading")
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "live persistence blocked by archive HTTP: {:?}",
            snapshot.state
        );
        thread::sleep(Duration::from_millis(5));
    }
    finish_tx.send(()).unwrap();
    server.join().unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut db = super::super::test_database::open(dir.path().join("cache.sqlite3")).unwrap();
    while block_on(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM coverage WHERE published=1")
            .fetch_one(&mut db),
    )
    .unwrap()
        != 1
    {
        assert!(Instant::now() < deadline, "old response was not saved");
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(controller.snapshot().source, Some(2));
    assert!(
        controller
            .snapshot()
            .records
            .iter()
            .all(|r| r.comment.text.as_ref() != "old archive")
    );
    controller.shutdown();
}

#[test]
fn whole_program_uses_one_request_across_playback_seeks_and_restart() {
    const UTC: i64 = 100_000;
    const DURATION: i64 = 114 * 60;
    const DEADLINE: Duration = Duration::from_secs(5);
    let dir = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket.set_read_timeout(Some(DEADLINE)).unwrap();
        let mut header = Vec::new();
        while !header.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            socket.read_exact(&mut byte).unwrap();
            header.push(byte[0]);
        }
        let header = String::from_utf8(header).unwrap();
        assert!(header.contains("starttime=99880"), "{header}");
        assert!(header.contains("endtime=106959"), "{header}");
        let body = br#"{"packet":[{"chat":{"date":100005,"content":"start"}},{"chat":{"date":103005,"content":"middle","nx_jikkyo":1}},{"chat":{"date":106005,"content":"end"}}]}"#;
        write!(
            socket,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
            body.len()
        )
        .unwrap();
        socket.write_all(body).unwrap();
        listener
    });
    let mut current = demand(1, UTC, true);
    let Source::Recording(Recording::Observed {
        current: program, ..
    }) = &mut current.source_range
    else {
        unreachable!()
    };
    *program = Program::new(
        ProgramId {
            network: 1,
            transport: 1,
            service: 1,
            event: 1,
        },
        UTC,
        DURATION,
    );
    let mut controller = Controller::with_endpoint(dir.path().into(), endpoint.clone()).unwrap();
    controller.configure(Some(current.clone()));
    let deadline = Instant::now() + DEADLINE;
    while controller.snapshot().records.is_empty() {
        assert!(
            Instant::now() < deadline,
            "programme was not downloaded: {:?}",
            controller.snapshot().state
        );
        thread::sleep(Duration::from_millis(5));
    }
    let listener = server.join().unwrap();
    let mut timings = Vec::new();
    for offset in [3000, 6000, 0, 6000, 3000, 0, 3000, 6000, 0, 6000] {
        current.view.as_mut().unwrap().interval =
            Interval::new(UTC + offset, UTC + offset + 60).unwrap();
        let started = Instant::now();
        controller.configure(Some(current.clone()));
        loop {
            let snapshot = controller.snapshot();
            if snapshot.view == current.view {
                assert_eq!(snapshot.records.len(), 1);
                break;
            }
            assert!(
                started.elapsed() < DEADLINE,
                "cached seek was not published"
            );
            thread::sleep(Duration::from_millis(1));
        }
        timings.push(started.elapsed());
    }
    timings.sort();
    eprintln!(
        "cached seek p95={:?}, samples={}",
        timings[timings.len() - 1],
        timings.len()
    );
    controller.shutdown();
    let mut reopened = Controller::with_endpoint(dir.path().into(), endpoint).unwrap();
    reopened.configure(Some(current.clone()));
    let deadline = Instant::now() + DEADLINE;
    while reopened.snapshot().records.is_empty() {
        assert!(Instant::now() < deadline, "restart did not reuse the cache");
        thread::sleep(Duration::from_millis(5));
    }
    thread::sleep(SEEK_SETTLE + POLL + POLL);
    let mut db = super::super::test_database::open(dir.path().join("cache.sqlite3")).unwrap();
    assert_eq!(
        block_on(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM requests").fetch_one(&mut db))
            .unwrap(),
        1
    );
    listener.set_nonblocking(true).unwrap();
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    reopened.shutdown();
}
