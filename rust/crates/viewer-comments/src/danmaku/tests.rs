use super::*;
fn engine(height: f64) -> Engine {
    let mut engine = Engine::default();
    assert_eq!(
        engine.configure(
            Viewport {
                width: 640.,
                height,
                ..Viewport::default()
            },
            1.
        ),
        Some(true)
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
    assert!(e.next_due().is_none());
    e.set_position(Duration::from_millis(1010));
    assert_eq!(e.next_due().expect("first").text.as_ref(), "first");
    assert!(e.next_due().is_none());
    e.seek(Duration::from_secs(3));
    e.set_position(Duration::from_millis(3010));
    e.set_paused(true);
    assert!(e.next_due().is_none());
    e.set_paused(false);
    let later = e.next_due().expect("later");
    assert_eq!(later.position, Position::Top);
    assert_eq!(later.color.rgb(), 0x123abc);
    assert_eq!(e.next_due().expect("same time").text.as_ref(), "same");
    assert!(e.set_position(Duration::ZERO));
    e.set_position(Duration::from_secs(2));
    assert_eq!(e.next_due().expect("rewound").text.as_ref(), "first");
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
    assert!(e.prepare(comment).is_none());
    assert!(e.pending.is_none());
    assert_eq!(e.active_count(), 0);
    e.set_visible(true);
    e.set_position(Duration::from_secs(4));
    assert_eq!(e.next_due().expect("new comment").text.as_ref(), "visible");
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
        assert_eq!(e.configure(reserved, 1.), Some(false));
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
            Some(false)
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
        assert_eq!(e.configure(reserved, 1.), Some(false));
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
            Some(false)
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
