//! Hardware-free HTTP transport faults, bounded prefetch and virtual retry time.
use super::*;
use std::{
    io::{BufRead, BufReader, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{Mutex, atomic::AtomicUsize},
    thread::{self, JoinHandle},
    time::Instant,
};

struct Request {
    start: usize,
    end: usize,
    if_range: bool,
}
enum Reply {
    Data,
    Drop,
    Partial,
    Status(u16),
    Changed,
    Short,
}
struct Server {
    address: SocketAddr,
    stopped: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    connections: Arc<AtomicUsize>,
    requests: Arc<Mutex<Vec<(usize, usize, usize)>>>,
}
impl Server {
    fn new(size: usize, handler: impl Fn(&Request) -> Reply + Send + Sync + 'static) -> Self {
        Self::bind(TcpListener::bind("127.0.0.1:0").unwrap(), size, handler)
    }
    fn bind(
        listener: TcpListener,
        size: usize,
        handler: impl Fn(&Request) -> Reply + Send + Sync + 'static,
    ) -> Self {
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let stopped = Arc::new(AtomicBool::new(false));
        let stop = stopped.clone();
        let connections = Arc::new(AtomicUsize::new(0));
        let count = connections.clone();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = requests.clone();
        let handler = Arc::new(handler);
        let worker = thread::spawn(move || {
            let mut clients = Vec::new();
            while !stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((socket, _)) => {
                        let connection = count.fetch_add(1, Ordering::AcqRel);
                        let stop = stop.clone();
                        let handler = handler.clone();
                        let seen = seen.clone();
                        clients.push(thread::spawn(move || {
                            serve(socket, size, connection, &stop, &seen, handler.as_ref())
                        }));
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(error) => panic!("accept: {error}"),
                }
            }
            for client in clients {
                client.join().unwrap();
            }
        });
        Self {
            address,
            stopped,
            worker: Some(worker),
            connections,
            requests,
        }
    }
    fn url(&self) -> Url {
        Url::parse(&format!(
            "http://{}/recording?secret=do-not-log",
            self.address
        ))
        .unwrap()
    }
    fn inspect(&self) -> Cursor {
        Verified::inspect(self.url(), Arc::new(AtomicBool::new(false))).unwrap()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        self.worker.take().unwrap().join().unwrap();
    }
}
fn serve(
    socket: TcpStream,
    size: usize,
    connection: usize,
    stopped: &AtomicBool,
    seen: &Mutex<Vec<(usize, usize, usize)>>,
    handler: &dyn Fn(&Request) -> Reply,
) {
    socket
        .set_read_timeout(Some(Duration::from_millis(50)))
        .unwrap();
    socket
        .set_write_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut socket = BufReader::new(socket);
    let mut last = Instant::now();
    loop {
        if stopped.load(Ordering::Acquire) {
            return;
        }
        let mut line = String::new();
        match socket.read_line(&mut line) {
            Ok(0) => return,
            Ok(_) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                if last.elapsed() >= Duration::from_secs(3) {
                    return;
                }
                continue;
            }
            Err(error) if error.kind() == io::ErrorKind::ConnectionReset => return,
            Err(error) => panic!("read request: {error}"),
        }
        assert!(line.starts_with("GET "));
        let mut range = None;
        let mut if_range = false;
        loop {
            line.clear();
            socket.read_line(&mut line).unwrap();
            if line == "\r\n" {
                break;
            }
            if let Some(value) = line.to_ascii_lowercase().strip_prefix("range: bytes=") {
                let (start, end) = value.trim().split_once('-').unwrap();
                range = Some((
                    start.parse().unwrap(),
                    if end.is_empty() {
                        size - 1
                    } else {
                        end.parse().unwrap()
                    },
                ));
            }
            if line.to_ascii_lowercase().starts_with("if-range:") {
                assert!(line.contains("\"v1\""));
                if_range = true;
            }
        }
        let (start, end) = range.unwrap();
        let request = Request {
            start,
            end,
            if_range,
        };
        seen.lock().unwrap().push((start, end, connection));
        let reply = handler(&request);
        if matches!(reply, Reply::Drop) {
            return;
        }
        let wire = socket.get_mut();
        let result = (|| -> io::Result<()> {
            if let Reply::Status(status) = reply {
                write!(
                    wire,
                    "HTTP/1.1 {status} Failure\r\nContent-Length: 0\r\n\r\n"
                )?;
            } else {
                assert!(end < size);
                let bytes: Vec<_> = (start..=end).map(|offset| (offset % 251) as u8).collect();
                let etag = if matches!(reply, Reply::Changed) {
                    "v2"
                } else {
                    "v1"
                };
                write!(
                    wire,
                    "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{end}/{size}\r\nETag: \"{etag}\"\r\n"
                )?;
                if matches!(reply, Reply::Short) {
                    wire.write_all(b"Connection: close\r\n\r\n")?;
                    wire.write_all(&bytes[..bytes.len().min(10)])?;
                } else {
                    write!(
                        wire,
                        "Content-Length: {}\r\nKeep-Alive: timeout=3\r\n\r\n",
                        bytes.len()
                    )?;
                    let count = if matches!(reply, Reply::Partial) {
                        bytes.len().min(RANGE_BYTES / 2)
                    } else {
                        bytes.len()
                    };
                    wire.write_all(&bytes[..count])?;
                }
            }
            wire.flush()
        })();
        if let Err(error) = result {
            // Seek and cancellation intentionally close an active connection.
            assert!(
                matches!(
                    error.kind(),
                    io::ErrorKind::BrokenPipe
                        | io::ErrorKind::ConnectionReset
                        | io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                ),
                "{error}"
            );
            return;
        }
        if matches!(reply, Reply::Partial | Reply::Short) {
            return;
        }
        last = Instant::now();
    }
}
fn wait(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(4);
    while !condition() {
        assert!(Instant::now() < deadline, "condition timed out");
        thread::sleep(Duration::from_millis(5));
    }
}
fn pattern(start: usize, bytes: &[u8]) {
    assert!(
        bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| *byte == ((start + index) % 251) as u8)
    );
}

