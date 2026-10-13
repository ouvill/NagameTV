//! Exercise history commands and model notifications without playback hardware.
use super::ffi;
use crate::viewing_history::recordings::{Identity, Position, Source};
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QModelIndex, QString};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub(super) fn run() -> Result<(), Box<dyn std::error::Error>> {
    favorite_commands_update_rows_without_reset()?;
    let mut player = ffi::new_player();
    let channels = crate::channels::parse(
        br#"[
        {"id":10,"name":"One","type":1,"channel":{"type":"GR"}},
        {"id":20,"name":"Two","type":1,"channel":{"type":"GR"}}
    ]"#,
    )?;
    player
        .pin_mut()
        .rust_mut()
        .catalog
        .replace(channels, Some(10));
    player.pin_mut().publish_catalog("http://example.test");
    let observed = Arc::new(Mutex::new(Vec::new()));
    let notifications = observed.clone();
    let _guard = player.pin_mut().on_viewing_history_changed(move |player| {
        notifications
            .lock()
            .unwrap()
            .push(player.rust().viewing_history.favorites().to_vec());
        assert!(player.viewing_history_ready());
    });
    assert!(!player.pin_mut().select_remote_key(0, QString::from("GR")));
    assert!(!player.pin_mut().select_remote_key(13, QString::from("ALL")));
    assert!(
        !player
            .pin_mut()
            .select_remote_key(1, QString::from("invalid"))
    );
    assert!(!player.pin_mut().select_remote_key(1, QString::from("BS")));
    assert!(!player.pin_mut().toggle_favorite(QString::from("unknown")));
    assert!(!player.pin_mut().toggle_favorite(QString::from("999")));
    assert!(player.pin_mut().toggle_favorite(QString::from("20")));
    assert_eq!(observed.lock().unwrap().last(), Some(&vec![20]));
    assert!(!player.pin_mut().move_favorite(QString::from("20"), -1));
    let now = Instant::now();
    player
        .pin_mut()
        .rust_mut()
        .viewing_history
        .observe_live(Some(20), now);
    player
        .pin_mut()
        .rust_mut()
        .viewing_history
        .observe_live(Some(20), now + Duration::from_secs(10));
    player.pin_mut().publish_viewing_history();
    assert_eq!(player.rust().recent_channel_model.count(), 1);
    assert_eq!(player.rust().recent_channel_model.row_for_channel(1), 0);

    let source = Source::Epgstation {
        server: "http://example.test".into(),
        recorded_id: u64::MAX,
        video_file_id: 22,
    };
    player.pin_mut().rust_mut().viewing_history.record(
        Identity::new(source, "番組".into()),
        Position::new(120_000.0, 600_000.0).unwrap(),
    );
    crate::continue_watching_model::ffi::check_continue_model(
        player
            .pin_mut()
            .rust_mut()
            .continue_watching_model
            .pin_mut(),
    );
    player.pin_mut().publish_recording_history();
    let model = &player.rust().continue_watching_model;
    assert_eq!(model.count(), 1);
    let role = |name: &str| {
        *model
            .role_names()
            .iter()
            .find(|(_, role)| role.to_string() == name)
            .unwrap()
            .0
    };
    let index = model.model_index(0, 0, &QModelIndex::default());
    assert_eq!(
        model
            .data(&index, role("recordedId"))
            .value::<QString>()
            .unwrap()
            .to_string(),
        u64::MAX.to_string()
    );
    assert_eq!(
        model.data(&index, role("progress")).value::<f64>(),
        Some(0.2)
    );
    assert_eq!(
        model.data(&index, role("remainingMs")).value::<f64>(),
        Some(480_000.0)
    );
    assert!(
        !model
            .data(&QModelIndex::default(), role("progress"))
            .is_valid()
    );
    assert!(!player.pin_mut().resume_recording(QString::from("missing")));
    assert!(player.pin_mut().rust_mut().continue_watching_model.pin_mut().load_test(QString::from(r#"[{"recordingKey":"fixture","programName":"Title","channelName":"TV","progress":0.5,"positionMs":50000,"durationMs":100000,"remainingMs":50000,"sourceKind":"local","recordedId":"","videoFileId":""}]"#)));
    assert_eq!(player.rust().continue_watching_model.count(), 1);
    let mut source = crate::channel_model::ffi::make_channel_model();
    assert!(source.pin_mut().load_test(QString::from(r#"[{"index":7,"label":"TV","band":"GR","favorite":true,"serviceId":"9007199254740993"},{"index":8,"label":"Other","band":"BS"}]"#)));
    assert_eq!(source.count(), 2);
    assert_eq!(source.row_for_channel(7), 0);
    let directory = tempfile::tempdir()?;
    let local = directory.path().join("resume.ts");
    std::fs::write(
        &local,
        include_bytes!("../../../tests/fixtures/recording-seek.ts"),
    )?;
    let source = Source::Local { path: local };
    player.pin_mut().rust_mut().viewing_history.record(
        Identity::new(source.clone(), "Local".into()),
        Position::new(15_000.0, 60_000.0).unwrap(),
    );
    let key = player
        .rust()
        .viewing_history
        .entry(&source)
        .unwrap()
        .key
        .clone();
    assert!(player.pin_mut().resume_recording(QString::from(key)));
    assert!(player.recording_loading());
    player.pin_mut().cancel_recording_open();
    assert!(!player.recording_loading());
    println!(
        "Viewing history: favorite commands, recent indices, resume roles and fixture loaders passed"
    );
    Ok(())
}

fn favorite_commands_update_rows_without_reset() -> Result<(), Box<dyn std::error::Error>> {
    use crate::channel_model::ffi as model;
    use cxx_qt_lib::{QMap, QMapPair_QString_QVariant, QVariant};
    use std::sync::atomic::{AtomicUsize, Ordering};
    fn field(row: QVariant, name: &str) -> QVariant {
        row.value::<QMap<QMapPair_QString_QVariant>>()
            .unwrap()
            .get(&QString::from(name))
            .unwrap()
            .clone()
    }
    let mut player = ffi::new_player();
    player.pin_mut().rust_mut().catalog.replace(
        crate::channels::parse(
            br#"[{"id":10,"name":"One","type":1},{"id":20,"name":"Two","type":1}]"#,
        )?,
        Some(10),
    );
    let now = Instant::now();
    player
        .pin_mut()
        .rust_mut()
        .viewing_history
        .observe_live(Some(20), now);
    player
        .pin_mut()
        .rust_mut()
        .viewing_history
        .observe_live(Some(20), now + Duration::from_secs(10));
    player.pin_mut().publish_viewing_history();
    assert_eq!(
        player.channels(),
        player.rust().channel_model.as_ptr().cast_mut()
    );
    let mut filter = model::make_channel_filter_model();
    model::set_test_source(
        filter.pin_mut(),
        player.pin_mut().rust_mut().channel_model.pin_mut(),
    );
    model::check_source_model(player.pin_mut().rust_mut().channel_model.pin_mut());
    model::check_source_model(player.pin_mut().rust_mut().recent_channel_model.pin_mut());
    model::check_filter_model(filter.pin_mut());
    filter.pin_mut().set_favorites_only(true);
    assert_eq!(filter.count(), 0);
    let signals = model::observe_model(player.pin_mut().rust_mut().channel_model.pin_mut());
    let recent_signals =
        model::observe_model(player.pin_mut().rust_mut().recent_channel_model.pin_mut());
    let before = *player.rust().channel_model.revision();
    let changed = Arc::new(AtomicUsize::new(0));
    let observed = changed.clone();
    let _changed = player
        .pin_mut()
        .rust_mut()
        .channel_model
        .pin_mut()
        .on_changed(move |model| {
            let count = observed.fetch_add(1, Ordering::SeqCst) + 1;
            assert_eq!(*model.revision(), before + count as u32);
        });
    assert!(player.pin_mut().toggle_favorite(QString::from("20")));
    assert_eq!(
        field(player.rust().channel_model.row(1), "favorite").value::<bool>(),
        Some(true)
    );
    assert_eq!(
        field(player.rust().recent_channel_model.row(0), "favorite").value::<bool>(),
        Some(true)
    );
    assert_eq!(filter.count(), 1);
    assert_eq!(filter.row_for_channel(1), 0);
    assert!(player.pin_mut().toggle_favorite(QString::from("10")));
    assert_eq!(filter.count(), 2);
    assert_eq!(filter.row_for_channel(1), 0);
    assert!(player.pin_mut().move_favorite(QString::from("10"), 0));
    assert_eq!(filter.row_for_channel(0), 0);
    assert_eq!(filter.row_for_channel(1), 1);
    assert_eq!(
        field(player.rust().channel_model.row(0), "favoriteOrder").value::<i32>(),
        Some(0)
    );
    assert_eq!(
        field(player.rust().recent_channel_model.row(0), "favoriteOrder").value::<i32>(),
        Some(1)
    );
    assert!(player.pin_mut().toggle_favorite(QString::from("10")));
    assert_eq!(
        field(player.rust().channel_model.row(0), "favorite").value::<bool>(),
        Some(false)
    );
    assert_eq!(filter.count(), 1);
    assert_eq!(filter.row_for_channel(1), 0);
    assert!(player.pin_mut().toggle_favorite(QString::from("20")));
    assert_eq!(
        field(player.rust().channel_model.row(1), "favorite").value::<bool>(),
        Some(false)
    );
    assert_eq!(
        field(player.rust().recent_channel_model.row(0), "favorite").value::<bool>(),
        Some(false)
    );
    assert_eq!(filter.count(), 0);
    assert_eq!(signals.resets(), 0);
    assert_eq!(recent_signals.resets(), 0);
    assert_eq!(signals.persistent_row(), 0);
    assert_eq!(recent_signals.persistent_row(), 0);
    assert_eq!(signals.updates(), 7);
    for (event, row) in [1, 0, 0, 1, 0, 1, 1].into_iter().enumerate() {
        assert_eq!(signals.updated_row(event as i32), row);
        assert!(signals.favorite_roles(event as i32, 261, 262));
    }
    assert_eq!(recent_signals.updates(), 4);
    for event in 0..recent_signals.updates() {
        assert!(recent_signals.favorite_roles(event, 261, 262));
    }
    assert_eq!(changed.load(Ordering::SeqCst), 5);
    player.pin_mut().publish_viewing_history();
    assert_eq!(
        changed.load(Ordering::SeqCst),
        5,
        "unchanged projections do not notify"
    );
    assert_eq!(player.selected(), 0);
    println!(
        "Favorite updates: Player rows, exact dataChanged roles, persistent indices and recent/filter projections passed without modelReset"
    );
    Ok(())
}
