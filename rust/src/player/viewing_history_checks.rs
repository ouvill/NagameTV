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
    crate::continue_watching_model::ffi::check_model(
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
    println!(
        "Viewing history: favorite commands, recent indices, resume roles and fixture loaders passed"
    );
    Ok(())
}
