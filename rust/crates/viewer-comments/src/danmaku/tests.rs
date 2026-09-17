use super::*;
fn engine(height: f64) -> Engine {
    let mut engine = Engine::default();
    engine.set_presentation(
        Presentation::new(DisplayMode::Scroll, PlacementMode::Collision).unwrap(),
    );
    assert_eq!(
        engine.configure(
            Viewport {
                width: 640.,
                height,
                ..Viewport::default()
            },
            1.
        ),
        Some(ConfigurationChange::Preserved)
    );
    engine
}
fn spawn(engine: &mut Engine, position: Position, width: f64) -> Option<Spawn> {
    let comment = Comment::new("test", position, 0xffffff)?;
    let request = engine.prepare(comment)?;
    engine.measured(request.id.value(), width)
}
#[test]
fn dynamic_lanes_no_fixed_count_and_burst_rejection() {
    let mut e = engine(3000.);
    assert!(e.lane_count() > 64);
    for _ in 0..1000 {
        spawn(&mut e, Position::Top, 100.);
    }
    assert_eq!(e.active_count(), e.lane_count());
    e.clear();
    assert_eq!(e.active_count(), 0);
    assert!(e.lanes.iter().all(|lanes| lanes.capacity() == 0));
    assert!(spawn(&mut e, Position::Top, 100.).is_some());
    e.configure(
        Viewport {
            width: 640.,
            height: 20.,
            ..Viewport::default()
        },
        1.,
    );
    assert_eq!(e.lane_count(), 0);
    assert!(spawn(&mut e, Position::Top, 100.).is_none());
}
#[test]
fn collision_and_speed_changes_use_actual_predecessor_speed() {
    let mut e = engine(94.); // exactly one 30px lane
    let first = spawn(&mut e, Position::Right, 100.).expect("first");
    assert_eq!(first.from_x, 640.);
    assert_eq!(first.to_x, -100.);
    assert!(spawn(&mut e, Position::Right, 100.).is_none());
    e.advance_wall(Duration::from_secs(1));
    assert!(spawn(&mut e, Position::Right, 2000.).is_none());
    assert!(spawn(&mut e, Position::Right, 20.).is_some());
    // Existing trajectories retain their speed when the preference changes.
    e.configure(
        Viewport {
            width: 640.,
            height: 94.,
            ..Viewport::default()
        },
        2.,
    );
    assert!(spawn(&mut e, Position::Right, 500.).is_none());
    assert_eq!(e.active_count(), 2);
    e.advance_wall(Duration::from_secs(6));
    assert_eq!(e.active_count(), 0);
    assert!(spawn(&mut e, Position::Right, 100.).is_some());
}

#[test]
fn resize_and_fullscreen_preserve_ids_and_original_deadlines() {
    let mut e = engine(480.);
    let right = spawn(&mut e, Position::Right, 100.).expect("right");
    let top = spawn(&mut e, Position::Top, 100.).expect("top");
    let bottom = spawn(&mut e, Position::Bottom, 100.).expect("bottom");
    e.advance_wall(Duration::from_secs(1));
    e.set_paused(true);
    for (width, height, full_screen) in
        [(360., 240., false), (960., 540., true), (640., 480., false)]
    {
        assert_eq!(
            e.configure(
                Viewport {
                    width,
                    height,
                    full_screen,
                    ..e.viewport
                },
                2.
            ),
            Some(ConfigurationChange::Preserved)
        );
        assert_eq!(e.active_count(), 3);
        assert!(e.advance_wall(Duration::from_secs(10)).is_empty());
    }
    e.set_paused(false);
    let expired = e.advance_wall(Duration::from_secs(3));
    assert_eq!(expired, [top.id, bottom.id]);
    assert_eq!(e.advance_wall(Duration::from_secs(1)), [right.id]);
}

