mod network;
mod playback;
mod player;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};

fn main() {
    cxx_qt::init_qml_module!("MirakurunViewer");
    player::ffi::configure_qt_quick_open_gl();
    let mut app = QGuiApplication::new();

    if let Err(error) = playback::preload() {
        eprintln!("Could not initialize playback: {error}");
        std::process::exit(1);
    }

    let mut engine = QQmlApplicationEngine::new();
    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/MirakurunViewer/qml/Main.qml"));
    }
    if let Some(app) = app.as_mut() {
        app.exec();
    }
}
