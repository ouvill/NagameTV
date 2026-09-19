use super::*;
use crate::cache::spool::{Receipt, Receiving};

const BASE: i64 = 100_000;
fn span(key: &str, start: i64, end: i64, utc: i64) -> ClockSpan {
    ClockSpan {
        key: key.into(),
        channel: 1,
        media_start_ms: start,
        media_end_ms: end,
        utc_start_ms: utc,
    }
}
fn view(key: &str, start: i64, end: i64) -> View {
    View {
        clock_key: key.into(),
        interval: Interval::new(start, end).unwrap(),
    }
}
fn comment(micros: u64, text: &str, id: u64) -> Comment {
    Comment {
        identity: None,
        text: text.into(),
        origin: Origin::Nx,
        phase: Phase::Live,
        unix_seconds: micros / 1_000_000,
        timestamp_micros: Some(micros),
        source_id: Some((1, id)),
        style: Default::default(),
    }
}
fn import(store: &mut Store, dir: &Path, id: &str, range: Interval, body: &str) {
    let mut rx = Receiving::prepare(
        dir,
        Receipt {
            id: id.into(),
            channel: 1,
            range,
            fetched: BASE + SETTLED_SECONDS * 2,
            generation: store.generation().unwrap(),
            plan: None,
        },
    )
    .unwrap();
    rx.write(body.as_bytes()).unwrap();
    let ready = rx.finish().unwrap().validate().map_err(|(_, e)| e).unwrap();
    ready.import(store, || false).unwrap();
    ready.finish().unwrap();
}
#[test]
fn refresh_splits_existing_coverage_and_keeps_outside_comments_and_empty_success() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let session = store.session().unwrap();
    let range = Interval::new(BASE, BASE + 60).unwrap();
    import(
        &mut store,
        dir.path(),
        "original",
        range,
        r#"{"packet":[{"chat":{"date":100005,"content":"before"}},{"chat":{"date":100025,"content":"keep"}},{"chat":{"date":100055,"content":"after"}}]}"#,
    );
    import(
        &mut store,
        dir.path(),
        "empty",
        Interval::new(BASE + 20, BASE + 40).unwrap(),
        r#"{"packet":[]}"#,
    );
    let records = store
        .read(&session.name, 1, &view("clock", BASE, BASE + 60))
        .unwrap();
    assert_eq!(
        records
            .iter()
            .map(|r| r.comment.text.as_ref())
            .collect::<Vec<_>>(),
        vec!["before", "keep", "after"]
    );
    assert!(
        range
            .missing(store.covered(1, range, BASE + SETTLED_SECONDS * 3).unwrap())
            .is_empty()
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM coverage WHERE published=1", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        3
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM comments", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        3
    );
}
#[test]
fn rate_slots_backoff_and_provider_lock_survive_reopen_and_cache_clear() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let other = Store::open(dir.path()).unwrap();
    let lease = store.try_provider().unwrap().unwrap();
    assert!(other.try_provider().unwrap().is_none());
    let range = Interval::new(100, 200).unwrap();
    for i in 0..6 {
        assert!(matches!(
            store.reserve(&lease, 1, range, BASE + i * 30).unwrap(),
            Reservation::Granted(_)
        ));
    }
    assert!(
        matches!(store.reserve(&lease,2,range,BASE+180).unwrap(),Reservation::Waiting(until) if until==BASE+600)
    );
    store.failed(false, Some(BASE + 3000), BASE + 180).unwrap();
    store.cleanup(BASE + 180, true).unwrap();
    drop(store);
    let mut store = Store::open(dir.path()).unwrap();
    assert!(
        matches!(store.reserve(&lease,1,range,BASE+600).unwrap(),Reservation::Waiting(until) if until>=BASE+3000)
    );
    assert!(matches!(
        store.reserve(&lease, 1, range, BASE - 1).unwrap(),
        Reservation::Waiting(_)
    ));
    drop(lease);
    assert!(other.try_provider().unwrap().is_some());
}
#[test]
fn pending_import_survives_real_sql_failure_and_reuses_download() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let range = Interval::new(BASE, BASE + 60).unwrap();
    let receipt = Receipt {
        id: "retry".into(),
        channel: 1,
        range,
        fetched: BASE + SETTLED_SECONDS * 2,
        generation: 0,
        plan: None,
    };
    let mut rx = Receiving::prepare(dir.path(), receipt).unwrap();
    rx.write(br#"{"packet":[{"chat":{"date":100010,"content":"saved once"}}]}"#)
        .unwrap();
    let ready = rx.finish().unwrap().validate().map_err(|(_, e)| e).unwrap();
    store.db.execute_batch("CREATE TRIGGER write_failure BEFORE INSERT ON comments BEGIN SELECT RAISE(ABORT,'injected disk write failure'); END;").unwrap();
    assert!(matches!(
        ready.import(&mut store, || false),
        Err(Error::Database(_))
    ));
    assert!(
        store
            .covered(1, range, BASE + SETTLED_SECONDS * 2)
            .unwrap()
            .is_empty()
    );
    assert!(dir.path().join("response.json").exists());
    store
        .db
        .execute_batch("DROP TRIGGER write_failure")
        .unwrap();
    ready.import(&mut store, || false).unwrap();
    ready.finish().unwrap();
    assert_eq!(
        store.covered(1, range, BASE + SETTLED_SECONDS * 2).unwrap(),
        vec![range]
    );
}
#[test]
fn protected_recording_survives_quota_and_clear_then_becomes_evictable() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let owner = store.session().unwrap();
    let range = Interval::new(BASE, BASE + 60).unwrap();
    store
        .pin(
            &owner.name,
            &[span("recording", 0, 60_000, BASE * 1000)],
            BASE,
        )
        .unwrap();
    import(
        &mut store,
        dir.path(),
        "recording",
        range,
        r#"{"packet":[]}"#,
    );
    store
        .db
        .execute("UPDATE coverage SET bytes=?1", [DEFAULT_CACHE_BYTES + 1])
        .unwrap();
    store.cleanup(BASE + SETTLED_SECONDS * 3, false).unwrap();
    store.cleanup(BASE + SETTLED_SECONDS * 3, true).unwrap();
    assert_eq!(
        store
            .db
            .query_row(
                "SELECT last_used FROM coverage WHERE published=1",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        BASE + SETTLED_SECONDS * 3,
        "maintenance must record use even when window reads perform no writes"
    );
    assert_eq!(
        store.covered(1, range, BASE + SETTLED_SECONDS * 3).unwrap(),
        vec![range]
    );
    store.release_session(&owner.name).unwrap();
    store.cleanup(BASE + SETTLED_SECONDS * 3, false).unwrap();
    assert!(
        store
            .covered(1, range, BASE + SETTLED_SECONDS * 3)
            .unwrap()
            .is_empty()
    );
}
#[test]
fn live_data_exceeding_memory_limits_remains_seekable_until_ts_expires() {
    const COUNT: u64 = 60_000;
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let owner = store.session().unwrap();
    let clock = span("clock", 0, 600_000, BASE * 1000);
    let text = "x".repeat(512);
    for begin in (0..COUNT).step_by(IMPORT_BATCH) {
        let batch = (begin..(begin + IMPORT_BATCH as u64).min(COUNT))
            .map(|id| {
                (
                    comment(BASE as u64 * 1_000_000 + id * 10_000, &text, id),
                    false,
                )
            })
            .collect::<Vec<_>>();
        store
            .insert_live(&owner.name, 1, Some(&clock), &batch)
            .unwrap();
    }
    store.cleanup(BASE + SETTLED_SECONDS * 3, true).unwrap();
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM live_comments", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        COUNT as i64
    );
    assert!(
        store
            .read(&owner.name, 1, &view("clock", BASE, BASE + 1))
            .unwrap()
            .iter()
            .any(|r| r.comment.source_id == Some((1, 0)))
    );
    assert!(
        store
            .read(&owner.name, 1, &view("clock", BASE + 599, BASE + 600))
            .unwrap()
            .iter()
            .any(|r| r.comment.source_id == Some((1, COUNT - 1)))
    );
    store.retain_live(&owner.name, 100_000, &[clock]).unwrap();
    assert!(
        store
            .read(&owner.name, 1, &view("clock", BASE + 83, BASE + 84))
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        store
            .read(&owner.name, 1, &view("clock", BASE + 84, BASE + 85))
            .unwrap()
            .len(),
        100
    );
}
#[test]
fn utc_rollback_retention_uses_media_positions_and_clock_identity() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let owner = store.session().unwrap();
    let old = span("old", 0, 30_000, BASE * 1000);
    let new = span("new", 60_000, 90_000, BASE * 1000);
    store
        .insert_live(
            &owner.name,
            1,
            Some(&old),
            &[(
                comment(BASE as u64 * 1_000_000 + 5_000_000, "old", 1),
                false,
            )],
        )
        .unwrap();
    store
        .insert_live(
            &owner.name,
            1,
            Some(&new),
            &[(comment(BASE as u64 * 1_000_000 + 5_000_000, "new", 2), true)],
        )
        .unwrap();
    store.retain_live(&owner.name, 60_000, &[new]).unwrap();
    assert!(
        store
            .read(&owner.name, 1, &view("old", BASE, BASE + 30))
            .unwrap()
            .is_empty()
    );
    let records = store
        .read(&owner.name, 1, &view("new", BASE, BASE + 30))
        .unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].comment.text.as_ref(), "new");
    assert_eq!(records[0].media_ms, Some(65_000));
}