#[test]
fn resize_collision_checks_use_existing_trajectories_and_new_entry_point() {
    let mut e = engine(94.); // One row: collisions must reject the new comment.
    let original = spawn(&mut e, Position::Right, 100.).expect("original");
    e.advance_wall(Duration::from_secs(1)); // Its right edge is still at x=592.
    e.configure(
        Viewport {
            width: 320.,
            ..e.viewport
        },
        1.,
    );
    assert!(spawn(&mut e, Position::Right, 20.).is_none());
    e.configure(
        Viewport {
            width: 1000.,
            ..e.viewport
        },
        1.,
    );
    // There is now room, but a long, fast comment would catch its predecessor.
    assert!(spawn(&mut e, Position::Right, 2000.).is_none());
    let next = spawn(&mut e, Position::Right, 20.).expect("safe gap");
    assert_eq!(next.from_x, 1000.);
    assert_eq!(next.y, original.y);
    assert_eq!(e.active_count(), 2);
    assert_eq!(e.advance_wall(Duration::from_secs(4)), [original.id]);
    assert_eq!(e.advance_wall(Duration::from_secs(1)), [next.id]);
}

#[test]
fn resize_retains_measurements_but_font_changes_invalidate_them() {
    let mut e = engine(480.);
    let request = e
        .prepare(Comment::new("pending", Position::Right, 0).unwrap())
        .unwrap();
    e.configure(
        Viewport {
            width: 400.,
            ..e.viewport
        },
        1.,
    );
    let entry = e.measured(request.id.value(), 100.).expect("still valid");
    assert_eq!(entry.from_x, 400.);
    for viewport in [
        Viewport {
            font_size: 36.,
            ..e.viewport
        },
        Viewport {
            text_height: 44.,
            ..e.viewport
        },
    ] {
        let request = e
            .prepare(Comment::new("old font", Position::Right, 0).unwrap())
            .unwrap();
        let Some(ConfigurationChange::Remeasure { round, comments }) = e.configure(viewport, 1.)
        else {
            panic!("font change requires measurements");
        };
        assert_eq!(e.active_count(), 1);
        assert!(e.measured(request.id.value(), 100.).is_none());
        for id in comments {
            e.remeasured(round.value(), id.value(), 200.);
        }
    }
    let request = e
        .prepare(Comment::new("zero width", Position::Right, 0).unwrap())
        .unwrap();
    e.configure(
        Viewport {
            width: 0.,
            ..e.viewport
        },
        1.,
    );
    assert!(e.measured(request.id.value(), 100.).is_none());
}

#[test]
fn font_relayout_requires_complete_current_measurements_and_keeps_deadlines() {
    let mut e = engine(480.);
    let right = spawn(&mut e, Position::Right, 100.).unwrap();
    let top = spawn(&mut e, Position::Top, 100.).unwrap();
    let bottom = spawn(&mut e, Position::Bottom, 100.).unwrap();
    e.advance_wall(Duration::from_secs(1));
    e.set_paused(true);
    let Some(ConfigurationChange::Remeasure { round: old, .. }) = e.configure(
        Viewport {
            font_size: 36.,
            text_height: 44.,
            ..e.viewport
        },
        1.,
    ) else {
        panic!("measurements");
    };
    let Some(ConfigurationChange::Remeasure { round, comments }) = e.configure(
        Viewport {
            font_size: 72.,
            text_height: 86.,
            ..e.viewport
        },
        1.,
    ) else {
        panic!("measurements");
    };
    assert_eq!(comments, [right.id, top.id, bottom.id]);
    for id in &comments {
        assert!(e.remeasured(old.value(), id.value(), 900.).is_empty());
    }
    assert!(
        e.remeasured(round.value(), right.id.value(), f64::NAN)
            .is_empty()
    );
    assert!(
        e.remeasured(round.value(), right.id.value(), 300.)
            .is_empty()
    );
    assert!(e.remeasured(round.value(), top.id.value(), 300.).is_empty());
    let updates = e.remeasured(round.value(), bottom.id.value(), 300.);
    assert_eq!(updates.len(), 3);
    assert_eq!(updates[0].from_x, 492.);
    assert_eq!(updates[0].to_x, -300.);
    assert_eq!(updates[0].remaining, Duration::from_secs(4));
    assert_eq!(updates[1].from_x, 170.);
    assert_eq!(updates[1].remaining, Duration::from_secs(3));
    assert_eq!(updates[2].remaining, Duration::from_secs(3));
    assert_eq!(e.active_count(), 3);
    assert!(e.advance_wall(Duration::from_secs(100)).is_empty());
    e.set_paused(false);
    assert_eq!(e.advance_wall(Duration::from_secs(3)), [top.id, bottom.id]);
    assert_eq!(e.advance_wall(Duration::from_secs(1)), [right.id]);
}

