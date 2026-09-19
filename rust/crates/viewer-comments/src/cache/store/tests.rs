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
        r#"{"packet":[{"chat":{"date":100005,"content":"before"}},{"chat":{"date":100025,"content":"remove"}},{"chat":{"date":100055,"content":"after"}}]}"#,
    );
    import(
        &mut store,
        dir.path(),
        "empty",
        Interval::new(BASE + 20, BASE + 40).unwrap(),
        r#"{"packet":[]}"#,
    );
    let records = store
        .read(
            &session.name,
            1,
            &view("clock", BASE, BASE + 60),
            BASE + SETTLED_SECONDS * 3,
        )
        .unwrap();
    assert_eq!(
        records
            .iter()
            .map(|r| r.comment.text.as_ref())
            .collect::<Vec<_>>(),
        vec!["before", "after"]
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
        2
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
        .pin(&owner.name, &[span("recording", 0, 60_000, BASE * 1000)])
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
        .execute("UPDATE coverage SET bytes=?1", [CACHE_BYTES + 1])
        .unwrap();
    store.cleanup(BASE + SETTLED_SECONDS * 3, false).unwrap();
    store.cleanup(BASE + SETTLED_SECONDS * 3, true).unwrap();
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
            .read(&owner.name, 1, &view("clock", BASE, BASE + 1), BASE)
            .unwrap()
            .iter()
            .any(|r| r.comment.source_id == Some((1, 0)))
    );
    assert!(
        store
            .read(&owner.name, 1, &view("clock", BASE + 599, BASE + 600), BASE)
            .unwrap()
            .iter()
            .any(|r| r.comment.source_id == Some((1, COUNT - 1)))
    );
    store.retain_live(&owner.name, 100_000, &[clock]).unwrap();
    assert!(
        store
            .read(&owner.name, 1, &view("clock", BASE + 83, BASE + 84), BASE)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        store
            .read(&owner.name, 1, &view("clock", BASE + 84, BASE + 85), BASE)
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
            .read(&owner.name, 1, &view("old", BASE, BASE + 30), BASE)
            .unwrap()
            .is_empty()
    );
    let records = store
        .read(&owner.name, 1, &view("new", BASE, BASE + 30), BASE)
        .unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].comment.text.as_ref(), "new");
    assert_eq!(records[0].media_ms, Some(65_000));
}
