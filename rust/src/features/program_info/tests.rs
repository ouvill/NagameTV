use super::*;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
};
#[test]
fn updates_replace_failure_retains_refresh_waits_and_disable_clears()
-> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let address = format!("http://{}", listener.local_addr()?);
    let count = Arc::new(AtomicUsize::new(0));
    let requests = count.clone();
    let server = thread::spawn(move || -> std::io::Result<()> {
        let deadline = Instant::now() + Duration::from_secs(5);
        while requests.load(Ordering::SeqCst) < 3 && Instant::now() < deadline {
            if let Ok((mut stream, _)) = listener.accept() {
                stream.set_read_timeout(Some(Duration::from_secs(1)))?;
                let mut input = [0; 4096];
                let n = stream.read(&mut input)?;
                assert!(String::from_utf8_lossy(&input[..n]).starts_with("GET /api/programs "));
                let request = requests.fetch_add(1, Ordering::SeqCst);
                let valid = r#"[{"id":1,"serviceId":1024,"networkId":32096,"startAt":100,"duration":500,"name":"番組"}]"#;
                let body = if request == 2 { "invalid JSON" } else { valid };
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )?;
            } else {
                thread::sleep(Duration::from_millis(2));
            }
        }
        Ok(())
    });
    let network = Network::new()?;
    let mut feature = ProgramInfo::default();
    for _ in 0..20 {
        feature.poll(&network);
    }
    assert_eq!(count.load(Ordering::SeqCst), 0);
    feature.configure(Some(address));
    for revision in [2, 3] {
        let deadline = Instant::now() + Duration::from_secs(3);
        while feature.revision < revision && Instant::now() < deadline {
            feature.poll(&network);
            thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(feature.revision, revision);
        assert_eq!(feature.counters().1, 1);
        assert!(
            feature
                .view(
                    key(32096, 1024),
                    guide::DayWindow::new(100.0, 100.0 + 86_400_000.0)?
                )?
                .contains("番組")
        );
        feature.refresh();
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    while !matches!(feature.status(), Status::Failed(_)) {
        assert!(Instant::now() < deadline);
        feature.poll(&network);
        thread::sleep(Duration::from_millis(1));
    }
    assert!(matches!(
        feature.status(),
        Status::Failed(FetchError::Parse(Error::Json(_)))
    ));
    assert_eq!(feature.revision, 3);
    assert!(
        feature
            .view(
                key(32096, 1024),
                guide::DayWindow::new(100.0, 100.0 + 86_400_000.0)?
            )?
            .contains("番組")
    );
    feature.poll_at(&network, Instant::now() + REFRESH - Duration::from_secs(1));
    assert_eq!(count.load(Ordering::SeqCst), 3);
    assert_eq!(feature.counters().0, 0);
    feature.poll_at(&network, Instant::now() + REFRESH + Duration::from_secs(1));
    assert!(matches!(feature.status(), Status::Fetching));
    feature.configure(None);
    let deadline = Instant::now() + Duration::from_secs(3);
    while !matches!(feature.status(), Status::Disabled) {
        assert!(Instant::now() < deadline);
        feature.poll(&network);
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(feature.counters(), (0, 0, false));
    assert_eq!(
        feature.view(
            key(32096, 1024),
            guide::DayWindow::new(100.0, 100.0 + 86_400_000.0)?
        )?,
        "[]"
    );
    server.join().map_err(|_| "EPG test server panicked")??;
    Ok(())
}
#[test]
fn cancelled_generation_cannot_return_on_same_server() -> Result<(), Box<dyn std::error::Error>> {
    let network = Network::new()?;
    let mut feature = ProgramInfo::default();
    feature.configure(Some("http://127.0.0.1:1".into()));
    feature.poll(&network);
    feature.configure(None);
    feature.configure(Some("http://127.0.0.1:1".into()));
    assert!(matches!(feature.status(), Status::Cancelling));
    let deadline = Instant::now() + Duration::from_secs(2);
    while matches!(&feature.acquisition, Acquisition::Cancelling(job) if !job.is_finished())
        && Instant::now() < deadline
    {
        thread::sleep(Duration::from_millis(1));
    }
    feature.configure(None);
    feature.poll(&network);
    assert_eq!(feature.counters(), (0, 0, false));
    assert!(matches!(feature.status(), Status::Disabled));
    Ok(())
}

fn schedule_fixture() -> Result<Snapshot, Error> {
    parse(
        br#"[
        {"id":3,"serviceId":20,"networkId":3,"startAt":2000,"duration":500,"name":null},
        {"id":2,"serviceId":20,"networkId":3,"startAt":1500,"duration":500,"name":"Second"},
        {"id":1,"serviceId":20,"networkId":3,"startAt":1000,"duration":500,"name":"First"},
        {"id":4,"serviceId":21,"networkId":3,"startAt":1000,"duration":2000,"name":"Other channel"},
        {"id":1,"serviceId":20,"networkId":3,"startAt":1000,"duration":500,"name":"First"}
    ]"#,
    )
}

#[test]
fn current_program_boundaries_gaps_and_missing_metadata() -> Result<(), Box<dyn std::error::Error>>
{
    let snapshot = schedule_fixture()?;
    assert_eq!(snapshot.len(), 4);
    let service = key(3, 20);
    for (now, expected) in [
        (999, None),
        (1000, Some(1)),
        (1499, Some(1)),
        (1500, Some(2)),
        (2000, Some(3)),
        (2500, None),
    ] {
        assert_eq!(snapshot.current(service, now).map(|p| p.id), expected);
    }
    assert!(snapshot.current(key(0, 999), 1000).is_none());
    assert!(snapshot.current(None, 1000).is_none());
    let unnamed = snapshot
        .current(service, 2000)
        .ok_or("unnamed program lost")?;
    assert!(unnamed.name.is_none());
    assert!(unnamed.description.is_none());
    assert_eq!(snapshot.current(key(3, 21), 2000).map(|p| p.id), Some(4));
    let guide: serde_json::Value = serde_json::from_str(&snapshot.view(
        service,
        guide::DayWindow::new(1500.0, 1500.0 + 86_400_000.0)?,
    )?)?;
    assert_eq!(guide.as_array().ok_or("guide must be an array")?.len(), 2);
    assert_eq!(guide[0]["id"], 2);
    assert_eq!(
        snapshot.view(None, guide::DayWindow::new(1000.0, 1000.0 + 86_400_000.0)?)?,
        "[]"
    );
    Ok(())
}

#[test]
fn current_projection_only_serializes_changes_and_handles_clock_reversal()
-> Result<(), Box<dyn std::error::Error>> {
    let snapshot = schedule_fixture()?;
    let mut projection = presentation::Projection::default();
    assert!(projection.is_stale(1, key(3, 20)));
    let first = projection.update(&snapshot, 1, key(3, 20), 1000)?;
    assert!(
        first
            .data
            .ok_or("missing first projection")?
            .contains("First")
    );
    assert_eq!(first.progress, 0.0);
    let tick = projection.update(&snapshot, 1, key(3, 20), 1250)?;
    assert!(tick.data.is_none());
    assert_eq!(tick.progress, 0.5);
    assert!(!projection.is_stale(1, key(3, 20)));
    assert!(
        projection
            .update(&snapshot, 1, key(3, 20), 1500)?
            .data
            .ok_or("boundary not published")?
            .contains("Second")
    );
    assert!(
        projection
            .update(&snapshot, 1, key(3, 20), 1200)?
            .data
            .ok_or("clock reversal not published")?
            .contains("First")
    );
    // A revision may change title/description without changing program identity.
    assert!(
        projection
            .update(&snapshot, 2, key(3, 20), 1200)?
            .data
            .is_some()
    );
    assert!(
        projection
            .update(&snapshot, 2, key(3, 21), 1200)?
            .data
            .ok_or("selection not published")?
            .contains("Other channel")
    );
    let disabled = projection.update(&Snapshot::default(), 3, key(3, 21), 1200)?;
    assert_eq!(disabled.data.as_deref(), Some("null"));
    assert_eq!(disabled.progress, 0.0);
    Ok(())
}

#[test]
fn current_program_zero_duration_overlap_and_overflow_do_not_resurrect_old_events()
-> Result<(), Box<dyn std::error::Error>> {
    let snapshot = parse(
        br#"[
        {"id":1,"serviceId":1,"networkId":1,"startAt":0,"duration":1000},
        {"id":2,"serviceId":1,"networkId":1,"startAt":10,"duration":0},
        {"id":3,"serviceId":1,"networkId":1,"startAt":18446744073709551605,"duration":100}
    ]"#,
    )?;
    assert!(snapshot.current(key(1, 1), 10).is_none());
    let final_program = snapshot
        .current(key(1, 1), u64::MAX - 1)
        .ok_or("saturating endpoint failed")?;
    assert!(final_program.progress(u64::MAX - 1).is_finite());
    assert!(snapshot.current(key(1, 1), u64::MAX).is_none());
    Ok(())
}