fn scheduled_target(start: i64, seconds: i64) -> plan::Target {
    let mut planner = plan::Planner::default();
    planner
        .update(
            &Demand {
                source: 1,
                channel: 1,
                view: None,
                fetch: true,
                source_range: Source::Recording(Recording::Observed {
                    current: Program::new(
                        ProgramId {
                            network: 1,
                            transport: 1,
                            service: 1,
                            event: 1,
                        },
                        start,
                        seconds,
                    ),
                    next: None,
                    utc_seconds: None,
                    at_start: false,
                }),
            },
            0,
        )
        .unwrap()
}

fn import_plan(store: &mut Store, dir: &Path, request: RequestPlan, fetched: i64, body: &str) {
    let mut incoming = Receiving::prepare(
        dir,
        Receipt {
            id: format!("plan-{fetched}-{}", request.refresh),
            channel: request.target.channel,
            range: request.range,
            fetched,
            generation: store.generation().unwrap(),
            plan: Some(request),
        },
    )
    .unwrap();
    incoming.write(body.as_bytes()).unwrap();
    let validated = incoming
        .finish()
        .unwrap()
        .validate()
        .map_err(|(_, error)| error)
        .unwrap();
    validated.import(store, || false).unwrap();
    validated.finish().unwrap();
}

#[test]
fn eighteen_fragments_are_completed_by_one_envelope_and_reused_after_reopening() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let target = scheduled_target(BASE, 114 * 60);
    let now = target.range.end + SETTLED_SECONDS * 2;
    for i in 0..18 {
        let start = BASE + 30 + i * 200;
        import(
            &mut store,
            dir.path(),
            &format!("fragment-{i}"),
            Interval::new(start, start + 2).unwrap(),
            r#"{"packet":[]}"#,
        );
    }
    let Planned::Ready(request) = store.planned("test", &target, now).unwrap() else {
        panic!("missing programme");
    };
    assert_eq!(request.range, target.range);
    import_plan(&mut store, dir.path(), request, now, r#"{"packet":[]}"#);
    drop(store);
    let store = Store::open(dir.path()).unwrap();
    assert!(matches!(
        store
            .planned("test", &target, now + SETTLED_SECONDS * 365)
            .unwrap(),
        Planned::Complete
    ));
}

