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
fn navigation_uses_current_snapshot_without_a_browser_and_tracks_program_boundaries()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::channels::Step::{Next, Previous};
    let channels = crate::channels::parse(br#"[
        {"id":1,"name":"A","type":1,"networkId":10,"serviceId":1,"channel":{"type":"GR","channel":"27"}},
        {"id":2,"name":"B","type":1,"networkId":10,"serviceId":2,"channel":{"type":"GR","channel":"27"}},
        {"id":3,"name":"C","type":1,"networkId":10,"serviceId":3,"channel":{"type":"GR","channel":"28"}}
    ]"#)?;
    let feature = ProgramInfo {
        snapshot: parse(br#"[
            {"id":1,"eventId":7,"networkId":10,"serviceId":1,"name":"A","startAt":100,"duration":200},
            {"id":2,"eventId":7,"networkId":10,"serviceId":2,"name":"A","startAt":100,"duration":200},
            {"id":3,"eventId":8,"networkId":10,"serviceId":2,"name":"B","startAt":200,"duration":100}
        ]"#)?,
        ..ProgramInfo::default()
    };
    assert_eq!(feature.visible_channels(&channels, Some(199)), vec![0, 2]);
    assert_eq!(
        feature.visible_channels(&channels, Some(200)),
        vec![0, 1, 2]
    );
    assert_eq!(
        feature.adjacent_channel(&channels, Some(0), Next, Some(199)),
        Some(2)
    );
    assert_eq!(
        feature.adjacent_channel(&channels, Some(0), Next, Some(200)),
        Some(1)
    );
    assert_eq!(
        feature.adjacent_channel(&channels, Some(2), Previous, Some(200)),
        Some(1)
    );
    assert_eq!(
        feature.adjacent_channel(&channels, Some(1), Previous, Some(300)),
        Some(0)
    );
    assert_eq!(
        feature.adjacent_channel(&channels, Some(0), Next, None),
        Some(2)
    );
    assert_eq!(feature.counters(), (0, 3, false));
    Ok(())
}

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
    assert_eq!(
        feature.poll(&network),
        Update {
            completed: None,
            started: true
        }
    );
    assert!(matches!(feature.status(), Status::Fetching));
    // Updates during acquisition must coalesce into exactly one follow-up request.
    for _ in 0..100 {
        feature.refresh();
    }
    for revision in [2, 3] {
        let deadline = Instant::now() + Duration::from_secs(3);
        while feature.revision < revision && Instant::now() < deadline {
            let update = feature.poll(&network);
            if feature.revision == revision {
                assert_eq!(update.completed, Some(Completion::Succeeded));
                assert_eq!(update.started, revision == 2);
            }
            thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(feature.revision, revision);
        assert_eq!(feature.counters().1, 1);
        assert!(feature.text_capacity_bytes >= "番組".len());
        assert!(
            feature
                .view(
                    key(32096, 1024),
                    guide::DayWindow::new(100.0, 100.0 + 86_400_000.0)?
                )?
                .contains("番組")
        );
        if revision == 3 {
            feature.refresh();
        }
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    while !matches!(feature.status(), Status::Failed(_)) {
        assert!(Instant::now() < deadline);
        let update = feature.poll(&network);
        if matches!(feature.status(), Status::Failed(_)) {
            assert_eq!(
                update,
                Update {
                    completed: Some(Completion::Failed),
                    started: false
                }
            );
        }
        thread::sleep(Duration::from_millis(1));
    }
    assert!(matches!(
        feature.status(),
        Status::Failed(FetchError::Parse(Error::Json(_)))
    ));
    assert_eq!(feature.revision, 3);
    assert!(feature.text_capacity_bytes >= "番組".len());
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
    assert_eq!(feature.text_capacity_bytes, 0);
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
    assert_eq!(feature.text_capacity_bytes, 0);
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
    let channels =
        crate::channels::parse(br#"[{"id":42,"name":"A","type":1,"networkId":3,"serviceId":20}]"#)?;
    let grid: serde_json::Value = serde_json::from_str(
        &snapshot.grid_view(&channels, guide::DayWindow::new(10000.0, 260000.0)?)?,
    )?;
    let cells = grid[0]["programs"]
        .as_array()
        .ok_or("grid programs missing")?;
    assert_eq!(cells.len(), 251);
    for (cell, program) in cells.iter().zip(&view) {
        assert_eq!(cell["id"], program["id"]);
        let key: serde_json::Value =
            serde_json::from_str(cell["watchKey"].as_str().ok_or("watch key missing")?)?;
        assert_eq!(key["endpoint"], 42);
        assert_eq!(key["program"], program["id"]);
    }
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

#[test]
fn audio_metadata_tracks_program_boundaries_disable_and_detail_payloads()
-> Result<(), Box<dyn std::error::Error>> {
    let mut feature = ProgramInfo {
        desired: Some("http://example.test".into()),
        snapshot: parse(
            br#"[
            {"id":1,"networkId":10,"serviceId":1,"startAt":100,"duration":100,
             "audios":[{"componentTag":16,"componentType":3,"isMain":true,"langs":["jpn"]}]},
            {"id":2,"networkId":10,"serviceId":1,"startAt":200,"duration":100,
             "audios":[{"componentTag":16,"componentType":3,"isMain":false,"langs":["eng"]}]}
        ]"#,
        )?,
        ..ProgramInfo::default()
    };
    let service = Some(BroadcastService {
        network_id: 10,
        service_id: 1,
    });
    let role = |feature: &ProgramInfo, now| {
        crate::audio::matching(
            feature.current(service, now).map_or(&[], |p| &p.audios),
            Some(16),
        )
        .map(crate::audio::Descriptor::role)
    };
    assert_eq!(role(&feature, 99), None);
    assert_eq!(role(&feature, 199), Some(crate::audio::Role::Main));
    assert_eq!(role(&feature, 200), Some(crate::audio::Role::Sub));
    assert_eq!(role(&feature, 300), None);
    assert!(feature.current(None, 100).is_none());
    assert!(feature.current(key(11, 1), 100).is_none());
    let guide = feature.view(service, guide::DayWindow::new(0.0, 86_400_000.0)?)?;
    let guide: serde_json::Value = serde_json::from_str(&guide)?;
    assert_eq!(guide[0]["audios"][0]["langs"][0], "jpn");
    assert_eq!(guide[1]["audios"][0]["isMain"], false);
    feature.configure(None);
    assert_eq!(role(&feature, 199), None);
    Ok(())
}

#[test]
fn epg_details_keep_order_optional_fields_and_unknown_formats_in_grid()
-> Result<(), Box<dyn std::error::Error>> {
    let snapshot = parse(r#"[
        {"id":1,"networkId":10,"serviceId":1,"startAt":100,"duration":100,
         "name":"番組名","description":"短い概要","isFree":false,
         "genres":[{"lv1":3,"lv2":0}],
         "extended":{"番組内容":"本文\n次の行","番組内容2":"続き","出演者":"<b>出演者</b>","スタッフ":"スタッフ名","独自の見出し":"末尾"},
         "video":{"type":"mpeg2","resolution":"1080i","streamContent":1,"componentType":179},
         "audios":[{"componentTag":16,"componentType":3,"isMain":true,"langs":["jpn"],"samplingRate":48000},
                   {"componentTag":17,"componentType":2,"isMain":false,"langs":["jpn","eng"],"samplingRate":-1}],
         "series":{"name":"シリーズ名","episode":3,"lastEpisode":12}},
        {"id":2,"networkId":10,"serviceId":1,"startAt":200,"duration":100},
        {"id":3,"networkId":10,"serviceId":1,"startAt":300,"duration":100,
         "extended":null,"video":{"type":"future-codec","resolution":"future-resolution"},"series":null,"isFree":null}
    ]"#.as_bytes())?;
    let channels =
        crate::channels::parse(br#"[{"id":42,"name":"A","type":1,"networkId":10,"serviceId":1}]"#)?;
    let grid: serde_json::Value = serde_json::from_str(
        &snapshot.grid_view(&channels, guide::DayWindow::new(0.0, 86_400_000.0)?)?,
    )?;
    let program = &grid[0]["programs"][0];
    let sections = program["extended"].as_array().ok_or("missing sections")?;
    let headings: Vec<_> = sections
        .iter()
        .map(|section| section["heading"].as_str())
        .collect();
    assert_eq!(
        headings,
        [
            Some("番組内容"),
            Some("番組内容2"),
            Some("出演者"),
            Some("スタッフ"),
            Some("独自の見出し")
        ]
    );
    assert_eq!(sections[0]["text"], "本文\n次の行");
    assert_eq!(sections[2]["text"], "<b>出演者</b>");
    assert_eq!(program["video"]["type"], "mpeg2");
    assert_eq!(program["video"]["resolution"], "1080i");
    assert_eq!(program["audios"][0]["samplingRate"], 48000);
    assert_eq!(program["audios"][1]["samplingRate"], -1);
    assert_eq!(program["series"]["episode"], 3);
    assert_eq!(program["isFree"], false);
    assert_eq!(program["genre"], 3);
    let missing = &grid[0]["programs"][1];
    for field in ["extended", "video", "series", "isFree"] {
        assert!(missing.get(field).is_none());
    }
    assert_eq!(missing["audios"], serde_json::json!([]));
    assert_eq!(grid[0]["programs"][2]["video"]["type"], "future-codec");
    assert!(grid[0]["programs"][2].get("extended").is_none());
    let storage = snapshot.storage();
    assert_eq!(
        storage.details,
        sections.len() * std::mem::size_of::<super::details::Section>()
    );
    assert!(storage.strings >= "番組名短い概要番組内容本文\n次の行番組内容2続き出演者<b>出演者</b>スタッフスタッフ名独自の見出し末尾mpeg21080iシリーズ名future-codecfuture-resolution".len());
    Ok(())
}

#[test]
fn grid_columns_use_explicit_services_and_only_overlap_the_requested_day()
-> Result<(), Box<dyn std::error::Error>> {
    let channels = crate::channels::parse(
        br#"[
        {"id":777,"name":"A","type":1,"networkId":4,"serviceId":42},
        {"id":888,"name":"B","type":1,"networkId":5,"serviceId":42},
        {"id":999,"name":"Unknown","type":1}
    ]"#,
    )?;
    let snapshot = parse(
        br#"[
        {"id":1,"networkId":4,"serviceId":42,"name":"Crossing","genres":[{"lv1":1,"lv2":0}],"startAt":50,"duration":100},
        {"id":2,"networkId":5,"serviceId":42,"name":"Other network","startAt":100,"duration":100},
        {"id":3,"networkId":4,"serviceId":42,"name":"Already ended","startAt":0,"duration":100},
        {"id":4,"networkId":4,"serviceId":42,"name":"Next day","startAt":200,"duration":100}
    ]"#,
    )?;
    let data: serde_json::Value = serde_json::from_str(
        &snapshot.grid_view(&channels, guide::DayWindow::new(100.0, 200.0)?)?,
    )?;
    for (index, channel) in channels.iter().enumerate() {
        assert_eq!(data[index]["index"], index);
        let programs = data[index]["programs"]
            .as_array()
            .ok_or("missing programs")?;
        match channel.id {
            777 => {
                assert_eq!(programs.len(), 1);
                assert_eq!(programs[0]["id"], 1);
                assert_eq!(programs[0]["genre"], 1);
            }
            888 => {
                assert_eq!(programs.len(), 1);
                assert_eq!(programs[0]["id"], 2);
                assert_eq!(programs[0]["genre"], 15);
            }
            _ => assert!(programs.is_empty()),
        }
    }
    assert_eq!(
        snapshot.grid_view(&[], guide::DayWindow::new(100.0, 200.0)?)?,
        "[]"
    );
    Ok(())
}

#[test]
fn watch_revalidates_opaque_ids_current_time_and_reordered_channels()
-> Result<(), Box<dyn std::error::Error>> {
    let mut channels = crate::channels::parse(
        br#"[
        {"id":18446744073709551615,"name":"A","type":1,"networkId":4,"serviceId":42},
        {"id":123,"name":"B","type":1,"networkId":5,"serviceId":42}
    ]"#,
    )?;
    let mut feature = ProgramInfo {
        snapshot: parse(
            br#"[
            {"id":18446744073709551615,"networkId":4,"serviceId":42,"startAt":100,"duration":100}
        ]"#,
        )?,
        ..ProgramInfo::default()
    };
    let data: serde_json::Value =
        serde_json::from_str(&feature.grid_view(&channels, guide::DayWindow::new(100.0, 200.0)?)?)?;
    let column = channels
        .iter()
        .position(|c| c.id == u64::MAX)
        .ok_or("missing channel")?;
    let key = data[column]["programs"][0]["watchKey"]
        .as_str()
        .ok_or("missing key")?;
    assert!(key.contains("18446744073709551615"));
    assert_eq!(feature.watch_channel(key, &channels, 100)?, column);
    channels.reverse();
    let reordered = feature.watch_channel(key, &channels, 199)?;
    assert_eq!(channels[reordered].id, u64::MAX);
    for now in [99, 200] {
        assert!(matches!(
            feature.watch_channel(key, &channels, now),
            Err(watch::Error::NotLive)
        ));
    }
    channels[reordered].id = 456;
    assert!(matches!(
        feature.watch_channel(key, &channels, 150),
        Err(watch::Error::Unavailable)
    ));
    channels[reordered].id = u64::MAX;
    // A broadcaster may revise the slot while retaining the event ID. The old
    // selection must not authorize a different start or duration, even if live.
    for (start, duration) in [(110, 100), (100, 80)] {
        feature.snapshot = parse(
            format!(
                r#"[{{"id":18446744073709551615,"networkId":4,"serviceId":42,"startAt":{start},"duration":{duration}}}]"#
            )
            .as_bytes(),
        )?;
        assert!(matches!(
            feature.watch_channel(key, &channels, 150),
            Err(watch::Error::NotLive)
        ));
    }
    feature.snapshot = parse(
        br#"[
        {"id":2,"networkId":4,"serviceId":42,"startAt":100,"duration":100}
    ]"#,
    )?;
    assert!(matches!(
        feature.watch_channel(key, &channels, 150),
        Err(watch::Error::NotLive)
    ));
    for bad in ["{}".to_owned(), "x".repeat(513)] {
        assert!(matches!(
            feature.watch_channel(&bad, &channels, 150),
            Err(watch::Error::Unavailable)
        ));
    }
    Ok(())
}