#[test]
fn growing_text_reassigns_colliding_comments_without_dropping_them() {
    let mut e = engine(480.);
    let first = spawn(&mut e, Position::Right, 100.).unwrap();
    e.advance_wall(Duration::from_secs(1));
    let second = spawn(&mut e, Position::Right, 20.).unwrap();
    assert_eq!(first.y, second.y);
    let Some(ConfigurationChange::Remeasure { round, comments }) = e.configure(
        Viewport {
            font_size: 72.,
            text_height: 86.,
            ..e.viewport
        },
        1.,
    ) else {
        panic!("measurements");
    };
    assert!(spawn(&mut e, Position::Right, 20.).is_none());
    assert!(
        e.remeasured(round.value(), comments[1].value(), 400.)
            .is_empty()
    );
    let updates = e.remeasured(round.value(), comments[0].value(), 400.);
    assert_eq!(updates.len(), 2);
    assert_ne!(updates[0].y, updates[1].y);
    assert_eq!(updates[1].y - updates[0].y, e.viewport.spacing());
    assert_eq!(e.active_count(), 2);
    assert!(spawn(&mut e, Position::Right, 20.).is_some());
    e.clear();
    assert!(
        e.remeasured(round.value(), first.id.value(), 500.)
            .is_empty()
    );
    assert_eq!(e.active_count(), 0);
}

#[test]
fn expiration_during_font_measurement_cannot_restore_expired_comments() {
    let mut e = engine(480.);
    let fixed = spawn(&mut e, Position::Top, 100.).unwrap();
    let flow = spawn(&mut e, Position::Right, 100.).unwrap();
    let Some(ConfigurationChange::Remeasure { round, .. }) = e.configure(
        Viewport {
            font_size: 36.,
            ..e.viewport
        },
        1.,
    ) else {
        panic!("measurements");
    };
    assert_eq!(e.advance_wall(Duration::from_secs(4)), [fixed.id]);
    let updates = e.remeasured(round.value(), flow.id.value(), 200.);
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].id, flow.id);
    assert_eq!(updates[0].remaining, Duration::from_secs(1));
    assert!(
        e.remeasured(round.value(), fixed.id.value(), 200.)
            .is_empty()
    );
    assert_eq!(e.active_count(), 1);
}

