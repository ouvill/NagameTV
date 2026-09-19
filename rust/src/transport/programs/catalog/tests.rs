use super::*;
const SECOND: u64 = 1_000_000_000;
const UTC: i64 = 1_700_000_000_000;
fn event(id: u16) -> Program {
    Program {
        event_id: id,
        service_id: 1,
        network_id: 4,
        transport_stream_id: 1,
        start_at: Some(UTC),
        duration: Some(10_000),
        name: format!("event {id}"),
        description: String::new(),
        extended: String::new(),
        genres: vec![],
        audios: Box::default(),
    }
}
fn observe(
    catalog: &mut Catalog,
    cursor: &mut ScanCursor,
    epoch: u64,
    seconds: u64,
    information: Information,
) {
    catalog.observe(
        cursor,
        ScanPoint {
            accuracy: Accuracy::Indexed,
            epoch,
            offset: seconds * 188,
            position: seconds * SECOND,
            end: (seconds + 1) * SECOND,
            observation: Some(&Observation {
                pcr: seconds * SECOND,
                information,
            }),
        },
    );
}
#[test]
fn title_and_clock_survive_sparse_si_and_scheduled_end_but_explicit_empty_wins() {
    let mut catalog = Catalog::default();
    let mut cursor = ScanCursor::default();
    let mut info = Information {
        current: Present::Event(event(1)),
        time: Some((0, UTC)),
        ..Default::default()
    };
    observe(&mut catalog, &mut cursor, 0, 0, info.clone());
    observe(&mut catalog, &mut cursor, 0, 600, info.clone());
    let view = catalog.view(300 * SECOND);
    assert_eq!(view.status, Status::Available);
    assert_eq!(view.clock.unwrap().utc(300 * SECOND), Some(UTC + 300_000));
    assert_eq!(catalog.samples, 1);
    info.current = Present::Empty;
    observe(&mut catalog, &mut cursor, 0, 601, info);
    assert_eq!(catalog.view(601 * SECOND).status, Status::Unavailable);
    assert_eq!(catalog.view(300 * SECOND).program.unwrap().event_id, 1);
}
#[test]
fn unknown_title_does_not_block_clock_and_late_clock_backfills_only_continuous_scan() {
    let mut catalog = Catalog::default();
    let mut scan = ScanCursor::default();
    observe(&mut catalog, &mut scan, 0, 0, Information::default());
    observe(
        &mut catalog,
        &mut scan,
        0,
        5,
        Information {
            time: Some((5 * SECOND, UTC + 5000)),
            ..Default::default()
        },
    );
    let view = catalog.view(SECOND);
    assert_eq!(view.status, Status::Pending);
    assert_eq!(view.clock.unwrap().utc(SECOND), Some(UTC + 1000));
    let mut random = ScanCursor::default();
    observe(&mut catalog, &mut random, 0, 100, Information::default());
    assert!(catalog.view(100 * SECOND).clock.is_none());
    assert!(catalog.view(50 * SECOND).clock.is_none());
}
#[test]
fn discontinuities_use_new_clock_and_preserve_old_mapping_for_backwards_seek() {
    let mut catalog = Catalog::default();
    let mut scan = ScanCursor::default();
    observe(
        &mut catalog,
        &mut scan,
        0,
        0,
        Information {
            time: Some((0, UTC)),
            ..Default::default()
        },
    );
    observe(&mut catalog, &mut scan, 0, 5, Information::default());
    observe(&mut catalog, &mut scan, 1, 6, Information::default());
    assert!(catalog.view(6 * SECOND).clock.is_none());
    observe(
        &mut catalog,
        &mut scan,
        1,
        10,
        Information {
            time: Some((10 * SECOND, UTC + 3_600_000)),
            ..Default::default()
        },
    );
    assert_eq!(
        catalog.view(6 * SECOND).clock.unwrap().utc(6 * SECOND),
        Some(UTC + 3_596_000)
    );
    assert_eq!(
        catalog.view(SECOND).clock.unwrap().utc(SECOND),
        Some(UTC + 1000)
    );
}
#[test]
fn lone_bad_tot_is_ignored_and_confirmed_clock_jump_starts_at_first_new_sample() {
    let mut catalog = Catalog::default();
    let mut scan = ScanCursor::default();
    for (second, utc) in [
        (0, UTC),
        (5, UTC + 3_605_000),
        (10, UTC + 10_000),
        (15, UTC + 3_615_000),
        (20, UTC + 3_620_000),
    ] {
        observe(
            &mut catalog,
            &mut scan,
            0,
            second,
            Information {
                time: Some((second * SECOND, utc)),
                ..Default::default()
            },
        );
    }
    assert_eq!(
        catalog.view(5 * SECOND).clock.unwrap().utc(5 * SECOND),
        Some(UTC + 5000)
    );
    assert_eq!(
        catalog.view(15 * SECOND).clock.unwrap().utc(15 * SECOND),
        Some(UTC + 3_615_000)
    );
    let reading = catalog.view(20 * SECOND).clock.unwrap();
    assert!(reading.media(UTC).is_none());
    assert_eq!(reading.media(UTC + 3_620_000), Some(20 * SECOND));
}
#[test]
fn recording_can_show_title_with_unknown_progress_without_a_broadcast_clock() {
    let mut catalog = Catalog::default();
    observe(
        &mut catalog,
        &mut ScanCursor::default(),
        0,
        0,
        Information {
            current: Present::Event(event(1)),
            ..Default::default()
        },
    );
    let (json, progress) = catalog.view(0).presentation(0);
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["name"], "event 1");
    assert_eq!(value["progressKnown"], false);
    assert_eq!(progress, 0.);
}