#[test]
#[ignore = "manual real-server capture validation; requires NAGAMETV_CAPTURE_DIR"]
fn validates_captured_server_catalog_and_guide() -> Result<(), Box<dyn std::error::Error>> {
    use std::{
        fs::File,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };
    let directory = PathBuf::from(
        std::env::var_os("NAGAMETV_CAPTURE_DIR").ok_or("NAGAMETV_CAPTURE_DIR is required")?,
    );
    let read = |name: &str, limit: usize| -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let mut bytes = Vec::new();
        File::open(directory.join(name))?
            .take(limit as u64 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > limit {
            return Err("capture exceeds application response limit".into());
        }
        Ok(bytes)
    };
    let channels = crate::channels::parse(&read("services.json", 1024 * 1024)?)?;
    let bytes = read("programs.json", MAX_RESPONSE)?;
    let snapshot = parse(&bytes)?;
    let now = u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())?;
    const DAY: u64 = 86_400_000;
    const JST: u64 = 9 * 3_600_000;
    let start = (now.checked_add(JST).ok_or("time overflow")? / DAY * DAY)
        .checked_sub(JST)
        .ok_or("time precedes supported calendar")?;
    let window = guide::DayWindow::new(
        start as f64,
        start.checked_add(DAY).ok_or("day overflow")? as f64,
    )?;
    let storage = snapshot.storage();
    eprintln!(
        "CAPTURE_STORAGE records={} strings={} audio={} total={}",
        storage.records,
        storage.strings,
        storage.audio,
        storage.total()
    );
    let json = snapshot.grid_view(&channels, window)?;
    let columns: Vec<serde_json::Value> = serde_json::from_str(&json)?;
    assert_eq!(columns.len(), channels.len());
    let mut cells = 0;
    for (index, column) in columns.iter().enumerate() {
        assert_eq!(column["index"], index);
        let programs = column["programs"]
            .as_array()
            .ok_or("missing program array")?;
        for program in programs {
            let key = program["watchKey"].as_str().ok_or("missing watch key")?;
            let _: watch::Identity = serde_json::from_str(key)?;
        }
        cells += programs.len();
    }
    let current = channels
        .iter()
        .filter(|channel| snapshot.current(channel.broadcast, now).is_some())
        .count();
    eprintln!(
        "CAPTURE channels={} programs={} raw_bytes={} program_string_capacity={} current_channels={} grid_cells={} grid_bytes={}",
        channels.len(),
        snapshot.len(),
        bytes.len(),
        storage.strings,
        current,
        cells,
        json.len()
    );
    Ok(())
}

#[test]
fn epg_enrichment_requires_service_event_and_start_and_keeps_ts_fields() {
    let feature = ProgramInfo { snapshot: parse(br#"[{"id":1,"eventId":7,"networkId":4,"serviceId":101,"startAt":1000,"duration":60000,"name":"EPG title","description":"EPG detail"}]"#).unwrap(), ..Default::default() };
    let ts = serde_json::json!({"eventId":7,"networkId":4,"serviceId":101,"startAt":1000,"name":"TS title","description":"","source":"broadcast_ts"});
    let mut matched = ts.clone();
    feature.supplement_data(&mut matched);
    assert_eq!(matched["name"], "TS title");
    assert_eq!(matched["description"], "EPG detail");
    for (key, value) in [
        ("eventId", 8),
        ("networkId", 5),
        ("serviceId", 102),
        ("startAt", 1001),
    ] {
        let mut mismatched = ts.clone();
        mismatched[key] = value.into();
        feature.supplement_data(&mut mismatched);
        assert_eq!(mismatched["description"], "");
    }
}