#[tokio::test(start_paused = true)]
async fn retries_use_fresh_connections_and_stop_at_the_total_deadline() {
    let start = tokio::time::Instant::now();
    let calls = Mutex::new(Vec::new());
    let episodes = AtomicUsize::new(0);
    let result = recover::<(), _, _>(
        |fresh| {
            calls
                .lock()
                .unwrap()
                .push((start.elapsed().as_secs(), fresh));
            async { Err(Error::Status(StatusCode::SERVICE_UNAVAILABLE)) }
        },
        &|_| {
            episodes.fetch_add(1, Ordering::AcqRel);
        },
    )
    .await;
    assert!(matches!(result, Err(Error::RetryExpired(_))));
    assert_eq!(
        *calls.lock().unwrap(),
        [
            (0, false),
            (0, true),
            (1, true),
            (3, true),
            (7, true),
            (15, true),
            (23, true)
        ]
    );
    assert_eq!(start.elapsed(), RETRY_DEADLINE);
    assert_eq!(episodes.load(Ordering::Acquire), 1);
}

#[tokio::test(start_paused = true)]
async fn total_deadline_cuts_off_an_inflight_request() {
    let start = tokio::time::Instant::now();
    let result = recover::<(), _, _>(
        |_| async {
            tokio::time::sleep(RETRY_DEADLINE * 2).await;
            Ok(())
        },
        &|_| panic!("no completed failure yet"),
    )
    .await;
    assert!(matches!(result, Err(Error::Deadline)));
    assert_eq!(start.elapsed(), RETRY_DEADLINE);
}

