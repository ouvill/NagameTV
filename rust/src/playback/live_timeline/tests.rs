use super::*;
use crate::transport::programs::Information;

const SECOND_MS: i64 = 1_000;
const MINUTE_MS: i64 = 60 * SECOND_MS;
const SHOW_MS: i64 = 30 * MINUTE_MS;
const BROADCAST_START_MS: i64 = 1_800_000_000_000;
const EPOCH: u64 = 1;
fn ns(ms: i64) -> u64 {
    ms as u64 * NS_PER_MS
}
fn program(id: u16, start: i64) -> Program {
    Program {
        event_id: id,
        service_id: 1,
        network_id: 1,
        transport_stream_id: 1,
        start_at: Some(BROADCAST_START_MS + start),
        duration: Some(SHOW_MS as u64),
        name: format!("番組{id}"),
        description: String::new(),
        extended: String::new(),
        genres: Vec::new(),
        audios: Box::default(),
    }
}
fn observe(history: &mut History, epoch: u64, position: i64, utc: i64, event: Option<Program>) {
    history.observe(
        epoch,
        ns(position),
        ns(position + SECOND_MS),
        Some(&Arc::new(Observation {
            pcr: ns(position),
            information: Information {
                current: event.into(),
                time: Some((ns(position), utc)),
                ..Default::default()
            },
        })),
    );
}
fn three_programs() -> History {
    let mut history = History::default();
    for id in 1..=3 {
        let start = i64::from(id - 1) * SHOW_MS;
        observe(
            &mut history,
            EPOCH,
            start,
            BROADCAST_START_MS + start,
            Some(program(id, start)),
        );
    }
    history.advance_end(ns(75 * MINUTE_MS));
    history
}
fn project(
    presenter: &mut Presenter,
    history: &mut History,
    start: i64,
    end: i64,
    phase: Phase,
    position: i64,
) -> Snapshot {
    presenter.project(
        history,
        ns(start),
        ns(end),
        true,
        Reading {
            phase,
            position_ns: Some(ns(position)),
            target_ns: None,
        },
    )
}
#[test]
fn fractional_retention_start_keeps_the_full_program_axis() {
    const PROGRAM_MINUTES: i64 = 27;
    const RETAINED_MINUTES: i64 = 21;
    const LIVE_MINUTES: i64 = 23;
    const PCR_FRACTION_NS: u64 = 333_333;
    let start = ns(RETAINED_MINUTES * MINUTE_MS) + PCR_FRACTION_NS;
    let end = ns(LIVE_MINUTES * MINUTE_MS) + PCR_FRACTION_NS;
    let mut event = program(1, 0);
    event.duration = Some((PROGRAM_MINUTES * MINUTE_MS) as u64);
    let mut history = History::default();
    history.observe(
        EPOCH,
        start,
        start + ns(SECOND_MS),
        Some(&Arc::new(Observation {
            pcr: start,
            information: Information {
                current: Some(event).into(),
                time: Some((start, BROADCAST_START_MS + RETAINED_MINUTES * MINUTE_MS)),
                ..Default::default()
            },
        })),
    );
    history.advance_end(end);
    history.expire(start);
    assert!(history.metadata(start).program.is_some());
    let mut presenter = Presenter::new();
    let snapshot = presenter.project(
        &mut history,
        start,
        end,
        true,
        Reading {
            phase: Phase::Paused,
            position_ns: Some(start),
            target_ns: None,
        },
    );
    assert_eq!(
        (snapshot.axis.start.0, snapshot.axis.end.0),
        (0, PROGRAM_MINUTES * MINUTE_MS)
    );
    assert_eq!(snapshot.axis.start_utc, Some(UtcMs(BROADCAST_START_MS)));
    assert_eq!(
        snapshot.axis.end_utc,
        Some(UtcMs(BROADCAST_START_MS + PROGRAM_MINUTES * MINUTE_MS))
    );
    assert_eq!(
        snapshot.available,
        [Span {
            start: MediaMs(RETAINED_MINUTES * MINUTE_MS),
            end: MediaMs(LIVE_MINUTES * MINUTE_MS),
        }]
    );
    let viewing = snapshot.viewing.unwrap();
    assert_eq!(viewing.program.unwrap().title, "番組1");
    assert_eq!(
        viewing.utc,
        Some(UtcMs(BROADCAST_START_MS + RETAINED_MINUTES * MINUTE_MS))
    );
}