#[test]
fn partial_observations_do_not_clear_present_or_revive_it_after_explicit_empty() {
    let mut catalog = Catalog::default();
    let mut scan = ScanCursor::default();
    observe(
        &mut catalog,
        &mut scan,
        0,
        0,
        Information {
            current: Present::Event(event(1)),
            time: Some((0, UTC)),
            ..Default::default()
        },
    );
    observe(&mut catalog, &mut scan, 0, 30, Information::default());
    assert_eq!(catalog.view(30 * SECOND).program.unwrap().event_id, 1);
    observe(
        &mut catalog,
        &mut scan,
        0,
        31,
        Information {
            current: Present::Empty,
            ..Default::default()
        },
    );
    observe(&mut catalog, &mut scan, 0, 32, Information::default());
    assert_eq!(catalog.view(32 * SECOND).status, Status::Unavailable);
}

#[test]
fn seeded_seek_reader_does_not_shorten_established_clock_lookback() {
    let mut catalog = Catalog::default();
    let mut full = ScanCursor::default();
    observe(
        &mut catalog,
        &mut full,
        0,
        0,
        Information {
            time: Some((0, UTC)),
            ..Default::default()
        },
    );
    observe(&mut catalog, &mut full, 0, 60, Information::default());
    let mut reader = ScanCursor::default();
    observe(
        &mut catalog,
        &mut reader,
        0,
        37,
        Information {
            time: Some((37 * SECOND, UTC + 37_000)),
            ..Default::default()
        },
    );
    observe(&mut catalog, &mut reader, 0, 45, Information::default());
    let clock = catalog.view(40 * SECOND).clock.unwrap();
    assert_eq!(clock.media(UTC + 30_000), Some(30 * SECOND));
}

#[test]
fn eit_without_optional_descriptors_keeps_known_fields_of_the_same_event() {
    let mut catalog = Catalog::default();
    let mut scan = ScanCursor::default();
    observe(
        &mut catalog,
        &mut scan,
        0,
        0,
        Information {
            current: Present::Event(event(1)),
            ..Default::default()
        },
    );
    let mut partial = event(1);
    partial.name.clear();
    partial.duration = None;
    observe(
        &mut catalog,
        &mut scan,
        0,
        5,
        Information {
            current: Present::Event(partial),
            ..Default::default()
        },
    );
    let program = catalog.view(5 * SECOND).program.unwrap();
    assert_eq!(program.name, "event 1");
    assert_eq!(
        program.duration, None,
        "undefined scheduling is still unknown"
    );
    let mut other = event(2);
    other.name.clear();
    observe(
        &mut catalog,
        &mut scan,
        0,
        10,
        Information {
            current: Present::Event(other),
            ..Default::default()
        },
    );
    assert!(catalog.view(10 * SECOND).program.unwrap().name.is_empty());
}