#[test]
fn shrinking_below_occupied_height_retains_rows_until_expiration() {
    for position in [Position::Right, Position::Top, Position::Bottom] {
        let mut e = engine(480.);
        let original: Vec<_> = (0..e.lane_count())
            .map(|_| spawn(&mut e, position, 100.).expect("row"))
            .collect();
        e.configure(
            Viewport {
                height: 94.,
                ..e.viewport
            },
            1.,
        );
        assert_eq!(e.active_count(), original.len());
        assert_eq!(e.lane_count(), 1);
        assert_eq!(e.origin(position), 40.);
        assert!(spawn(&mut e, position, 100.).is_none());
        e.advance_wall(Duration::from_millis(200));
        assert!(spawn(&mut e, position, 100.).is_none());
        e.configure(
            Viewport {
                height: 480.,
                ..e.viewport
            },
            1.,
        );
        e.advance_wall(Duration::from_millis(200));
        assert_rows_stay_in_frame(&e);
        assert_eq!(e.active_count(), original.len());
        let expired = e.advance_wall(original[0].lifetime - Duration::from_millis(400));
        assert_eq!(
            expired,
            original.iter().map(|entry| entry.id).collect::<Vec<_>>()
        );
    }
}
#[test]
fn formats_durations_pause_and_lifetime_are_core_owned() {
    let mut e = engine(480.);
    let right = spawn(&mut e, Position::Right, 100.).expect("right");
    let top = spawn(&mut e, Position::Top, 100.).expect("top");
    let bottom = spawn(&mut e, Position::Bottom, 100.).expect("bottom");
    assert_eq!(right.lifetime, Duration::from_secs(5));
    assert_eq!(top.lifetime, Duration::from_secs(4));
    assert_eq!(top.from_x, 270.);
    assert_eq!(top.from_x, top.to_x);
    assert!(e.origin(Position::Bottom) + bottom.y > e.origin(Position::Top) + top.y);
    e.set_paused(true);
    assert!(e.advance_wall(Duration::from_secs(100)).is_empty());
    assert!(spawn(&mut e, Position::Top, 100.).is_none());
    e.set_paused(false);
    let expired = e.advance_wall(Duration::from_secs(4));
    assert_eq!(expired.len(), 2);
    assert!(expired.contains(&top.id));
    assert_eq!(e.advance_wall(Duration::from_secs(1)), [right.id]);
    e.configure(
        Viewport {
            width: 640.,
            height: 480.,
            full_screen: true,
            ..Viewport::default()
        },
        2.,
    );
    assert_eq!(
        spawn(&mut e, Position::Right, 100.)
            .expect("right")
            .lifetime,
        Duration::from_secs(4)
    );
    assert_eq!(
        spawn(&mut e, Position::Top, 100.).expect("top").lifetime,
        Duration::from_secs(3)
    );
}
#[test]
fn stale_measurements_and_invalid_geometry_cannot_allocate_entries() {
    let mut e = engine(480.);
    let a = e
        .prepare(Comment::new("a", Position::Right, 0).expect("comment"))
        .expect("request");
    e.clear();
    let b = e
        .prepare(Comment::new("b", Position::Right, 0).expect("comment"))
        .expect("request");
    assert!(e.measured(a.id.value(), 100.).is_none());
    assert!(e.measured(b.id.value(), f64::NAN).is_none());
    assert_eq!(e.active_count(), 0);
    assert_eq!(
        e.configure(
            Viewport {
                width: f64::INFINITY,
                ..Viewport::default()
            },
            1.
        ),
        None
    );
    assert_eq!(e.configure(Viewport::default(), 0.), None);
    assert!(Comment::new("x", Position::Top, 0x1000000).is_none());
}
#[test]
fn timeline_sort_seek_pause_replacement_and_validation() -> Result<(), LoadError> {
    let mut e = engine(480.);
    let data = parse_timeline(
        r##"[{"time":3,"text":"later","type":"top","color":"#123abc"},{"time":1,"text":"first"},{"time":3,"text":"same","type":2,"color":0}]"##,
    )?;
    e.load(data);
    e.set_position(Duration::from_secs(1));
    assert_eq!(e.next_due().expect("first").comment.text.as_ref(), "first");
    e.set_position(Duration::from_millis(1010));
    assert!(e.next_due().is_none());
    e.seek(Duration::from_secs(3));
    e.set_position(Duration::from_millis(3010));
    e.set_paused(true);
    assert_eq!(
        e.next_due()
            .expect("restored in flight")
            .comment
            .text
            .as_ref(),
        "first"
    );
    let later = e.next_due().expect("later");
    assert_eq!(later.comment.position, Position::Top);
    assert_eq!(later.comment.color.rgb(), 0x123abc);
    assert_eq!(
        e.next_due().expect("same time").comment.text.as_ref(),
        "same"
    );
    e.set_paused(false);
    assert!(e.set_position(Duration::ZERO));
    e.set_position(Duration::from_secs(2));
    assert_eq!(
        e.next_due().expect("rewound").comment.text.as_ref(),
        "first"
    );
    e.seek(Duration::from_secs(100));
    assert!(e.next_due().is_none());
    for input in [
        r#"[{"time":-1,"text":"bad"}]"#,
        r#"[{"time":1,"text":"bad","type":3}]"#,
        r#"[{"time":1,"text":"bad","color":16777216}]"#,
    ] {
        assert!(parse_timeline(input).is_err());
        assert_eq!(e.timeline_count(), 3);
    }
    e.reset();
    assert_eq!(e.timeline_count(), 0);
    assert_eq!(e.timeline.capacity(), 0);
    Ok(())
}