#[test]
fn a_continuous_get_survives_a_pause_longer_than_the_former_request_timeout() {
    let server = Server::new(RANGE_BYTES * 12, |_| Reply::Data);
    let mut cursor = server.inspect();
    let control = cursor.prefetch();
    cursor.read_exact(&mut [0; 8]).unwrap();
    wait(|| {
        control
            .buffered()
            .is_some_and(|(bytes, budget)| bytes + RANGE_BYTES > budget)
    });
    let requests = server.requests.lock().unwrap().len();
    thread::sleep(REQUEST_TIMEOUT + Duration::from_millis(200));
    assert_eq!(server.requests.lock().unwrap().len(), requests);
    let mut bytes = vec![0; RANGE_BYTES * 12 - 8];
    cursor.read_exact(&mut bytes).unwrap();
    pattern(8, &bytes);
    assert_eq!(server.requests.lock().unwrap().len(), requests);
    assert_eq!(requests, 2, "one size probe and one continuous GET");
    assert_eq!(server.connections.load(Ordering::Acquire), 2);
}

#[test]
fn dropped_headers_partial_body_and_503_recover_without_splicing_bytes() {
    for fault in [0, 1, 2] {
        let failures = Arc::new(AtomicUsize::new(0));
        let attempts = failures.clone();
        let server = Server::new(RANGE_BYTES * 4, move |request| {
            if request.end > 1 && attempts.fetch_add(1, Ordering::AcqRel) == 0 {
                assert!(request.if_range);
                match fault {
                    0 => Reply::Drop,
                    1 => Reply::Partial,
                    _ => Reply::Status(503),
                }
            } else {
                Reply::Data
            }
        });
        let mut cursor = server.inspect();
        let (_, log) = crate::logging::tests::capture_output("warn", || {
            let mut bytes = vec![0; RANGE_BYTES];
            cursor.read_exact(&mut bytes).unwrap();
            pattern(0, &bytes);
        });
        assert_eq!(log.matches("Retrying recording HTTP range").count(), 1);
        assert!(!log.contains("secret"));
        assert!(!log.contains("http://"));
        let requests = server.requests.lock().unwrap();
        let retry = requests.last().unwrap();
        assert_eq!((retry.0, retry.1), (requests[1].0, requests[1].1));
        assert_ne!(requests[0].2, retry.2, "retry reused the failed connection");
    }
}

#[test]
fn permanent_statuses_changed_validator_and_clean_short_body_are_not_retried() {
    for fault in [0, 1, 2, 3, 4] {
        let server = Server::new(RANGE_BYTES * 4, move |request| {
            if request.end <= 1 {
                return Reply::Data;
            }
            match fault {
                0 => Reply::Status(401),
                1 => Reply::Status(403),
                2 => Reply::Status(404),
                3 => Reply::Changed,
                _ => Reply::Short,
            }
        });
        let mut cursor = server.inspect();
        assert!(cursor.read_exact(&mut [0; 8]).is_err());
        assert_eq!(server.requests.lock().unwrap().len(), 2);
    }
}

#[test]
fn temporarily_refused_connection_recovers_on_a_fresh_attempt() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let (sender, receiver) = std::sync::mpsc::channel();
    let setup = thread::spawn(move || {
        thread::sleep(Duration::from_millis(150));
        sender
            .send(Server::bind(
                TcpListener::bind(address).unwrap(),
                100,
                |_| Reply::Data,
            ))
            .unwrap_or_else(|_| panic!("server receiver closed"));
    });
    let began = Instant::now();
    let cursor = Verified::inspect(
        Url::parse(&format!("http://{address}/recording")).unwrap(),
        Arc::new(AtomicBool::new(false)),
    )
    .unwrap();
    assert_eq!(cursor.source.size(), 100);
    assert!(began.elapsed() >= Duration::from_secs(1));
    let _server = receiver.recv().unwrap();
    setup.join().unwrap();
}