#[test]
fn fractional_pcr_updates_keep_program_boundary_clocks_exact() {
    const CLOCK_FRACTION_NS: u64 = 800_000;
    const FIRST_PCR_NS: u64 = NS_PER_MS + 200_000;
    const PCR_FRACTIONS_NS: [u64; 2] = [200_000, 900_000];
    const UPDATE_COUNT: usize = 6;
    let mut history = History::default();
    // The first TDT arrives after reception starts, between two PCR samples.
    history.observe(EPOCH, 0, FIRST_PCR_NS, None);
    history.observe(
        EPOCH,
        FIRST_PCR_NS,
        FIRST_PCR_NS + ns(SECOND_MS),
        Some(&Arc::new(Observation {
            pcr: FIRST_PCR_NS,
            information: Information {
                current: Some(program(1, 0)).into(),
                time: Some((CLOCK_FRACTION_NS, BROADCAST_START_MS)),
                ..Default::default()
            },
        })),
    );
    let mut presenter = Presenter::new();
    for (update, fraction_ns) in PCR_FRACTIONS_NS
        .into_iter()
        .cycle()
        .take(UPDATE_COUNT)
        .enumerate()
    {
        let received_ms = (update as i64 + 1) * SECOND_MS;
        let start_ns = ns(received_ms) + fraction_ns;
        let end_ns = ns(MINUTE_MS + received_ms) + fraction_ns;
        history.advance_end(end_ns);
        let snapshot = presenter.project(
            &mut history,
            start_ns,
            end_ns,
            true,
            Reading {
                phase: Phase::Playing,
                position_ns: Some(end_ns - ns(SECOND_MS)),
                target_ns: None,
            },
        );
        assert_eq!(
            snapshot.axis.end_utc,
            Some(UtcMs(BROADCAST_START_MS + SHOW_MS)),
            "a fractional PCR update must not move the scheduled end into the previous minute"
        );
        assert_eq!(snapshot.axis.start_utc, Some(UtcMs(BROADCAST_START_MS)));
        let span = snapshot.live.program.as_ref().unwrap().span.unwrap();
        assert_eq!(span.start, snapshot.programs[0].start);
        assert_eq!(span.end, snapshot.programs[0].end);
        assert_eq!(span.start, snapshot.axis.start);
        assert_eq!(span.end, snapshot.axis.end);
    }
}