#[test]
fn recent_snapshot_is_reused_hourly_then_rechecked_once_and_manual_refresh_is_coalesced() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let target = scheduled_target(BASE, 3600);
    let early = target.range.end + ARCHIVE_DELAY_SECONDS + COLLECTION_SECONDS;
    let Planned::Ready(request) = store.planned("test", &target, early).unwrap() else {
        panic!("initial");
    };
    import_plan(&mut store, dir.path(), request, early, r#"{"packet":[]}"#);
    assert!(!store.is_settled(target.channel, target.range).unwrap());
    for hour in [1, 2, 12, 23] {
        assert!(matches!(
            store
                .planned("test", &target, target.range.end + hour * 3600)
                .unwrap(),
            Planned::Complete
        ));
    }
    let mature = target.range.end + SETTLED_SECONDS;
    let Planned::Ready(request) = store.planned("test", &target, mature).unwrap() else {
        panic!("one final check");
    };
    assert_eq!(request.range, target.range);
    import_plan(&mut store, dir.path(), request, mature, r#"{"packet":[]}"#);
    assert!(matches!(
        store
            .planned("test", &target, mature + SETTLED_SECONDS * 30)
            .unwrap(),
        Planned::Complete
    ));
    assert!(store.is_settled(target.channel, target.range).unwrap());
    store.request_refresh(&target, mature + 1).unwrap();
    store.request_refresh(&target, mature + 2).unwrap();
    let Planned::Ready(request) = store.planned("test", &target, mature + 2).unwrap() else {
        panic!("manual check");
    };
    assert_eq!(request.refresh, 1);
    import_plan(
        &mut store,
        dir.path(),
        request,
        mature + 2,
        r#"{"packet":[]}"#,
    );
    assert!(matches!(
        store.planned("test", &target, mature + 3).unwrap(),
        Planned::Complete
    ));
}

#[test]
fn overlapping_snapshots_keep_maximum_multiplicity_and_empty_does_not_delete_posts() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let session = store.session().unwrap();
    let range = Interval::new(BASE, BASE + 60).unwrap();
    let body = r#"{"packet":[{"chat":{"date":100005,"content":"same"}},{"chat":{"date":100005,"content":"same"}},{"chat":{"date":100005,"content":"same","nx_jikkyo":1}}]}"#;
    import(&mut store, dir.path(), "first", range, body);
    import(&mut store, dir.path(), "overlap", range, body);
    import(
        &mut store,
        dir.path(),
        "temporarily-empty",
        range,
        r#"{"packet":[]}"#,
    );
    let records = store
        .read(&session.name, 1, &view("clock", BASE, BASE + 60))
        .unwrap();
    assert_eq!(records.len(), 3);
    assert_eq!(
        records
            .iter()
            .filter(|r| r.comment.origin == Origin::Nx)
            .count(),
        1
    );
}

