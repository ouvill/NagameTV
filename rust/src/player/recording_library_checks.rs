//! Real model/notification/persistence checks without display or playback devices.
use super::ffi;
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QModelIndex, QString};
use serde_json::json;
use std::{
    thread,
    time::{Duration, Instant},
};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

pub(super) fn run() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()?;
    let server = runtime.block_on(MockServer::start());
    let record = json!({"id":u64::MAX,"name":"録画一覧","startAt":1000,"endAt":2000,
        "isRecording":false,"videoFiles":[{"id":123,"type":"ts","size":1880},{"id":u64::MAX,"type":"encoded","size":2048,"name":"H.264","filename":"番組.mp4"}]});
    runtime.block_on(
        Mock::given(method("GET"))
            .and(path("/api/recorded"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"records":[record],"total":1})),
            )
            .expect(1)
            .mount(&server),
    );
    runtime.block_on(
        Mock::given(method("GET"))
            .and(path("/denied/api/recorded"))
            .respond_with(ResponseTemplate::new(401))
            .expect(1)
            .mount(&server),
    );
    let directory = tempfile::tempdir()?;
    let settings_path = directory.path().join("settings.toml");
    let mut player = ffi::new_player();
    player.pin_mut().rust_mut().preferences =
        crate::settings::Loaded::open(settings_path.clone())?.activate(None, None);
    crate::recording_model::ffi::check_model(player.pin_mut().rust_mut().recording_model.pin_mut());
    crate::video_file_model::ffi::check_video_model(
        player.pin_mut().rust_mut().video_file_model.pin_mut(),
    );
    crate::subtitle_model::ffi::check_subtitle_model(
        player.pin_mut().rust_mut().subtitle_model.pin_mut(),
    );
    player
        .pin_mut()
        .rust_mut()
        .subtitle_model
        .pin_mut()
        .replace(
            vec![
                crate::media_subtitles::Track {
                    id: "stable-id".into(),
                    title: "日本語".into(),
                    selected: true,
                },
                crate::media_subtitles::Track {
                    id: "external".into(),
                    title: "外部.ass".into(),
                    selected: false,
                },
            ]
            .into(),
        );
    let subtitles = &player.rust().subtitle_model;
    assert_eq!(subtitles.count(), 2);
    let selected_role = subtitles
        .role_names()
        .iter()
        .find(|(_, name)| name.to_string() == "trackSelected")
        .map(|(id, _)| *id)
        .unwrap();
    assert_eq!(
        subtitles
            .data(
                &subtitles.model_index(0, 0, &QModelIndex::default()),
                selected_role
            )
            .value::<bool>(),
        Some(true)
    );
    player.pin_mut().publish_subtitle_tracks();
    assert_eq!(player.rust().subtitle_model.count(), 0);
    assert!(!player.pin_mut().select_subtitle(QString::from("stable-id")));
    let _notification = player.pin_mut().on_epgstation_changed(|player| {
        assert_eq!(
            player.rust().recording_model.count() as usize,
            player.rust().recording_library.rows().len()
        );
        assert_eq!(
            player.epgstation_loaded(),
            player.rust().recording_library.loaded()
        );
        if player.epgstation_busy() {
            assert!(!player.epgstation_previous());
            assert!(!player.epgstation_next());
        }
    });
    assert!(
        player
            .pin_mut()
            .browse_epgstation(QString::from(server.uri()), QString::default())
    );
    assert!(
        player.epgstation_server().is_empty(),
        "pending connections are not committed"
    );
    fn finish(player: &mut cxx::UniquePtr<ffi::Player>) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while player.epgstation_busy() {
            player.pin_mut().poll_recording_library();
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        }
    }
    finish(&mut player);
    assert_eq!(player.epgstation_server().to_string(), server.uri());
    assert!(player.epgstation_error().is_empty());
    let model = &player.rust().recording_model;
    assert_eq!(model.count(), 1);
    let index = model.model_index(0, 0, &QModelIndex::default());
    let id_role = model
        .role_names()
        .iter()
        .find(|(_, name)| name.to_string() == "recordedId")
        .map(|(role, _)| *role)
        .unwrap();
    assert_eq!(
        model
            .data(&index, id_role)
            .value::<QString>()
            .unwrap()
            .to_string(),
        u64::MAX.to_string()
    );
    assert!(!model.data(&QModelIndex::default(), id_role).is_valid());
    assert!(
        !player.pin_mut().play_epgstation(QString::from("123")),
        "video ID cannot select a recording"
    );
    assert!(
        player
            .pin_mut()
            .choose_epgstation(QString::from(u64::MAX.to_string()))
    );
    let files = &player.rust().video_file_model;
    assert_eq!(files.count(), 2);
    let video_role = files
        .role_names()
        .iter()
        .find(|(_, name)| name.to_string() == "videoId")
        .map(|(role, _)| *role)
        .unwrap();
    assert_eq!(
        files
            .data(
                &files.model_index(1, 0, &QModelIndex::default()),
                video_role
            )
            .value::<QString>()
            .unwrap()
            .to_string(),
        u64::MAX.to_string()
    );
    assert!(
        !player
            .pin_mut()
            .play_epgstation_file(QString::from(u64::MAX.to_string()), QString::from("999"))
    );
    let saved = crate::settings::Loaded::open(settings_path.clone())?;
    assert_eq!(saved.preferences().epgstation_server, server.uri());
    assert!(
        saved.preferences().server.is_empty(),
        "Mirakurun settings remain independent"
    );
    assert!(player.pin_mut().browse_epgstation(
        QString::from(format!("{}/denied", server.uri())),
        QString::default()
    ));
    finish(&mut player);
    assert!(!player.epgstation_error().is_empty());
    assert_eq!(player.rust().recording_model.count(), 0);
    assert_eq!(player.rust().video_file_model.count(), 0);
    assert_eq!(
        crate::settings::Loaded::open(settings_path)?
            .preferences()
            .epgstation_server,
        server.uri()
    );
    assert!(
        !player
            .pin_mut()
            .browse_epgstation(QString::from("file:///tmp"), QString::default())
    );
    runtime.block_on(server.verify());
    println!(
        "EPGStation: real model, stable IDs, notification coherence and verified settings passed"
    );
    Ok(())
}
