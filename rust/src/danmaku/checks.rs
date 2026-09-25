//! Hardware-free checks of typed delivery and synchronous measurement callbacks.
use super::*;
use crate::features::comments::replay::Timeline;
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut controller = ffi::new_controller();
    assert!(
        controller
            .pin_mut()
            .configure(1280., 720., 30., 24., false, 0., false, 720., false, 1.,)
    );
    let clears = Arc::new(AtomicUsize::new(0));
    let observed = clears.clone();
    controller
        .pin_mut()
        .on_cleared(move |_| {
            observed.fetch_add(1, Ordering::Relaxed);
        })
        .release();
    controller
        .pin_mut()
        .on_measure_requested(|controller, token, _, _| {
            controller.measured(token, 160.);
        })
        .release();
    let timeline = Timeline::fixture(danmaku_core::parse_timeline(
        r#"[{"id":"1","time":1,"text":"再生位置のコメント"}]"#,
    )?);
    let position = Duration::from_secs(1);
    controller
        .pin_mut()
        .apply_playback_timeline(timeline.clone(), position);
    assert_eq!(*controller.timeline_count(), 1);
    assert_eq!(*controller.active_count(), 1);
    assert_eq!(clears.load(Ordering::Relaxed), 1);
    controller
        .pin_mut()
        .apply_playback_timeline(timeline.clone(), position);
    assert_eq!(*controller.active_count(), 1);
    assert_eq!(
        clears.load(Ordering::Relaxed),
        1,
        "repeat delivery preserves active labels"
    );

    let empty = Timeline::default();
    controller
        .pin_mut()
        .apply_playback_timeline(empty.clone(), position);
    assert_eq!(*controller.active_count(), 0);
    assert_eq!(*controller.timeline_count(), 0);
    assert_eq!(clears.load(Ordering::Relaxed), 2);
    controller
        .pin_mut()
        .apply_playback_timeline(empty, position);
    assert_eq!(clears.load(Ordering::Relaxed), 2);
    controller
        .pin_mut()
        .apply_playback_timeline(Timeline::default(), position);
    assert_eq!(
        clears.load(Ordering::Relaxed),
        3,
        "another empty source still resets"
    );

    controller
        .pin_mut()
        .apply_playback_timeline(timeline.clone(), position);
    assert!(controller.pin_mut().load_timeline(QString::from("[]")));
    controller
        .pin_mut()
        .apply_playback_timeline(timeline.clone(), position);
    assert_eq!(
        *controller.timeline_count(),
        1,
        "import invalidates the delivered snapshot"
    );
    controller.pin_mut().reset();
    controller
        .pin_mut()
        .apply_playback_timeline(timeline, position);
    assert_eq!(
        *controller.active_count(),
        1,
        "reset allows a fresh delivery"
    );
    Ok(())
}