#[test]
fn three_program_axis_and_broadcast_progress_do_not_follow_the_playhead() {
    let mut history = three_programs();
    let mut presenter = Presenter::new();
    let old = project(
        &mut presenter,
        &mut history,
        10 * MINUTE_MS,
        75 * MINUTE_MS,
        Phase::Playing,
        20 * MINUTE_MS,
    );
    assert_eq!((old.axis.start.0, old.axis.end.0), (0, 90 * MINUTE_MS));
    assert_eq!(old.programs.len(), 3);
    assert_eq!(old.boundaries, [MediaMs(SHOW_MS), MediaMs(2 * SHOW_MS)]);
    assert_eq!(
        (old.available[0].start.0, old.available[0].end.0),
        (10 * MINUTE_MS, 75 * MINUTE_MS)
    );
    assert_eq!(
        old.viewing
            .as_ref()
            .unwrap()
            .program
            .as_ref()
            .unwrap()
            .title,
        "番組1"
    );
    assert_eq!(old.live.program.as_ref().unwrap().title, "番組3");
    assert!((old.live.program.as_ref().unwrap().progress - 0.5).abs() < 0.001);
    let new = project(
        &mut presenter,
        &mut history,
        10 * MINUTE_MS,
        75 * MINUTE_MS,
        Phase::Playing,
        40 * MINUTE_MS,
    );
    assert_eq!(new.axis, old.axis);
    assert_eq!(new.live, old.live);
    assert_eq!(new.viewing.unwrap().program.unwrap().title, "番組2");
}
#[test]
fn partial_expiry_keeps_axis_and_metadata_outlives_the_expired_paused_ts() {
    let mut history = three_programs();
    let mut presenter = Presenter::new();
    let paused = project(
        &mut presenter,
        &mut history,
        10 * MINUTE_MS,
        75 * MINUTE_MS,
        Phase::Paused,
        20 * MINUTE_MS,
    );
    history.expire(ns(25 * MINUTE_MS));
    let expired = project(
        &mut presenter,
        &mut history,
        25 * MINUTE_MS,
        75 * MINUTE_MS,
        Phase::Paused,
        20 * MINUTE_MS,
    );
    assert_eq!(expired.axis, paused.axis);
    assert_eq!(
        expired.viewing.as_ref().unwrap().availability,
        Availability::Expired
    );
    assert_eq!(
        expired.viewing.as_ref().unwrap().program,
        paused.viewing.as_ref().unwrap().program
    );
    history.expire(ns(35 * MINUTE_MS));
    let outside = project(
        &mut presenter,
        &mut history,
        35 * MINUTE_MS,
        75 * MINUTE_MS,
        Phase::Paused,
        20 * MINUTE_MS,
    );
    assert_eq!(outside.axis.start.0, SHOW_MS);
    let view = outside.viewing.unwrap();
    assert!(view.offscreen);
    assert_eq!(view.position.0, 20 * MINUTE_MS);
    assert_eq!(view.program.unwrap().title, "番組1");
    assert_eq!(outside.programs.len(), 2);
    assert!(history.selection(ns(20 * MINUTE_MS)).is_some());
    assert!(
        history
            .retained(Some(Span {
                start: MediaMs(20 * MINUTE_MS),
                end: MediaMs(21 * MINUTE_MS),
            }))
            .is_empty()
    );
}
#[test]
fn missing_position_sample_keeps_the_last_view_until_a_new_position_is_known() {
    let mut history = three_programs();
    let mut presenter = Presenter::new();
    let before = project(
        &mut presenter,
        &mut history,
        0,
        75 * MINUTE_MS,
        Phase::Playing,
        20 * MINUTE_MS,
    );
    let missing = presenter.project(
        &mut history,
        0,
        ns(75 * MINUTE_MS),
        true,
        Reading {
            phase: Phase::Playing,
            position_ns: None,
            target_ns: None,
        },
    );
    assert_eq!(missing.viewing, before.viewing);
    assert_eq!(missing.viewing_program(), before.viewing_program());
    assert_eq!(missing.program_status(), before.program_status());

    let recovered = project(
        &mut presenter,
        &mut history,
        0,
        75 * MINUTE_MS,
        Phase::Playing,
        40 * MINUTE_MS,
    );
    assert_eq!(recovered.viewing.unwrap().program.unwrap().title, "番組2");

    // A known position without metadata must still clear the previous title.
    let unknown = project(
        &mut presenter,
        &mut history,
        0,
        75 * MINUTE_MS,
        Phase::Playing,
        90 * MINUTE_MS,
    );
    assert!(unknown.viewing.as_ref().unwrap().program.is_none());
    assert_eq!(
        unknown.program_status(),
        crate::transport::programs::catalog::Status::Pending
    );

    // A replacement session cannot reuse the previous channel's view.
    let startup = Presenter::new().project(
        &mut history,
        0,
        ns(75 * MINUTE_MS),
        true,
        Reading {
            phase: Phase::Playing,
            position_ns: None,
            target_ns: None,
        },
    );
    assert!(startup.viewing.is_none());
}