#[test]
fn buffered_reads_continue_while_retrying_and_resume_the_same_position() {
    let recover = Arc::new(AtomicBool::new(false));
    let available = recover.clone();
    let partial = Arc::new(AtomicBool::new(false));
    let sent = partial.clone();
    let server = Server::new(RANGE_BYTES * 8, move |request| {
        if request.end <= 1 {
            return Reply::Data;
        }
        if !sent.swap(true, Ordering::AcqRel) {
            return Reply::Partial;
        }
        if !available.load(Ordering::Acquire) {
            Reply::Status(503)
        } else {
            Reply::Data
        }
    });
    let mut cursor = server.inspect();
    let control = cursor.prefetch();
    let mut first = [0; 4096];
    cursor.read_exact(&mut first).unwrap();
    pattern(0, &first);
    wait(|| control.reconnecting());
    let began = Instant::now();
    let mut rest = vec![0; RANGE_BYTES / 2 - first.len()];
    cursor.read_exact(&mut rest).unwrap();
    pattern(first.len(), &rest);
    assert!(began.elapsed() < Duration::from_millis(200));
    recover.store(true, Ordering::Release);
    let mut next = vec![0; RANGE_BYTES];
    cursor.read_exact(&mut next).unwrap();
    pattern(RANGE_BYTES / 2, &next);
    wait(|| !control.reconnecting());
    let requests = server.requests.lock().unwrap();
    assert!(
        requests
            .iter()
            .skip(2)
            .all(|request| request.0 == RANGE_BYTES / 2)
    );
}

#[test]
fn seek_cancels_old_prefetch_without_waiting_for_its_body() {
    let blocked = Arc::new(AtomicBool::new(false));
    let started = blocked.clone();
    let release = Arc::new(AtomicBool::new(false));
    let released = release.clone();
    let server = Server::new(RANGE_BYTES * 12, move |request| {
        if request.start == 0 && request.end > 1 {
            started.store(true, Ordering::Release);
            while !released.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(5));
            }
        }
        Reply::Data
    });
    let mut cursor = server.inspect();
    let control = cursor.prefetch();
    let worker = thread::spawn(move || {
        let result = cursor.read_exact(&mut [0; 8]);
        (result, cursor)
    });
    wait(|| blocked.load(Ordering::Acquire));
    let began = Instant::now();
    control.interrupt();
    let (result, mut cursor) = worker.join().unwrap();
    assert!(result.is_err());
    cursor
        .seek(SeekFrom::Start((RANGE_BYTES * 6) as u64))
        .unwrap();
    let mut bytes = [0; 4096];
    cursor.read_exact(&mut bytes).unwrap();
    pattern(RANGE_BYTES * 6, &bytes);
    release.store(true, Ordering::Release);
    assert!(began.elapsed() < Duration::from_millis(300));
}

#[test]
fn cancellation_stops_backoff_and_releases_the_prefetch_window() {
    let server = Server::new(RANGE_BYTES * 4, |request| {
        if request.end <= 1 {
            Reply::Data
        } else {
            Reply::Status(503)
        }
    });
    let mut cursor = server.inspect();
    let cancelled = cursor.cancelled.clone();
    let control = cursor.prefetch();
    let observed = control.clone();
    let worker = thread::spawn(move || {
        let result = cursor.read_exact(&mut [0; 8]);
        drop(cursor);
        result
    });
    wait(|| observed.reconnecting() && server.requests.lock().unwrap().len() >= 3);
    let began = Instant::now();
    cancelled.store(true, Ordering::Release);
    assert!(worker.join().unwrap().is_err());
    assert!(began.elapsed() < Duration::from_millis(300));
    assert_eq!(control.buffered(), None);
}

