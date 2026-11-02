//! Real QObject publication without a display, tuner or event-server dependency.
use super::ffi;
use cxx_qt::CxxQtType;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::path};

pub(super) fn run() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Runtime::new()?;
    let server = runtime.block_on(MockServer::start());
    let start = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
    let program = Arc::new(Mutex::new(
        serde_json::json!({"id":1,"eventId":1,"networkId":10,"serviceId":1,"startAt":start,"duration":3600000,"name":"original"}),
    ));
    let response = program.clone();
    runtime.block_on(
        Mock::given(path("/api/programs"))
            .respond_with(move |_: &wiremock::Request| {
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!([response.lock().unwrap().clone()]))
            })
            .expect(2)
            .mount(&server),
    );
    let channels = r#"[{"id":9000000001,"name":"TV","type":1,"networkId":10,"serviceId":1}]"#;
    runtime.block_on(
        Mock::given(path("/api/services"))
            .respond_with(ResponseTemplate::new(200).set_body_string(channels))
            .expect(1)
            .mount(&server),
    );
    let mut player = ffi::new_player();
    {
        let state = player.pin_mut().rust_mut().get_mut();
        state.network = Some(crate::services::Network::new()?);
        state.server = server.uri().into();
        state.epg_enabled = true;
        state.channel_refresh.requested(Instant::now());
        state
            .catalog
            .replace(crate::channels::parse(channels.as_bytes())?, None);
        state.epg.configure(Some(server.uri()));
        state.guide = crate::features::program_info::guide::Guide::Showing(
            crate::features::program_info::guide::DayWindow::new(
                start as f64,
                (start + 86_400_000) as f64,
            )?,
        );
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while !matches!(
        player.rust().epg.status(),
        crate::features::program_info::Status::Ready(_)
    ) {
        assert!(Instant::now() < deadline);
        player.pin_mut().poll_epg();
        std::thread::sleep(Duration::from_millis(1));
    }
    let notifications = Arc::new(Mutex::new(0));
    let observed = notifications.clone();
    let _signal = player.pin_mut().on_current_program_data_changed(move |_| {
        *observed.lock().unwrap() += 1;
    });
    let revision = player.rust().epg.revision;
    let guide_revision = player.rust().guide_revision;
    for _ in 0..100 {
        let mut changes = viewer_epg_events::Changes::default();
        viewer_epg_events::Decoder::default().push_changes(&serde_json::to_vec(&serde_json::json!({"resource":"program","type":"update","data":program.lock().unwrap().clone()}))?,&mut changes)?;
        player
            .pin_mut()
            .apply_epg_events(viewer_epg_events::controller::Update {
                changes,
                ..Default::default()
            });
        player.pin_mut().poll_epg();
        assert!(!player.loading());
    }
    assert_eq!(*notifications.lock().unwrap(), 0);
    assert_eq!(player.rust().epg.revision, revision);
    assert_eq!(player.rust().guide_revision, guide_revision);
    program.lock().unwrap()["name"] = "changed".into();
    let mut changes = viewer_epg_events::Changes::default();
    viewer_epg_events::Decoder::default().push_changes(&serde_json::to_vec(&serde_json::json!({"resource":"program","type":"update","data":program.lock().unwrap().clone()}))?,&mut changes)?;
    player
        .pin_mut()
        .apply_epg_events(viewer_epg_events::controller::Update {
            changes,
            ..Default::default()
        });
    player.pin_mut().poll_epg();
    assert_eq!(*notifications.lock().unwrap(), 1);
    assert!(
        player
            .current_program_data()
            .to_string()
            .contains("changed")
    );
    let requests = runtime
        .block_on(server.received_requests())
        .ok_or("missing HTTP requests")?;
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.url.path() == "/api/services")
            .count(),
        0
    );
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.url.path() == "/api/programs")
            .count(),
        1
    );
    player
        .pin_mut()
        .apply_epg_events(viewer_epg_events::controller::Update {
            changes: viewer_epg_events::Changes::Full,
            refresh_channels: true,
            ..Default::default()
        });
    player.pin_mut().poll_epg();
    while player.loading()
        || matches!(
            player.rust().epg.status(),
            crate::features::program_info::Status::Fetching
        )
    {
        assert!(Instant::now() < deadline);
        player.pin_mut().poll_channels();
        player.pin_mut().poll_epg();
        std::thread::sleep(Duration::from_millis(1));
    }
    runtime.block_on(server.verify());
    Ok(())
}