#[test]
fn seek_target_does_not_replace_presented_frame_until_output_is_confirmed() {
    let mut history = three_programs();
    let mut presenter = Presenter::new();
    let before = project(
        &mut presenter,
        &mut history,
        10 * MINUTE_MS,
        75 * MINUTE_MS,
        Phase::Paused,
        20 * MINUTE_MS,
    );
    let target = 65 * MINUTE_MS;
    let seeking = presenter.project(
        &mut history,
        ns(10 * MINUTE_MS),
        ns(75 * MINUTE_MS),
        true,
        Reading {
            phase: Phase::Seeking(Resume::Paused),
            position_ns: Some(ns(target)),
            target_ns: Some(ns(target)),
        },
    );
    assert_eq!(seeking.viewing, before.viewing);
    assert_eq!(seeking.axis, before.axis);
    assert_eq!(seeking.seek_target, Some(MediaMs(target)));
    let completed = project(
        &mut presenter,
        &mut history,
        10 * MINUTE_MS,
        75 * MINUTE_MS,
        Phase::Paused,
        target,
    );
    assert_eq!(completed.viewing.unwrap().program.unwrap().title, "番組3");
    assert!(completed.seek_target.is_none());
}
#[test]
fn missing_si_preserves_known_schedule_but_does_not_extend_an_ended_show() {
    let mut history = History::default();
    observe(
        &mut history,
        EPOCH,
        0,
        BROADCAST_START_MS,
        Some(program(1, 0)),
    );
    let mut presenter = Presenter::new();
    let initial = project(
        &mut presenter,
        &mut history,
        0,
        SECOND_MS,
        Phase::Playing,
        0,
    );
    let halfway = SHOW_MS / 2;
    observe(
        &mut history,
        EPOCH,
        halfway,
        BROADCAST_START_MS + halfway,
        None,
    );
    let missing = project(
        &mut presenter,
        &mut history,
        0,
        halfway,
        Phase::Playing,
        halfway,
    );
    assert_eq!(missing.axis, initial.axis);
    assert_eq!(missing.live.program.unwrap().title, "番組1");
    let after = SHOW_MS + SECOND_MS;
    history.advance_end(ns(after));
    let ended = project(
        &mut presenter,
        &mut history,
        0,
        after,
        Phase::Playing,
        halfway,
    );
    assert!(ended.live.program.is_none());
    assert_eq!(ended.axis.start.0, 0);
    assert_eq!(ended.axis.end.0, after);
    assert!(ended.axis.end >= initial.axis.end);
    assert!(!ended.available.is_empty());
}
#[test]
fn schedule_changes_update_existing_event_and_preserve_media_coordinates() {
    let mut history = History::default();
    observe(
        &mut history,
        EPOCH,
        0,
        BROADCAST_START_MS,
        Some(program(1, 0)),
    );
    let mut extended = program(1, 0);
    extended.duration = Some((SHOW_MS + 10 * MINUTE_MS) as u64);
    observe(
        &mut history,
        EPOCH,
        MINUTE_MS,
        BROADCAST_START_MS + MINUTE_MS,
        Some(extended),
    );
    let mut presenter = Presenter::new();
    let snapshot = project(
        &mut presenter,
        &mut history,
        0,
        MINUTE_MS,
        Phase::Playing,
        0,
    );
    assert_eq!(snapshot.programs.len(), 1);
    assert_eq!(snapshot.axis.end.0, 40 * MINUTE_MS);
    let shifted = program(1, -MINUTE_MS);
    observe(
        &mut history,
        EPOCH,
        2 * MINUTE_MS,
        BROADCAST_START_MS + 2 * MINUTE_MS,
        Some(shifted),
    );
    let changed = project(
        &mut presenter,
        &mut history,
        0,
        2 * MINUTE_MS,
        Phase::Playing,
        0,
    );
    assert_eq!(changed.programs.len(), 1);
    assert_eq!(changed.axis.start.0, -MINUTE_MS);
}
#[test]
fn missing_clock_disabled_history_and_clock_acquisition_are_explicit() {
    let mut history = History::default();
    history.observe(EPOCH, 0, ns(MINUTE_MS), None);
    let mut presenter = Presenter::new();
    let elapsed = project(
        &mut presenter,
        &mut history,
        0,
        MINUTE_MS,
        Phase::Playing,
        0,
    );
    assert_eq!(elapsed.axis.clock, ClockMode::Elapsed);
    assert!(!elapsed.available.is_empty());
    observe(
        &mut history,
        EPOCH,
        MINUTE_MS,
        BROADCAST_START_MS + MINUTE_MS,
        Some(program(1, 0)),
    );
    let disabled = presenter.project(
        &mut history,
        0,
        ns(MINUTE_MS),
        false,
        Reading {
            phase: Phase::Paused,
            position_ns: Some(0),
            target_ns: None,
        },
    );
    assert_eq!(disabled.axis.clock, ClockMode::Broadcast);
    assert_eq!(disabled.axis.end.0, SHOW_MS);
    assert!(disabled.available.is_empty());
    assert_eq!(
        disabled.viewing.unwrap().availability,
        Availability::Disabled
    );
}
#[test]
fn reconnect_gaps_and_clock_jumps_do_not_create_seekable_missing_time() {
    let mut history = History::default();
    observe(
        &mut history,
        EPOCH,
        0,
        BROADCAST_START_MS,
        Some(program(1, 0)),
    );
    history.advance_end(ns(10 * MINUTE_MS));
    let resumed = 12 * MINUTE_MS;
    let utc_after_gap = BROADCAST_START_MS + SHOW_MS;
    observe(
        &mut history,
        EPOCH + 1,
        resumed,
        utc_after_gap,
        Some(program(2, SHOW_MS)),
    );
    history.advance_end(ns(20 * MINUTE_MS));
    let mut presenter = Presenter::new();
    let snapshot = project(
        &mut presenter,
        &mut history,
        0,
        20 * MINUTE_MS,
        Phase::Playing,
        5 * MINUTE_MS,
    );
    assert_eq!(snapshot.available.len(), 2);
    assert_eq!(snapshot.available[0].end.0, 10 * MINUTE_MS);
    assert_eq!(snapshot.available[1].start.0, resumed);
    let at_resumed: serde_json::Value =
        serde_json::from_str(&presenter.preview(&snapshot.session, resumed as f64)).unwrap();
    assert_eq!(at_resumed["utc"], utc_after_gap);
    assert_eq!(at_resumed["title"], "番組2");
    let in_gap: serde_json::Value =
        serde_json::from_str(&presenter.preview(&snapshot.session, (11 * MINUTE_MS) as f64))
            .unwrap();
    assert_eq!(in_gap["available"], false);
    assert!(in_gap["utc"].is_null());
    let target = history
        .seek_target(0, ns(20 * MINUTE_MS), (11 * MINUTE_MS) as f64)
        .unwrap();
    assert!(target.milliseconds > resumed as f64);
    assert!(matches!(target.correction, Correction::Adjusted));
    observe(
        &mut history,
        EPOCH + 1,
        21 * MINUTE_MS,
        utc_after_gap + SHOW_MS,
        Some(program(3, 2 * SHOW_MS)),
    );
    assert_eq!(history.epochs.len(), 2, "one outlier is not a new clock");
    observe(
        &mut history,
        EPOCH + 1,
        22 * MINUTE_MS,
        utc_after_gap + SHOW_MS + MINUTE_MS,
        Some(program(3, 2 * SHOW_MS)),
    );
    assert_eq!(
        history.epochs.len(),
        3,
        "a confirmed UTC correction splits the mapping"
    );
}
#[test]
fn expired_drag_uses_recovery_headroom_and_stale_session_preview_is_rejected() {
    let mut history = three_programs();
    let start = 35 * MINUTE_MS;
    let target = history
        .seek_target(ns(start), ns(75 * MINUTE_MS), (20 * MINUTE_MS) as f64)
        .unwrap();
    let expected_headroom = 5 * SECOND_MS;
    assert_eq!(target.milliseconds, (start + expected_headroom) as f64);
    assert!(history.seek_target(0, ns(SHOW_MS), f64::NAN).is_err());
    let mut old = Presenter::new();
    let snapshot = project(&mut old, &mut history, 0, 75 * MINUTE_MS, Phase::Playing, 0);
    let new = Presenter::new();
    assert!(!new.owns(&snapshot.session));
    assert_eq!(new.preview(&snapshot.session, 0.0), "null");
}
#[test]
fn catalog_limits_do_not_reduce_available_ts_and_revisions_change_only_with_content() {
    let mut history = History::default();
    for id in 1..=(MAX_PROGRAMS + 1) as u16 {
        let position = i64::from(id - 1) * SECOND_MS;
        observe(
            &mut history,
            EPOCH,
            position,
            BROADCAST_START_MS + position,
            Some(program(id, position)),
        );
    }
    assert_eq!(history.epochs[0].records.len(), MAX_PROGRAMS);
    let mut huge = program(u16::MAX, SHOW_MS);
    huge.description = "x".repeat(MAX_PROGRAM_BYTES + 1);
    observe(
        &mut history,
        EPOCH,
        SHOW_MS,
        BROADCAST_START_MS + SHOW_MS,
        Some(huge),
    );
    assert!(history.epochs[0].records.is_empty());
    let mut presenter = Presenter::new();
    let first = project(&mut presenter, &mut history, 0, SHOW_MS, Phase::Playing, 0);
    assert_eq!(first.available[0].start.0, 0);
    assert_eq!(first.available[0].end.0, SHOW_MS);
    let same = project(&mut presenter, &mut history, 0, SHOW_MS, Phase::Playing, 0);
    assert_eq!(same.revision, first.revision);
    let advanced = project(
        &mut presenter,
        &mut history,
        0,
        SHOW_MS,
        Phase::Playing,
        SECOND_MS,
    );
    assert!(advanced.revision > same.revision);
}