#[test]
fn measurement_tokens_never_wrap_or_reuse_on_clear() {
    let mut e = engine(480.);
    e.serial = u32::MAX - 1;
    let request = e
        .prepare(Comment::new("last", Position::Right, 0).expect("comment"))
        .expect("request");
    assert_eq!(request.id.value(), u32::MAX);
    e.clear();
    assert!(
        e.prepare(Comment::new("overflow", Position::Right, 0).expect("comment"))
            .is_none()
    );
    assert!(e.measured(request.id.value(), 100.).is_none());
}

#[test]
fn hidden_timeline_keeps_cursor_but_no_pending_or_visible_objects() -> Result<(), LoadError> {
    let mut e = engine(480.);
    e.load(parse_timeline(
        r#"[{"time":1,"text":"hidden"},{"time":3,"text":"visible"}]"#,
    )?);
    e.set_visible(false);
    e.set_position(Duration::from_secs(2));
    let comment = e.next_due().expect("due hidden comment");
    assert!(e.prepare_timed(comment).is_none());
    assert!(e.pending.is_none());
    assert_eq!(e.active_count(), 0);
    e.set_visible(true);
    e.set_position(Duration::from_secs(4));
    assert_eq!(
        e.next_due().expect("new comment").comment.text.as_ref(),
        "visible"
    );
    Ok(())
}

#[test]
fn controls_move_rows_without_resetting_deadlines_or_occupancy() {
    for position in [Position::Right, Position::Top, Position::Bottom] {
        let mut e = engine(480.);
        let original = spawn(&mut e, position, 100.).expect("original");
        e.advance_wall(Duration::from_secs(1));
        let reserved = Viewport {
            width: 640.,
            height: 480.,
            title_overlaps: true,
            title_bottom: 130.,
            controls_overlaps: true,
            controls_top: 350.,
            ..Viewport::default()
        };
        assert_eq!(
            e.configure(reserved, 1.),
            Some(ConfigurationChange::Preserved)
        );
        assert_eq!(e.active_count(), 1);
        if position == Position::Bottom {
            assert!(e.origin(position) < 480. - 24. - reserved.spacing());
        } else {
            assert_eq!(e.origin(position), reserved.top());
        }
        let next = spawn(&mut e, position, 100.).expect("new placement");
        assert!(e.origin(position) + next.y >= reserved.top());
        assert!(e.origin(position) + next.y + reserved.spacing() <= reserved.bottom());
        assert_eq!(
            e.configure(
                Viewport {
                    title_overlaps: false,
                    ..reserved
                },
                1.
            ),
            Some(ConfigurationChange::Preserved)
        );
        assert_eq!(e.active_count(), 2);
        // Fixed comments never reuse an occupied physical row after hiding UI.
        if position != Position::Right {
            let mut occupied = vec![original.y, next.y];
            while let Some(entry) = spawn(&mut e, position, 100.) {
                assert!(!occupied.contains(&entry.y));
                occupied.push(entry.y);
            }
        }
        // Showing UI did not restart the first comment's lifetime.
        let expired = e.advance_wall(original.lifetime - Duration::from_secs(1));
        assert!(expired.contains(&original.id));
        assert!(!expired.contains(&next.id));
    }
}

