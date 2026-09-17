use super::*;

fn engine(display: DisplayMode, placement: PlacementMode) -> Engine {
    let mut engine = Engine::default();
    engine.set_presentation(Presentation::new(display, placement).unwrap());
    engine
        .configure(
            Viewport {
                width: 640.,
                height: 480.,
                ..Viewport::default()
            },
            1.,
        )
        .unwrap();
    engine
}
fn record(id: &str, time: f64, position: Position) -> TimedComment {
    TimedComment {
        id: id.into(),
        time: seconds(time).unwrap(),
        comment: Comment::new("コメント", position, 0xffffff).unwrap(),
    }
}
fn timed(engine: &mut Engine, record: TimedComment, width: f64) -> Option<Spawn> {
    let measurement = engine.prepare_timed(record)?;
    engine.measured(measurement.id.value(), width)
}
fn direct(engine: &mut Engine, width: f64) -> Option<Spawn> {
    let measurement = engine.prepare(Comment::new("test", Position::Right, 0xffffff).unwrap())?;
    engine.measured(measurement.id.value(), width)
}

#[test]
fn default_admission_limits_bursts_without_comparing_glyphs() {
    let mut e = engine(DisplayMode::Scroll, PlacementMode::Sequential);
    assert!(direct(&mut e, 2000.).is_some());
    for _ in 0..1000 {
        assert!(direct(&mut e, 20.).is_none());
    }
    assert_eq!(e.active_count(), 1);
    e.advance_wall(Duration::from_secs(1));
    assert!(direct(&mut e, 2000.).is_some());
    assert_eq!(e.active_count(), 2);
}

#[test]
fn width_does_not_affect_velocity_and_overlap_is_allowed() {
    let mut e = engine(DisplayMode::Scroll, PlacementMode::Sequential);
    e.configure(
        Viewport {
            height: 94.,
            ..e.viewport
        },
        1.,
    )
    .unwrap();
    let first = direct(&mut e, 2000.).unwrap();
    e.advance_wall(Duration::from_secs(1));
    let second = direct(&mut e, 20.).unwrap();
    assert_eq!(first.y, second.y, "one row, no collision-based relocation");
    let before = e.visuals();
    e.advance_wall(Duration::from_secs(1));
    let after = e.visuals();
    assert_eq!(before[0].1.x - after[0].1.x, before[1].1.x - after[1].1.x);
    assert!(
        before[0].1.x + first.width > before[1].1.x,
        "overlap is deliberately permitted"
    );
    assert!(first.lifetime <= MAX_LIFETIME);
}

#[test]
fn pop_rises_slows_and_falls_with_bounded_tilt_and_fade() {
    let mut e = engine(DisplayMode::Pop, PlacementMode::Sequential);
    let born = direct(&mut e, 160.).unwrap();
    assert!(born.y >= e.viewport.height);
    assert!(born.rotation.abs() <= POP_MAX_ROTATION_DEGREES);
    let quarter = born.lifetime / 4;
    e.advance_wall(quarter);
    let rising = e.visuals()[0].1;
    e.advance_wall(quarter);
    let top = e.visuals()[0].1;
    e.advance_wall(quarter);
    let falling = e.visuals()[0].1;
    assert!(rising.y < born.y && top.y < rising.y && falling.y > top.y);
    assert!(top.y > 0. && top.y < e.viewport.height);
    assert_eq!(top.rotation, born.rotation);
    assert_eq!(top.opacity, 1.);
    assert!(falling.opacity < top.opacity);
    assert_eq!(e.advance_wall(born.lifetime), [born.id]);
}

#[test]
fn timed_pop_seek_restores_the_same_trajectory_and_paused_clock() {
    let mut normal = engine(DisplayMode::Pop, PlacementMode::Random);
    normal.load(vec![record("stable", 1., Position::Top)]);
    normal.set_position(seconds(1.).unwrap());
    let due = normal.next_due().unwrap();
    timed(&mut normal, due, 160.).unwrap();
    normal.set_position(seconds(3.).unwrap());
    let expected = normal.visuals()[0].1;
    let mut restored = engine(DisplayMode::Pop, PlacementMode::Random);
    restored.load(vec![record("stable", 1., Position::Top)]);
    restored.seek(seconds(3.).unwrap());
    restored.set_paused(true);
    let due = restored.next_due().unwrap();
    let spawn = timed(&mut restored, due, 160.).unwrap();
    assert_eq!(
        spawn.display,
        DisplayMode::Pop,
        "fixed mail commands also pop"
    );
    assert_eq!(restored.visuals()[0].1, expected);
    restored.advance_wall(Duration::from_secs(10));
    assert_eq!(restored.visuals()[0].1, expected);
}

