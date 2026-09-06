mod features;
mod playback;
mod player;
mod services;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

fn main() {
    let plan = features::LaunchPlan::parse(std::env::args().skip(1)).unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(2);
    });
    eprintln!("Feature plan: {plan:?}");
    features::PLAN.set(plan).unwrap();
    cxx_qt::init_qml_module!("MinimalViewer");
    player::ffi::configure_qt_quick_open_gl();
    let mut app = QGuiApplication::new();
    // qml6glsink registers its QML video type before loading the UI.
    if let Err(error) = playback::preload() {
        eprintln!("Playback initialization failed: {error}");
        std::process::exit(1);
    }
    let mut engine = QQmlApplicationEngine::new();
    if let Some(mut engine) = engine.as_mut() {
        let failed = Arc::new(AtomicBool::new(false));
        let flag = failed.clone();
        let _connection = engine.as_mut().on_object_creation_failed(move |_, _| {
            flag.store(true, Ordering::Relaxed);
        });
        engine
            .as_mut()
            .load(&QUrl::from("qrc:/qt/qml/MinimalViewer/qml/Main.qml"));
        if failed.load(Ordering::Relaxed) {
            eprintln!("Could not load the Qt interface");
            std::process::exit(1);
        }
    }
    if let Some(app) = app.as_mut() {
        app.exec();
    }
}