#[test]
fn old_database_migration_preserves_empty_coverage_and_request_waiting() {
    let dir = tempfile::tempdir().unwrap();
    let db = Connection::open(dir.path().join("cache.sqlite3")).unwrap();
    db.execute_batch(include_str!("../schema.sql")).unwrap();
    db.execute_batch("PRAGMA user_version=1; INSERT INTO coverage(channel,start,end,fetched,last_used,receipt,published,bytes)
        VALUES(1,100000,100060,300000,300000,'legacy',1,0);
        UPDATE provider SET wait_until=400000;
        INSERT INTO requests(started) VALUES(300000);").unwrap();
    drop(db);
    let mut store = Store::open(dir.path()).unwrap();
    let range = Interval::new(BASE, BASE + 60).unwrap();
    assert_eq!(store.covered(1, range, 500000).unwrap(), vec![range]);
    assert!(store.imported("legacy").unwrap());
    assert_eq!(
        store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        SCHEMA_VERSION
    );
    let lease = store.try_provider().unwrap().unwrap();
    assert!(matches!(
        store.reserve(&lease, 2, range, 300100).unwrap(),
        Reservation::Waiting(400000)
    ));
}

#[test]
fn failed_target_stops_after_two_retries_and_restart_cannot_reset_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let target = scheduled_target(BASE, 3600);
    let now = BASE + 2 * SETTLED_SECONDS;
    assert!(
        store
            .fail_target(&target, "unavailable", false, None, now)
            .unwrap()
            .is_some()
    );
    assert!(
        store
            .fail_target(&target, "unavailable", false, None, now + 600)
            .unwrap()
            .is_some()
    );
    assert_eq!(
        store
            .fail_target(&target, "unavailable", false, None, now + 3000)
            .unwrap(),
        None
    );
    drop(store);
    let mut store = Store::open(dir.path()).unwrap();
    assert!(matches!(
        store
            .planned("test", &target, now + SETTLED_SECONDS)
            .unwrap(),
        Planned::Failed(_)
    ));
    store
        .request_refresh(&target, now + SETTLED_SECONDS)
        .unwrap();
    assert!(matches!(
        store
            .planned("test", &target, now + SETTLED_SECONDS)
            .unwrap(),
        Planned::Ready(_)
    ));
}