#[test]
fn read_ahead_is_limited_by_media_time_and_bytes_and_stops_when_full() {
    const SIZE: usize = 100 * 1024 * 1024;
    let server = Server::new(SIZE, |_| Reply::Data);
    for duration in [800, 40, 10] {
        let mut cursor = server.inspect();
        let control = cursor.prefetch();
        control.duration(Duration::from_secs(duration));
        cursor.read_exact(&mut [0; 1]).unwrap();
        wait(|| {
            control
                .buffered()
                .is_some_and(|(bytes, budget)| bytes + RANGE_BYTES > budget)
        });
        let (bytes, budget) = control.buffered().unwrap();
        assert!(budget <= 32 * 1024 * 1024);
        let estimated = bytes as f64 * duration as f64 / SIZE as f64;
        if duration >= 40 {
            assert!(estimated >= 10.0 - RANGE_BYTES as f64 * duration as f64 / SIZE as f64);
            assert!(estimated < 10.0 + RANGE_BYTES as f64 * duration as f64 / SIZE as f64);
        } else {
            assert_eq!(budget, 32 * 1024 * 1024);
        }
        let count = server.requests.lock().unwrap().len();
        thread::sleep(Duration::from_millis(50));
        assert_eq!(
            server.requests.lock().unwrap().len(),
            count,
            "full buffer kept growing"
        );
        drop(cursor);
        assert_eq!(control.buffered(), None);
    }
}

#[test]
fn a_validator_change_during_recovery_fails_instead_of_resuming_a_different_file() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let calls = attempts.clone();
    let server = Server::new(RANGE_BYTES * 4, move |request| {
        if request.end <= 1 {
            return Reply::Data;
        }
        if calls.fetch_add(1, Ordering::AcqRel) == 0 {
            Reply::Status(503)
        } else {
            Reply::Changed
        }
    });
    let mut cursor = server.inspect();
    let control = cursor.prefetch();
    assert!(cursor.read_exact(&mut [0; 8]).is_err());
    assert!(!control.reconnecting());
    assert_eq!(attempts.load(Ordering::Acquire), 2);
}

#[test]
fn seeks_inside_the_window_reuse_bytes_and_the_final_byte_uses_a_valid_range() {
    let server = Server::new(RANGE_BYTES * 8, |_| Reply::Data);
    let mut cursor = server.inspect();
    let control = cursor.prefetch();
    cursor.read_exact(&mut [0; 8]).unwrap();
    wait(|| {
        control
            .buffered()
            .is_some_and(|(bytes, budget)| bytes + RANGE_BYTES > budget)
    });
    let requests = server.requests.lock().unwrap().len();
    cursor.seek(SeekFrom::Start(100)).unwrap();
    let mut bytes = [0; 8];
    cursor.read_exact(&mut bytes).unwrap();
    pattern(100, &bytes);
    thread::sleep(Duration::from_millis(50));
    assert_eq!(server.requests.lock().unwrap().len(), requests);
    cursor.seek(SeekFrom::End(-1)).unwrap();
    let mut final_byte = [0];
    cursor.read_exact(&mut final_byte).unwrap();
    pattern(RANGE_BYTES * 8 - 1, &final_byte);
    assert_eq!(cursor.read(&mut final_byte).unwrap(), 0);
    let requests = server.requests.lock().unwrap();
    assert_eq!(requests.last().unwrap().0, RANGE_BYTES * 8 - 2);
    assert_eq!(requests.last().unwrap().1, RANGE_BYTES * 8 - 1);
}

#[test]
fn confirming_the_current_offset_does_not_restart_a_healthy_continuous_get() {
    let server = Server::new(RANGE_BYTES * 8, |_| Reply::Data);
    let mut cursor = server.inspect();
    cursor.prefetch();
    let mut offset = 0;
    let mut bytes = [0; 8192];
    while offset < RANGE_BYTES * 8 {
        cursor.seek(SeekFrom::Start(offset as u64)).unwrap();
        let count = cursor.read(&mut bytes).unwrap();
        assert!(count > 0);
        pattern(offset, &bytes[..count]);
        offset += count;
    }
    assert_eq!(server.requests.lock().unwrap().len(), 2);
}