fn assert_rows_stay_in_frame(e: &Engine) {
    for (kind, lanes) in e.lanes.iter().enumerate() {
        let (low, high) = e.origins[kind].envelope(e.clock);
        for (row, entries) in lanes.iter().enumerate() {
            if entries.is_empty() {
                continue;
            }
            let offset = row as f64 * e.viewport.spacing() * if kind == 2 { -1. } else { 1. };
            assert!(low + offset >= 40. - 1e-8);
            assert!(high + offset + e.viewport.spacing() <= e.viewport.height - 24. + 1e-8);
        }
    }
}

#[test]
fn full_height_preserves_spacing_and_entries_when_controls_cannot_fit() {
    for position in [Position::Right, Position::Top, Position::Bottom] {
        let mut e = engine(480.);
        let capacity = e.lane_count();
        for _ in 0..capacity {
            assert!(spawn(&mut e, position, 100.).is_some());
        }
        let initial = e.origin(position);
        let reserved = Viewport {
            width: 640.,
            height: 480.,
            title_overlaps: true,
            title_bottom: 130.,
            controls_overlaps: true,
            controls_top: 300.,
            ..Viewport::default()
        };
        assert_eq!(
            e.configure(reserved, 1.),
            Some(ConfigurationChange::Preserved)
        );
        assert_eq!(e.active_count(), capacity);
        assert_eq!(e.origin(position), initial);
        assert!(spawn(&mut e, position, 100.).is_none());
        assert_rows_stay_in_frame(&e);
        assert_eq!(
            e.configure(
                Viewport {
                    title_overlaps: false,
                    controls_overlaps: false,
                    ..reserved
                },
                1.
            ),
            Some(ConfigurationChange::Preserved)
        );
        assert_eq!(e.active_count(), capacity);
        assert_rows_stay_in_frame(&e);
    }
}

#[test]
fn new_rows_during_animation_and_rapid_reversal_never_overflow() {
    for position in [Position::Top, Position::Bottom] {
        let mut e = engine(480.);
        spawn(&mut e, position, 100.).expect("first");
        let reserved = Viewport {
            width: 640.,
            height: 480.,
            title_overlaps: true,
            title_bottom: 130.,
            controls_overlaps: true,
            controls_top: 350.,
            ..Viewport::default()
        };
        for _ in 0..4 {
            e.configure(reserved, 1.);
            while spawn(&mut e, position, 100.).is_some() {}
            assert_rows_stay_in_frame(&e);
            e.advance_wall(Duration::from_millis(30));
            e.configure(
                Viewport {
                    title_overlaps: false,
                    controls_overlaps: false,
                    ..reserved
                },
                1.,
            );
            while spawn(&mut e, position, 100.).is_some() {}
            assert_rows_stay_in_frame(&e);
        }
        e.advance_wall(Duration::from_millis(180));
        while spawn(&mut e, position, 100.).is_some() {}
        assert_eq!(e.active_count(), e.lane_count());
        assert_rows_stay_in_frame(&e);
    }
}

#[test]
fn small_view_with_large_text_keeps_old_rows_and_rejects_new_when_ui_fills_space() {
    let mut e = Engine::default();
    e.set_presentation(Presentation::new(DisplayMode::Scroll, PlacementMode::Collision).unwrap());
    let viewport = Viewport {
        width: 400.,
        height: 240.,
        text_height: 44.,
        font_size: 36.,
        ..Viewport::default()
    };
    e.configure(viewport, 1.);
    for _ in 0..e.lane_count() {
        spawn(&mut e, Position::Top, 100.).expect("fits");
    }
    let count = e.active_count();
    e.configure(
        Viewport {
            title_overlaps: true,
            title_bottom: 130.,
            controls_overlaps: true,
            controls_top: 150.,
            ..viewport
        },
        1.,
    );
    assert_eq!(e.lane_count(), 0);
    assert_eq!(e.active_count(), count);
    assert!(spawn(&mut e, Position::Top, 100.).is_none());
    assert_rows_stay_in_frame(&e);
}

