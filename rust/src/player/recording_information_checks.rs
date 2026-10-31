//! Player publication checks with inspected files and HTTP, without a decoder or devices.
use super::{ffi, stream_state::State};
use crate::{
    playback::{
        recording::{Loader, Recording, Request},
        timeline::Phase,
    },
    transport::programs::{
        Program,
        catalog::{Status, View},
        presentation::Cache,
    },
    viewing_history::recordings::{Identity, Source},
};
use cxx_qt::CxxQtType;
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, query_param},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;
fn load(request: Request) -> Result<Recording, Box<dyn std::error::Error>> {
    let mut loader = Loader::default();
    loader.begin(request);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some((_, result)) = loader.poll() {
            return Ok(result?);
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn data(player: &ffi::Player) -> Value {
    serde_json::from_str(&player.current_program_data().to_string()).unwrap()
}
fn player() -> cxx::UniquePtr<ffi::Player> {
    let mut player = ffi::new_player();
    player.pin_mut().rust_mut().preferences =
        crate::settings::Loaded::transient(Default::default()).activate(None, None);
    player.pin_mut().rust_mut().epg_enabled = true;
    player
}

pub(super) fn run() -> TestResult {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()?;
    let server = runtime.block_on(MockServer::start());
    let record = json!({"id":1,"name":"Catalogue title","description":"Summary","extended":"Details",
        "channelName":"Station","startAt":1000,"endAt":61000,"genre1":7,"subGenre1":0,"isRecording":false,
        "videoFiles":[{"id":123,"type":"encoded","size":1234,"startAt":1000}]});
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
        Mock::given(path("/api/recorded/1"))
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&server),
    );
    let mut detail = record.clone();
    detail["id"] = json!(2);
    detail["name"] = json!("Resumed title");
    runtime.block_on(
        Mock::given(method("GET"))
            .and(path("/api/recorded/2"))
            .and(query_param("isHalfWidth", "false"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(detail)
                    .set_delay(Duration::from_millis(80)),
            )
            .expect(1)
            .mount(&server),
    );
    runtime.block_on(
        Mock::given(method("GET"))
            .and(path("/api/videos/123"))
            .respond_with(|request: &wiremock::Request| {
                let bytes = include_bytes!("../../../tests/fixtures/media-h264.mp4");
                let range = request.headers["range"]
                    .to_str()
                    .unwrap()
                    .strip_prefix("bytes=")
                    .unwrap();
                let (start, end) = range.split_once('-').unwrap();
                let (start, end) = (
                    start.parse::<usize>().unwrap(),
                    end.parse::<usize>().unwrap(),
                );
                assert!(end < bytes.len());
                ResponseTemplate::new(206)
                    .insert_header(
                        "Content-Range",
                        format!("bytes {start}-{end}/{}", bytes.len()),
                    )
                    .insert_header("ETag", "\"fixture\"")
                    .set_body_bytes(bytes[start..=end].to_vec())
            })
            .mount(&server),
    );
    let mut catalogue_player = player();
    assert!(
        catalogue_player
            .pin_mut()
            .browse_epgstation(server.uri().into(), "".into())
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while catalogue_player.epgstation_busy() {
        catalogue_player.pin_mut().poll_recording_library();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    let file = load(
        catalogue_player
            .rust()
            .recording_library
            .playback_request(1, Some(123))
            .ok_or("catalogue playback request")?,
    )?;
    assert!(file.program().is_some());
    catalogue_player
        .pin_mut()
        .update_stream_state(State::Recording(file, Phase::Playing));
    assert_eq!(data(&catalogue_player)["name"], "Catalogue title");
    assert_eq!(data(&catalogue_player)["description"], "Summary\n\nDetails");
    assert_eq!(data(&catalogue_player)["station"], "Station");
    assert_eq!(data(&catalogue_player)["progressKnown"], false);
    assert!(!catalogue_player.rust().recording_program.busy());

    let view = View {
        status: Status::Available,
        program: Some(Program {
            event_id: 1,
            service_id: 1,
            network_id: 1,
            transport_stream_id: 1,
            start_at: Some(1000),
            duration: Some(60000),
            name: "Position-scoped TS title".into(),
            description: String::new(),
            extended: String::new(),
            genres: vec![],
            audios: Box::default(),
        }),
        ..Default::default()
    };
    let (ts, _, status) =
        catalogue_player
            .rust()
            .recording_program
            .project(true, &view, 0, &mut Cache::default());
    assert_eq!(ts.unwrap().title(), "Position-scoped TS title");
    assert_eq!(status, "available");
    assert!(
        catalogue_player
            .rust()
            .recording_program
            .project(false, &view, 0, &mut Cache::default())
            .0
            .is_none()
    );
    catalogue_player.pin_mut().rust_mut().epg_enabled = false;
    catalogue_player.pin_mut().change_stream_state(|state| {
        state.transport(Phase::Seeking(crate::playback::timeline::Resume::Playing))
    });
    assert!(data(&catalogue_player).is_null());
    assert_eq!(catalogue_player.program_status().to_string(), "disabled");

    let mut resumed = player();
    resumed.pin_mut().rust_mut().epg_enabled = false;
    let identity = Identity::new(
        Source::Epgstation {
            server: server.uri(),
            recorded_id: 2,
            video_file_id: 123,
        },
        "History title".into(),
    );
    let file = load(resumed.rust().recording_library.resume_request(identity)?)?;
    assert!(file.program().is_none());
    assert!(!resumed.epgstation_loaded());
    resumed
        .pin_mut()
        .update_stream_state(State::Recording(file, Phase::Playing));
    assert!(data(&resumed).is_null());
    assert!(!resumed.rust().recording_program.busy());
    assert!(
        runtime
            .block_on(server.received_requests())
            .unwrap()
            .iter()
            .all(|request| request.url.path() != "/api/recorded/2")
    );
    resumed.pin_mut().rust_mut().epg_enabled = true;
    resumed.pin_mut().change_stream_state(|state| state);
    assert!(resumed.playing());
    assert!(resumed.rust().recording_program.busy());
    assert!(data(&resumed).is_null());
    let deadline = Instant::now() + Duration::from_secs(5);
    while resumed.rust().recording_program.busy() {
        resumed.pin_mut().change_stream_state(|state| state);
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(data(&resumed)["name"], "Resumed title");
    resumed.pin_mut().change_stream_state(State::stopped);
    assert!(data(&resumed).is_null());
    assert!(!resumed.pin_mut().seek_chapter(1));

    let mut toc = gstreamer::Toc::new(gstreamer::TocScope::Global);
    let mut chapter = gstreamer::TocEntry::new(gstreamer::TocEntryType::Chapter, "chapter");
    chapter.get_mut().unwrap().set_start_stop_times(0, -1);
    toc.get_mut().unwrap().append_entry(chapter);
    resumed.pin_mut().rust_mut().chapters.seed_for_check(&toc);
    assert_eq!(resumed.current_chapter(), 0);
    let changes = Arc::new(Mutex::new(0));
    let observed = changes.clone();
    let _signal = resumed.pin_mut().on_chapters_changed(move |player| {
        assert_eq!(player.chapters_data().to_string(), "[]");
        assert_eq!(player.current_chapter(), -1);
        *observed.lock().unwrap() += 1;
    });
    let local = Recording::open(std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../tests/fixtures/media-h264.mp4"
    )))?;
    resumed
        .pin_mut()
        .update_stream_state(State::Recording(local, Phase::Playing));
    assert_eq!(*changes.lock().unwrap(), 1);
    assert!(data(&resumed).is_null());
    resumed.pin_mut().change_stream_state(|state| state);
    assert_eq!(*changes.lock().unwrap(), 1);
    for player in [&catalogue_player, &resumed] {
        assert!(
            player.rust().media.playback().is_none(),
            "checks must never initialize native playback"
        );
    }
    runtime.block_on(server.verify());
    Ok(())
}
