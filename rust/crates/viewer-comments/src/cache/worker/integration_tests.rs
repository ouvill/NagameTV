use super::*;
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
        source_range: Source::Recording(RecordingRange::Known(vec![ClockSpan {
            key,
            channel: 1,
            media_start_ms: 0,
            media_end_ms: 60_000,
            utc_start_ms: utc * 1000,
        }])),
    }
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
    let Source::Recording(range) = next.source_range else {
        unreachable!()
    };
    let live = Comment {
        identity: None,
        source_id: Some((1, 1)),
        text: "live while downloading".into(),
        origin: Origin::Nx,
        phase: Phase::Live,
        unix_seconds: 200005,
        timestamp_micros: Some(200005_000_000),
        style: Default::default(),
    };
    controller
        .receive(2, 1, Some(range.spans()[0].clone()), vec![(live, true)])
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
    let db = rusqlite::Connection::open(dir.path().join("cache.sqlite3")).unwrap();
    while db
        .query_row("SELECT count(*) FROM coverage WHERE published=1", [], |r| {
            r.get::<_, i64>(0)
        })
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