#[test]
fn pop_choices_have_reproducible_variation() {
    let mut rotations = Vec::new();
    for n in 0..20 {
        let mut e = engine(DisplayMode::Pop, PlacementMode::Random);
        e.load(Vec::new());
        e.set_position(seconds(2.).unwrap());
        let a = timed(&mut e, record(&n.to_string(), 1., Position::Right), 100.).unwrap();
        rotations.push(a.rotation);
        assert!(a.from_x.is_finite() && a.y.is_finite());
    }
    assert!(rotations.iter().any(|rotation| *rotation < 0.));
    assert!(rotations.iter().any(|rotation| *rotation > 0.));
    assert!(rotations.windows(2).all(|pair| pair[0] != pair[1]));
}

#[test]
fn switching_presentation_invalidates_old_measurements() {
    let mut e = engine(DisplayMode::Scroll, PlacementMode::Sequential);
    let pending = e
        .prepare(Comment::new("old", Position::Right, 0).unwrap())
        .unwrap();
    e.set_presentation(Presentation::new(DisplayMode::Pop, PlacementMode::Random).unwrap());
    assert!(e.measured(pending.id.value(), 120.).is_none());
    assert_eq!(direct(&mut e, 120.).unwrap().display, DisplayMode::Pop);
}

#[test]
fn font_resize_preserves_speed_deadline_and_does_not_search_for_a_free_row() {
    let mut e = engine(DisplayMode::Scroll, PlacementMode::Sequential);
    let first = direct(&mut e, 100.).unwrap();
    e.advance_wall(Duration::from_secs(1));
    let before = e.visuals()[0].1;
    let ConfigurationChange::Remeasure { round, .. } = e
        .configure(
            Viewport {
                font_size: 72.,
                ..e.viewport
            },
            1.,
        )
        .unwrap()
    else {
        panic!("remeasure");
    };
    let updates = e.remeasured(round.value(), first.id.value(), 700.);
    assert_eq!(updates[0].y, first.y);
    assert_eq!(
        updates[0].remaining,
        first.lifetime - Duration::from_secs(1)
    );
    e.advance_wall(Duration::from_secs(1));
    assert_eq!(
        before.x - e.visuals()[0].1.x,
        e.viewport.width / SCROLL_SECONDS
    );
}

#[test]
fn resize_pop_and_hide_release_display_without_late_resurrection() {
    let mut e = engine(DisplayMode::Pop, PlacementMode::Sequential);
    let first = direct(&mut e, 200.).unwrap();
    e.advance_wall(first.lifetime / 2);
    e.set_paused(true);
    let ConfigurationChange::Remeasure { round, .. } = e
        .configure(
            Viewport {
                width: 360.,
                height: 240.,
                font_size: 72.,
                ..e.viewport
            },
            1.,
        )
        .unwrap()
    else {
        panic!("remeasure");
    };
    e.remeasured(round.value(), first.id.value(), 600.);
    let visual = e.visuals()[0].1;
    assert!(visual.x.is_finite() && visual.y.is_finite());
    e.set_visible(false);
    assert!(
        e.remeasured(round.value(), first.id.value(), 200.)
            .is_empty()
    );
    assert_eq!(e.active_count(), 0);
}

#[test]
fn unknown_and_unavailable_choices_are_rejected_at_api_boundary() {
    assert!(DisplayMode::parse("unknown").is_none());
    assert!(PlacementMode::parse("unknown").is_none());
    #[cfg(not(feature = "evaluation-collision-layout"))]
    assert!(PlacementMode::parse("collision").is_none());
    #[cfg(feature = "evaluation-collision-layout")]
    assert!(Presentation::new(DisplayMode::Pop, PlacementMode::Collision).is_none());
}