#[test]
fn clock_catalog_limit_keeps_older_bytes_seekable_as_unknown_programs() {
    let mut history = History::default();
    for epoch in 0..=(MAX_EPOCHS as u64) {
        let position = epoch as i64 * SECOND_MS;
        observe(
            &mut history,
            epoch,
            position,
            BROADCAST_START_MS + position,
            Some(program(1, 0)),
        );
    }
    assert_eq!(history.epochs.len(), MAX_EPOCHS);
    assert!(history.selection(ns(0)).is_none());
    let target = history
        .seek_target(0, ns(MAX_EPOCHS as i64 * SECOND_MS), 0.0)
        .unwrap();
    assert!(matches!(target.correction, Correction::Unchanged));
    assert_eq!(target.milliseconds, 0.0);
}
#[test]
fn old_broadcast_clock_is_not_reused_after_pcr_epoch_change() {
    let mut history = History::default();
    let observation = Arc::new(Observation {
        pcr: 0,
        information: Information {
            current: Some(program(1, 0)).into(),
            time: Some((0, BROADCAST_START_MS)),
            ..Default::default()
        },
    });
    history.observe(EPOCH, 0, ns(SECOND_MS), Some(&observation));
    history.observe(
        EPOCH + 1,
        ns(SECOND_MS),
        ns(2 * SECOND_MS),
        Some(&observation),
    );
    assert!(history.utc(ns(SECOND_MS)).is_none());
    assert!(history.epochs.back().unwrap().clock.is_none());
    observe(
        &mut history,
        EPOCH + 1,
        2 * SECOND_MS,
        BROADCAST_START_MS + SHOW_MS,
        Some(program(2, SHOW_MS)),
    );
    assert_eq!(
        history.utc(ns(2 * SECOND_MS)),
        Some(UtcMs(BROADCAST_START_MS + SHOW_MS))
    );
}

