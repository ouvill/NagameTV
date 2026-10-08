//! Check update properties during real Qt notifications, without opening a browser.
use super::ffi::{self, UpdateStatus};
use cxx_qt::CxxQtType;
use std::{
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};

pub(super) fn run() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()?;
    let server = runtime.block_on(MockServer::start());
    let mut player = ffi::new_player();
    let observations = Arc::new(Mutex::new(Vec::new()));
    let observed = observations.clone();
    let _connection = player.pin_mut().on_updates_changed(move |player| {
        observed.lock().unwrap().push((
            player.update_check_status(),
            player.update_version().to_string(),
            player.update_error().to_string(),
            player.update_last_checked(),
            player.update_history_error().to_string(),
        ));
    });
    assert!(player.update_check_status() == UpdateStatus::UpdateIdle);
    assert!(!player.automatic_updates_allowed());
    player.pin_mut().configure_auto_update_check(true);
    assert!(!player.auto_update_check());
    assert!(!player.pin_mut().open_update_release());
    for (code, version, expected) in [
        (200, "999.0.0", UpdateStatus::UpdateAvailable),
        (403, "999.0.0", UpdateStatus::UpdateFailed),
        (
            200,
            crate::build_info::INFO.version,
            UpdateStatus::UpdateCurrent,
        ),
        (404, "999.0.0", UpdateStatus::UpdateNoRelease),
    ] {
        let previous_success = player.update_last_checked();
        runtime.block_on(server.reset());
        runtime.block_on(Mock::given(method("GET")).respond_with(ResponseTemplate::new(code)
            .set_body_json(serde_json::json!({"tag_name":format!("v{version}"), "prerelease":false, "draft":false})))
            .expect(1).mount(&server));
        {
            let mut state = player.pin_mut();
            let state = state.as_mut().rust_mut();
            let state = state.get_mut();
            assert!(
                state
                    .updates
                    .check_fixture(state.network.as_ref(), server.uri())
            );
        }
        player.pin_mut().updates_changed();
        assert!(player.update_check_status() == UpdateStatus::UpdateChecking);
        assert!(player.update_version().is_empty());
        assert!(player.update_error().is_empty());
        player.pin_mut().check_updates();
        let deadline = Instant::now() + Duration::from_secs(5);
        while player.update_check_status() == UpdateStatus::UpdateChecking {
            assert!(
                Instant::now() < deadline,
                "update projection did not finish"
            );
            player.pin_mut().poll_updates();
            thread::sleep(Duration::from_millis(1));
        }
        assert!(player.update_check_status() == expected);
        let values = observations.lock().unwrap();
        let (status, published, error, success, history_error) = values.last().unwrap();
        assert!(*status == expected);
        assert_eq!(
            published,
            if expected == UpdateStatus::UpdateAvailable {
                version
            } else {
                ""
            }
        );
        assert_eq!(!error.is_empty(), expected == UpdateStatus::UpdateFailed);
        assert!(history_error.is_empty(), "{history_error}");
        if expected == UpdateStatus::UpdateFailed {
            assert_eq!(*success, previous_success);
        } else {
            assert!(*success >= 0.0);
            assert_eq!(
                crate::updates::Checker::load(false, false).last_success(),
                Some(*success as u64),
            );
        }
        runtime.block_on(server.verify());
    }
    player.pin_mut().rust_mut().network = None;
    player.pin_mut().check_updates();
    assert!(player.update_check_status() == UpdateStatus::UpdateFailed);
    assert!(!player.update_error().is_empty());
    assert!(player.pin_mut().shutdown());
    assert!(player.update_check_status() == UpdateStatus::UpdateIdle);
    Ok(())
}