fn due_spawn(e: &mut Engine, width: f64) -> Option<Spawn> {
    let record = e.next_due()?;
    let measured = e.prepare_timed(record)?;
    e.measured(measured.id.value(), width)
}
#[test]
fn paused_seek_restores_scroll_and_fixed_positions_with_remaining_lifetimes() {
    let mut e = engine(480.);
    e.load(parse_timeline(r#"[{"time":1,"text":"scroll"},{"time":1,"text":"top","type":"top"},{"time":1,"text":"bottom","type":"bottom"}]"#).unwrap());
    e.set_paused(true);
    e.seek(Duration::from_secs(3));
    let scroll = due_spawn(&mut e, 100.).unwrap();
    assert_eq!(scroll.from_x, 640. - (640. + 100.) * 2. / 5.);
    assert_eq!(scroll.to_x, -100.);
    assert_eq!(scroll.lifetime, Duration::from_secs(3));
    for _ in 0..2 {
        let fixed = due_spawn(&mut e, 100.).unwrap();
        assert_eq!(fixed.from_x, 270.);
        assert_eq!(fixed.lifetime, Duration::from_secs(2));
    }
    let before = e.positions();
    assert!(e.advance_wall(Duration::from_secs(60)).is_empty());
    assert_eq!(e.positions(), before);
    e.set_paused(false);
    assert!(e.next_due().is_none());
    e.set_position(Duration::from_secs(5));
    assert_eq!(e.advance_wall(Duration::ZERO).len(), 2);
    e.set_position(Duration::from_secs(6));
    assert_eq!(e.advance_wall(Duration::ZERO), vec![scroll.id]);
    e.seek(Duration::from_secs(6));
    assert!(due_spawn(&mut e, 100.).is_none());
}
#[test]
fn restored_motion_matches_normal_playback_and_old_measurement_tokens_are_rejected() {
    let records = parse_timeline(r#"[{"time":1,"text":"moving"}]"#).unwrap();
    let mut normal = engine(480.);
    normal.load(records.clone());
    normal.set_position(Duration::from_secs(1));
    due_spawn(&mut normal, 200.).unwrap();
    normal.set_position(Duration::from_millis(3500));
    let normal_x = normal.positions()[0].1;
    let mut restored = engine(480.);
    restored.load(records);
    restored.seek(Duration::from_millis(3500));
    let pending = restored.next_due().unwrap();
    let stale = restored.prepare_timed(pending).unwrap();
    restored.seek(Duration::from_millis(3500));
    let spawn = due_spawn(&mut restored, 200.).unwrap();
    assert_eq!(spawn.from_x, normal_x);
    assert_eq!(spawn.lifetime, Duration::from_millis(2500));
    assert!(restored.measured(stale.id.value(), 200.).is_none());
}
#[test]
fn overlapping_fetches_keep_active_ids_and_late_comments_start_midflight() {
    let first = parse_timeline(r#"[{"id":"one","time":1,"text":"first"}]"#).unwrap();
    let both = parse_timeline(
        r#"[{"id":"one","time":1,"text":"first"},{"id":"two","time":2,"text":"late"}]"#,
    )
    .unwrap();
    let mut e = engine(480.);
    e.load(first);
    e.seek(Duration::from_secs(3));
    let first = due_spawn(&mut e, 100.).unwrap();
    e.replace(both.clone());
    let late = due_spawn(&mut e, 200.).unwrap();
    assert_eq!(late.lifetime, Duration::from_secs(4));
    assert!(e.positions().iter().any(|(id, _)| *id == first.id));
    e.replace(both);
    assert!(e.next_due().is_none());
    assert_eq!(e.active_count(), 2);
    for lane in &e.lanes[0] {
        for (index, entry) in lane.iter().enumerate() {
            for other in lane.iter().skip(index + 1) {
                assert!(entry.separate_from(other, e.clock));
            }
        }
    }
}
