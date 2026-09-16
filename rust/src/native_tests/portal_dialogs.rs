//! Real Qt portal plugin against a private D-Bus protocol fixture, with a real GUI.
use super::bridge::ffi;
use crate::{platform, player};
use cxx_qt_lib::{QByteArray, QGuiApplication, QQmlApplicationEngine, QString, QUrl};
use std::time::{Duration, Instant};

fn evaluate(engine: &mut cxx::UniquePtr<QQmlApplicationEngine>, source: &str) -> bool {
    ffi::evaluate_root(engine.pin_mut(), &QString::from(source))
        .expect("QML expression")
        .value::<bool>()
        .expect("Boolean QML expression")
}

fn selected_url(engine: &QQmlApplicationEngine, property: &str) -> QUrl {
    ffi::root_property(engine, &QString::from(property))
        .value::<QUrl>()
        .expect("URL QML property")
}

fn wait_for(
    app: &QGuiApplication,
    engine: &mut cxx::UniquePtr<QQmlApplicationEngine>,
    source: &str,
) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        app.process_events();
        if evaluate(engine, source) {
            return;
        }
        assert!(Instant::now() < deadline, "Timed out: {source}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

pub fn run() -> i32 {
    let directory = std::path::PathBuf::from(
        std::env::var_os("VIEWER_PORTAL_TEST_DIR").expect("Run scripts/test-portal-dialogs.sh"),
    );
    let file = url::Url::from_file_path(directory.join("録画 #100%.ts")).unwrap();
    let folder = url::Url::from_directory_path(directory.join("キャプチャ #100%"))
        .unwrap()
        .as_str()
        .trim_end_matches('/')
        .to_owned();
    // SAFETY: Fresh main-thread test process, before Qt or any worker starts.
    let dialogs = unsafe { platform::DialogSetup::prepare() };
    player::ffi::configure_qt_quick_open_gl();
    let app = QGuiApplication::new();
    assert!(!app.is_null());
    assert_eq!(dialogs.finish(&app), platform::DialogBackend::Portal);
    let mut engine = QQmlApplicationEngine::new();
    engine.pin_mut().load_data(
        &QByteArray::from(include_str!("../../tests/portal-dialogs.qml")),
        &QUrl::from("file:///PortalDialogs.qml"),
    );
    assert_eq!(ffi::root_count(&engine), 1);
    wait_for(&app, &mut engine, "painted");
    assert!(evaluate(&mut engine, "file.open(); true"));
    wait_for(&app, &mut engine, "accepted === 1 && !file.visible");
    let expected_file = QUrl::from(file.as_str());
    assert_eq!(selected_url(&engine, "fileUrl"), expected_file);
    assert!(evaluate(&mut engine, "file.open(); true"));
    wait_for(&app, &mut engine, "rejected === 1 && !file.visible");
    assert!(evaluate(&mut engine, "accepted === 1"));
    assert_eq!(selected_url(&engine, "fileUrl"), expected_file);
    assert!(evaluate(&mut engine, "folder.open(); true"));
    wait_for(&app, &mut engine, "accepted === 2 && !folder.visible");
    let expected_folder = QUrl::from(folder.as_str());
    assert_eq!(selected_url(&engine, "folderUrl"), expected_folder);
    assert!(evaluate(&mut engine, "folder.open(); true"));
    wait_for(&app, &mut engine, "rejected === 2 && !folder.visible");
    assert!(evaluate(&mut engine, "accepted === 2"));
    assert_eq!(selected_url(&engine, "folderUrl"), expected_folder);
    assert!(evaluate(&mut engine, "close(); true"));
    app.process_events();
    println!("Portal dialog checks passed: file/folder selection, cancellation and escaped URLs");
    0
}
