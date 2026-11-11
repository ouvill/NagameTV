//! Real Player/model refresh checks with QCoreApplication and local HTTP only.
use super::*;
use crate::viewing_history::recordings::{Identity, Position, Source};
use std::sync::{Arc, Mutex};
use wiremock::matchers::query_param;

fn row(id: u64, video: u64, name: &str) -> serde_json::Value {
    json!({"id":id,"name":name,"startAt":1000,"endAt":601000,
        "isRecording":false,"videoFiles":[{"id":video,"type":"encoded","size":1234}]})
}

fn finish(player: &mut cxx::UniquePtr<ffi::Player>) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while player.epgstation_busy() {
        player.pin_mut().poll_recording_library();
        assert!(Instant::now() < deadline, "refresh did not finish");
        thread::sleep(Duration::from_millis(1));
    }
}

fn value(player: &ffi::Player, row: i32, role: &str) -> cxx_qt_lib::QVariant {
    let model = &player.rust().recording_model;
    let role = *model
        .role_names()
        .iter()
        .find(|(_, name)| name.to_string() == role)
        .unwrap()
        .0;
    model.data(&model.model_index(row, 0, &QModelIndex::default()), role)
}

fn video_ids(player: &ffi::Player) -> Vec<String> {
    let model = &player.rust().video_file_model;
    let role = *model
        .role_names()
        .iter()
        .find(|(_, name)| name.to_string() == "videoId")
        .unwrap()
        .0;
    (0..model.count())
        .map(|row| {
            model
                .data(&model.model_index(row, 0, &QModelIndex::default()), role)
                .value::<QString>()
                .unwrap()
                .to_string()
        })
        .collect()
}