#[test]
fn broadcast_program_changes_while_the_viewer_stays_in_a_past_program() {
    let mut history = three_programs();
    let mut presenter = Presenter::new();
    let before = project(
        &mut presenter,
        &mut history,
        10 * MINUTE_MS,
        50 * MINUTE_MS,
        Phase::Playing,
        20 * MINUTE_MS,
    );
    let after = project(
        &mut presenter,
        &mut history,
        10 * MINUTE_MS,
        75 * MINUTE_MS,
        Phase::Playing,
        20 * MINUTE_MS,
    );
    assert_eq!(before.viewing, after.viewing);
    assert_eq!(before.live.program.unwrap().title, "番組2");
    assert_eq!(after.live.program.unwrap().title, "番組3");
    assert_eq!(before.axis.end.0, 2 * SHOW_MS);
    assert_eq!(after.axis.end.0, 3 * SHOW_MS);
}
#[test]
fn no_clock_following_event_does_not_overwrite_older_presented_program() {
    let mut history = History::default();
    let first = Arc::new(Observation {
        pcr: 0,
        information: Information {
            current: Some(program(1, 0)).into(),
            next: Some(program(2, SHOW_MS)),
            ..Default::default()
        },
    });
    history.observe(EPOCH, 0, ns(SECOND_MS), Some(&first));
    let second = Arc::new(Observation {
        pcr: ns(SHOW_MS),
        information: Information {
            current: Some(program(2, SHOW_MS)).into(),
            ..Default::default()
        },
    });
    history.observe(EPOCH, ns(SHOW_MS), ns(SHOW_MS + SECOND_MS), Some(&second));
    assert_eq!(history.selection(ns(SHOW_MS / 2)).unwrap().title, "番組1");
    assert_eq!(history.selection(ns(SHOW_MS)).unwrap().title, "番組2");
}
