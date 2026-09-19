//! FileTransfer protocol and real QML drop events on the private portal test bus.
use super::{
    bridge::ffi,
    portal_dialogs::{evaluate, wait_for},
};
use crate::playback::recording::{Error, Loader, Purpose, Request};
use cxx_qt_lib::{QByteArray, QGuiApplication, QPoint, QQmlApplicationEngine, QString, QUrl};
use std::{
    path::Path,
    time::{Duration, Instant},
};

fn finish(
    app: &QGuiApplication,
    loader: &mut Loader,
) -> Result<crate::playback::recording::Recording, Error> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        app.process_events();
        if let Some((purpose, result)) = loader.poll() {
            assert_eq!(purpose, Purpose::Open);
            assert!(!loader.loading());
            return result;
        }
        assert!(Instant::now() < deadline, "Transfer inspection timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}

pub fn run(app: &QGuiApplication, directory: &Path) {
    cxx_qt::init_qml_module!("MinimalViewer");
    let ts = include_bytes!("../../../tests/fixtures/recording.ts");
    std::fs::write(directory.join("録画 #100%.ts"), ts).unwrap();
    const TS_PACKET_BYTES: usize = 188;
    const M2TS_PREFIX_BYTES: usize = 4;
    let mut m2ts = Vec::new();
    for packet in ts.as_chunks::<TS_PACKET_BYTES>().0 {
        m2ts.extend_from_slice(&[0; M2TS_PREFIX_BYTES]);
        m2ts.extend_from_slice(packet);
    }
    std::fs::write(directory.join("録画 #100%.m2ts"), m2ts).unwrap();
    let mut loader = Loader::default();
    for (key, extension) in [("ts\0", "ts"), ("m2ts", "m2ts")] {
        loader.begin(Request::portal_transfer(key).unwrap());
        let recording = finish(app, &mut loader).unwrap();
        assert_eq!(
            recording.path(),
            directory.join(format!("録画 #100%.{extension}"))
        );
    }
    for key in [
        "ts", "multiple", "empty", "relative", "unknown", "", "bad\0key",
    ] {
        assert!(
            matches!(Request::portal_transfer(key), Err(Error::Portal(_))),
            "{key:?}"
        );
    }

    let mut engine = QQmlApplicationEngine::new();
    engine.pin_mut().load_data(
        &QByteArray::from(include_str!("../../tests/recording-drop.qml")),
        &QUrl::from("file:///RecordingDrop.qml"),
    );
    assert_eq!(ffi::root_count(&engine), 1);
    wait_for(app, &mut engine, "painted");
    let position = QPoint::new(20, 200);
    let host = QString::from("file:///inaccessible/recording.ts");
    let mut transfers = 0;
    for setup in [false, true] {
        if setup {
            assert!(evaluate(&mut engine, "setup.open(); true"));
            wait_for(app, &mut engine, "setup.opened");
        }
        for format in [
            "application/vnd.portal.filetransfer",
            "application/vnd.portal.files",
        ] {
            // Both a portal-only drop and a drop also advertising a host URI.
            for url in [QString::default(), host.clone()] {
                let key = format!("drop-{transfers}");
                // GTK includes a terminating NUL, KDE does not.
                let payload = if url.is_empty() {
                    format!("{key}\0")
                } else {
                    key.clone()
                };
                assert!(ffi::drop_transfer_on_root(
                    engine.pin_mut(),
                    &QString::from(format),
                    &QByteArray::from(payload.as_str()),
                    &url,
                    &position
                ));
                // No event pumping/waiting after drop: receipt must finish
                // before Qt acknowledges it and KDE stops the source transfer.
                assert!(directory.join(format!("received-{key}")).exists());
                transfers += 1;
                assert!(evaluate(
                    &mut engine,
                    &format!(
                        "transfers === {transfers} && receivedKey.replace(/\\0$/, '') === '{key}' && files === 0 && receiver.recording_loading && receiver.file_error.length === 0"
                    )
                ));
                assert!(evaluate(
                    &mut engine,
                    "receiver.cancel_recording_open(); true"
                ));
            }
        }
    }
    assert!(evaluate(&mut engine, "close(); true"));
    app.process_events();
    println!(
        "Recording drop checks passed: portal priority, first-run screen, TS/M2TS and invalid transfers"
    );
}