fn key(network_id: u16, service_id: u16) -> Option<crate::channels::BroadcastService> {
    Some(crate::channels::BroadcastService {
        network_id,
        service_id,
    })
}

#[test]
fn epg_uses_broadcast_metadata_instead_of_deriving_endpoint_ids()
-> Result<(), Box<dyn std::error::Error>> {
    let channels = crate::channels::parse(
        br#"[
        {"id":42,"name":"Opaque endpoint","type":1,"networkId":3,"serviceId":20},
        {"id":300020,"name":"Missing broadcast metadata","type":1}
    ]"#,
    )?;
    let snapshot = schedule_fixture()?;
    let identified = channels
        .iter()
        .find(|c| c.id == 42)
        .ok_or("missing channel")?;
    assert_eq!(
        snapshot.current(identified.broadcast, 1000).map(|p| p.id),
        Some(1)
    );
    let unknown = channels
        .iter()
        .find(|c| c.id == 300020)
        .ok_or("missing legacy channel")?;
    assert!(unknown.broadcast.is_none());
    assert!(snapshot.current(unknown.broadcast, 1000).is_none());
    Ok(())
}

#[test]
fn guide_day_includes_crossing_programs_and_does_not_truncate_at_200()
-> Result<(), Box<dyn std::error::Error>> {
    let mut entries = (0..250)
        .map(|i| {
            serde_json::json!({
                "id": i+1, "networkId": 3, "serviceId": 20, "startAt": i*1000+10000, "duration":1000
            })
        })
        .collect::<Vec<_>>();
    entries.push(
        serde_json::json!({"id":999,"networkId":3,"serviceId":20,"startAt":9999,"duration":2}),
    );
    let snapshot = parse(&serde_json::to_vec(&entries)?)?;
    let view: Vec<serde_json::Value> = serde_json::from_str(
        &snapshot.view(key(3, 20), guide::DayWindow::new(10000.0, 260000.0)?)?,
    )?;
    assert_eq!(view.len(), 251);
    assert_eq!(view[0]["id"], 999);
    let next: Vec<serde_json::Value> = serde_json::from_str(
        &snapshot.view(key(3, 20), guide::DayWindow::new(260000.0, 261000.0)?)?,
    )?;
    assert!(next.is_empty()); // Exclusive end: an event ending at midnight is not repeated.
    Ok(())
}

#[test]
fn guide_window_rejects_invalid_numbers_and_accepts_dst_days() {
    use guide::DayWindow;
    for hours in [23.0, 24.0, 25.0] {
        assert!(DayWindow::new(1000.0, 1000.0 + hours * 3600000.0).is_ok());
    }
    for (start, end) in [
        (f64::NAN, 100.0),
        (0.0, f64::INFINITY),
        (-1.0, 1.0),
        (0.1, 1.0),
        (1.0, 1.0),
        (2.0, 1.0),
        (0.0, 27.0 * 3600000.0),
        (9e15, 9e15 + 1000.0),
    ] {
        assert!(DayWindow::new(start, end).is_err());
    }
}