pub(super) fn run() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()?;
    let server = runtime.block_on(MockServer::start());
    let mount = |offset: u64, response: ResponseTemplate| {
        runtime.block_on(
            Mock::given(method("GET"))
                .and(path("/api/recorded"))
                .and(query_param("keyword", "retained search"))
                .and(query_param("offset", offset.to_string()))
                .respond_with(response)
                .up_to_n_times(1)
                .expect(1)
                .mount(&server),
        );
    };
    let response = |rows: Vec<serde_json::Value>, total: u64| {
        ResponseTemplate::new(200).set_body_json(json!({"records":rows,"total":total}))
    };
    let mut player = ffi::new_player();
    player.pin_mut().rust_mut().preferences =
        crate::settings::Loaded::transient(Default::default()).activate(None, None);
    crate::recording_model::ffi::check_model(player.pin_mut().rust_mut().recording_model.pin_mut());
    crate::video_file_model::ffi::check_video_model(
        player.pin_mut().rust_mut().video_file_model.pin_mut(),
    );
    let notices = Arc::new(Mutex::new(Vec::new()));
    let observed = notices.clone();
    let _notification = player.pin_mut().on_epgstation_changed(move |player| {
        let rows = player.rust().recording_library.rows();
        assert_eq!(player.rust().recording_model.count() as usize, rows.len());
        for (index, row) in rows.iter().enumerate() {
            assert_eq!(
                player
                    .rust()
                    .recording_model
                    .row_for_recording(row.id.to_string().into()),
                index as i32
            );
        }
        let selected: Vec<_> = player
            .rust()
            .recording_library
            .selected_files()
            .iter()
            .map(|file| file.id.to_string())
            .collect();
        assert_eq!(
            video_ids(&player),
            selected,
            "chooser reflects retained or invalidated selection before notification"
        );
        observed.lock().unwrap().push((
            player.epgstation_busy(),
            player.epgstation_page().to_string(),
            player.rust().recording_model.count(),
        ));
    });
    assert!(!player.pin_mut().refresh_epgstation());
    assert!(notices.lock().unwrap().is_empty());
    assert_eq!(
        player.rust().recording_model.row_for_recording("7".into()),
        -1
    );

    mount(0, response(vec![row(1, 111, "First page")], 101));
    mount(
        50,
        response(
            vec![
                row(7, 777, "Old title"),
                row(u64::MAX, 222, "Keep selected"),
            ],
            101,
        ),
    );
    assert!(
        player
            .pin_mut()
            .browse_epgstation(server.uri().into(), "retained search".into())
    );
    let pending_notices = notices.lock().unwrap().len();
    assert!(!player.pin_mut().refresh_epgstation());
    assert_eq!(notices.lock().unwrap().len(), pending_notices);
    finish(&mut player);
    player.pin_mut().epgstation_change_page(true);
    finish(&mut player);
    assert_eq!(player.epgstation_page().to_string(), "2");
    let lookup =
        |player: &ffi::Player, id: &str| player.rust().recording_model.row_for_recording(id.into());
    assert_eq!(lookup(&player, "7"), 0);
    assert_eq!(lookup(&player, &u64::MAX.to_string()), 1);
    for invalid in ["", "-1", "bad", " 7", "18446744073709551616", "999"] {
        assert_eq!(lookup(&player, invalid), -1);
    }

    let identity = Identity::new(
        Source::Epgstation {
            server: server.uri(),
            recorded_id: u64::MAX,
            video_file_id: 222,
        },
        "Keep selected".into(),
    );
    player.pin_mut().rust_mut().viewing_history.record(
        identity.clone(),
        Position::new(300_000.0, 600_000.0).unwrap(),
    );
    player.pin_mut().publish_recording_history();
    let signals = crate::recording_model::ffi::observe_recording_model(
        player.pin_mut().rust_mut().recording_model.pin_mut(),
    );
    assert_eq!(player.rust().continue_watching_model.count(), 1);

    let updated = vec![
        row(u64::MAX, 222, "Keep selected"),
        row(7, 333, "New title"),
    ];
    mount(
        50,
        response(updated.clone(), 101).set_delay(Duration::from_millis(60)),
    );
    assert!(player.pin_mut().choose_epgstation("7".into()));
    assert_eq!(player.rust().video_file_model.count(), 1);
    let model = player.recordings();
    assert!(player.pin_mut().refresh_epgstation());
    assert_eq!(video_ids(&player), ["777"]);
    assert!(player.epgstation_busy() && player.epgstation_loaded());
    assert!(player.epgstation_previous() && player.epgstation_next());
    assert_eq!(player.epgstation_page().to_string(), "2");
    assert_eq!(player.epgstation_total().to_string(), "101");
    assert_eq!(
        value(&player, 0, "programName")
            .value::<QString>()
            .unwrap()
            .to_string(),
        "Old title"
    );
    assert_eq!(signals.resets(), 0);
    assert_eq!(signals.updates(), 0);
    let before_duplicate = notices.lock().unwrap().len();
    assert!(!player.pin_mut().refresh_epgstation());
    assert_eq!(notices.lock().unwrap().len(), before_duplicate);
    assert!(
        player.pin_mut().choose_epgstation("7".into()),
        "retained rows remain selectable"
    );
    finish(&mut player);
    assert_eq!(video_ids(&player), ["333"]);
    assert_eq!(player.recordings(), model);
    assert_eq!(
        signals.resets(),
        1,
        "one direct replacement; no empty intermediate model"
    );
    assert_eq!(lookup(&player, &u64::MAX.to_string()), 0);
    assert_eq!(lookup(&player, "7"), 1);
    assert_eq!(
        value(&player, 1, "programName")
            .value::<QString>()
            .unwrap()
            .to_string(),
        "New title"
    );
    assert_eq!(value(&player, 0, "progress").value::<f64>(), Some(0.5));
    assert_eq!(player.rust().continue_watching_model.count(), 1);
    assert!(
        player
            .rust()
            .recording_library
            .playback_request(7, Some(777))
            .is_none()
    );
    assert!(
        player
            .rust()
            .recording_library
            .playback_request(7, Some(333))
            .is_some()
    );
    assert!(player.pin_mut().choose_epgstation("7".into()));
    assert_eq!(player.rust().recording_library.files(7)[0].id, 333);

    mount(50, ResponseTemplate::new(503));
    assert!(player.pin_mut().refresh_epgstation());
    finish(&mut player);
    assert!(!player.epgstation_error().is_empty());
    assert_eq!(video_ids(&player), ["333"]);
    assert!(player.epgstation_loaded());
    assert_eq!(player.epgstation_page().to_string(), "2");
    assert_eq!(signals.resets(), 1);
    assert_eq!(lookup(&player, "7"), 1);

    mount(50, response(updated, 101));
    player
        .pin_mut()
        .rust_mut()
        .viewing_history
        .record(identity, Position::new(450_000.0, 600_000.0).unwrap());
    assert!(player.pin_mut().refresh_epgstation());
    assert!(player.epgstation_error().is_empty());
    finish(&mut player);
    assert_eq!(
        signals.resets(),
        1,
        "identical catalogue does not reset selection"
    );
    assert_eq!(signals.updates(), 1, "progress uses dataChanged");
    assert_eq!(value(&player, 0, "progress").value::<f64>(), Some(0.75));

    mount(50, response(vec![], 1));
    mount(0, response(vec![row(u64::MAX, 222, "Keep selected")], 1));
    let before_clamp = notices.lock().unwrap().len();
    assert!(player.pin_mut().refresh_epgstation());
    finish(&mut player);
    assert_eq!(player.epgstation_page().to_string(), "1");
    assert_eq!(player.epgstation_total().to_string(), "1");
    assert!(!player.epgstation_previous() && !player.epgstation_next());
    assert_eq!(signals.resets(), 2);
    assert_eq!(lookup(&player, "7"), -1);
    assert!(
        video_ids(&player).is_empty(),
        "removed recording clears chooser"
    );
    assert_eq!(lookup(&player, &u64::MAX.to_string()), 0);
    assert_eq!(
        &notices.lock().unwrap()[before_clamp..],
        &[(true, "2".into(), 2), (false, "1".into(), 1)]
    );

    mount(0, response(vec![], 0));
    assert!(player.pin_mut().refresh_epgstation());
    finish(&mut player);
    assert!(player.epgstation_loaded());
    assert_eq!(signals.resets(), 3);
    assert_eq!(lookup(&player, &u64::MAX.to_string()), -1);
    runtime.block_on(server.verify());
    println!(
        "Recordings refresh: retained page, atomic replacement, failure, clamp, ID lookup and notifications passed"
    );
    Ok(())
}