#[test]
fn quota_evicts_whole_targets_and_protects_every_fragment_when_one_is_pinned() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let session = store.session().unwrap();
    let target = scheduled_target(BASE, 3600);
    let mature = target.range.end + SETTLED_SECONDS;
    for (offset, start, end) in [(0, BASE, BASE + 60), (1, BASE + 120, BASE + 180)] {
        import_plan(
            &mut store,
            dir.path(),
            RequestPlan {
                target: target.clone(),
                range: Interval::new(start, end).unwrap(),
                refresh: 0,
            },
            mature + offset,
            r#"{"packet":[]}"#,
        );
    }
    store.set_budget(COVERAGE_CHARGE);
    store
        .pin_archive(
            &session.name,
            1,
            Interval::new(BASE, BASE + 1).unwrap(),
            mature,
        )
        .unwrap();
    store.cleanup(mature, false).unwrap();
    assert_eq!(store.covered(1, target.range, mature).unwrap().len(), 2);
    store.release_session(&session.name).unwrap();
    store.cleanup(mature, false).unwrap();
    assert!(store.covered(1, target.range, mature).unwrap().is_empty());
    assert!(matches!(
        store.planned("test", &target, mature).unwrap(),
        Planned::Ready(_)
    ));
}

#[test]
#[ignore = "read-only source probe; requires NAGAMETV_COMMENT_CACHE_PROBE"]
fn copied_legacy_cache_is_preserved_and_completed_as_one_program() {
    let path =
        std::env::var_os("NAGAMETV_COMMENT_CACHE_PROBE").expect("NAGAMETV_COMMENT_CACHE_PROBE");
    let dir = tempfile::tempdir().unwrap();
    std::fs::copy(path, dir.path().join("cache.sqlite3")).unwrap();
    let before = Connection::open(dir.path().join("cache.sqlite3")).unwrap();
    let count: i64 = before
        .query_row("SELECT count(*) FROM comments", [], |row| row.get(0))
        .unwrap();
    drop(before);
    let mut store = Store::open(dir.path()).unwrap();
    // 2026-09-18 21:00 JST, 114-minute EIT from the supplied recording.
    const MOVIE_START: i64 = 1_789_732_800;
    let mut target = scheduled_target(MOVIE_START, 114 * 60);
    target.channel = 4;
    let now = target.range.end + SETTLED_SECONDS;
    let ranges = store.covered(4, target.range, now).unwrap();
    let Planned::Ready(request) = store.planned("probe", &target, now).unwrap() else {
        panic!("legacy fragments need completion")
    };
    assert_eq!(request.range, target.range);
    import_plan(&mut store, dir.path(), request, now, r#"{"packet":[]}"#);
    let after: i64 = store
        .db
        .query_row("SELECT count(*) FROM comments", [], |row| row.get(0))
        .unwrap();
    assert_eq!(after, count, "empty refresh removed valid legacy comments");
    assert!(matches!(
        store.planned("probe", &target, now).unwrap(),
        Planned::Complete
    ));
    eprintln!(
        "legacy cache: {} fragments, {} covered seconds, {} total DB comments preserved; completed as one range [{},{})",
        ranges.len(),
        ranges.iter().map(|r| r.end - r.start).sum::<i64>(),
        count,
        target.range.start,
        target.range.end
    );
}
