mod audio;
mod channels;
mod comment_model;
mod danmaku;
#[cfg(feature = "qml_tests")]
mod danmaku_ui_tests;
mod diagnostics;
mod error_log;
mod features;
mod logging;
mod memory;
#[cfg(feature = "native_tests")]
mod native_tests;
#[cfg(target_os = "linux")]
mod platform;
mod playback;
mod player;
mod remote;
mod screenshots;
mod services;
mod settings;
mod transport;
#[cfg(feature = "video_item_tests")]
mod video_item_tests;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Debug, thiserror::Error)]
enum StartupError {
    #[error("Allocator initialization failed: {0}")]
    Allocator(&'static str),
    #[error("{0}")]
    Arguments(#[source] features::ParseError),
    #[error("Feature plan was already initialized")]
    PlanAlreadyInitialized,
    #[error("Diagnostics initialization failed: {0}")]
    Diagnostics(#[from] diagnostics::Error),
    #[error("Could not create the Qt application")]
    Application,
    #[error("Could not create the Qt QML engine")]
    Engine,
    #[error("Playback initialization failed: {0}")]
    Playback(#[source] playback::Error),
    #[error("Could not load UI translation")]
    Translation,
    #[error("Could not load the Qt interface")]
    Interface,
}

fn main() -> std::process::ExitCode {
    #[cfg(feature = "native_tests")]
    if std::env::args().nth(1).as_deref() == Some("--native-tests") {
        return std::process::ExitCode::from(native_tests::run().clamp(0, 255) as u8);
    }
    // GUI integration checks must run on the process main thread, not libtest.
    #[cfg(feature = "video_item_tests")]
    if std::env::args().nth(1).as_deref() == Some("--video-item-tests") {
        return std::process::ExitCode::from(video_item_tests::run() as u8);
    }
    // Qt must run on the process main thread, not a libtest worker.
    #[cfg(feature = "qml_tests")]
    if std::env::args().nth(1).as_deref() == Some("--qml-tests") {
        return std::process::ExitCode::from(danmaku_ui_tests::run().clamp(0, 255) as u8);
    }
    logging::init();
    // SAFETY: Logging initialization creates no threads. Before Qt/GStreamer, diagnostics
    // or application workers are initialized. No application thread exists yet.
    #[cfg(target_os = "linux")]
    unsafe {
        platform::configure_at_startup();
    }
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!("{error}");
            if matches!(error, StartupError::Arguments(_)) {
                std::process::ExitCode::from(2)
            } else {
                std::process::ExitCode::FAILURE
            }
        }
    }
}

// Returning errors unwinds local ownership normally: the QML engine (and its
// Player) is destroyed before QGuiApplication, including failed UI creation.
fn run() -> Result<(), StartupError> {
    memory::configure().map_err(StartupError::Allocator)?;
    let plan =
        features::LaunchPlan::parse(std::env::args().skip(1)).map_err(StartupError::Arguments)?;
    tracing::info!("Feature plan: {plan:?}");
    features::PLAN
        .set(plan)
        .map_err(|_| StartupError::PlanAlreadyInitialized)?;
    player::ffi::install_qt_logging(logging::record_qt);
    if diagnostics::requested(plan.locked) {
        player::ffi::install_qt_gc_logging(diagnostics::record_qt_gc);
    }
    cxx_qt::init_qml_module!("MinimalViewer");
    player::ffi::configure_qt_quick_open_gl();
    let mut app = QGuiApplication::new();
    // qml6glsink registers its QML video type before loading the UI.
    if app.is_null() {
        return Err(StartupError::Application);
    }
    let _preloaded = playback::preload().map_err(StartupError::Playback)?;
    // Reverse local drop order keeps diagnostics alive through engine destruction.
    let _diagnostics = diagnostics::Lifetime::new()?;
    let mut engine = QQmlApplicationEngine::new();
    {
        let mut engine = engine.as_mut().ok_or(StartupError::Engine)?;
        // Match main: resolve the startup language before constructing QML.
        // Player loads the complete settings session and reports load errors separately.
        let language = if plan.locked {
            settings::Language::System
        } else {
            settings::settings_path()
                .and_then(settings::Loaded::open)
                .map(|session| session.preferences().language)
                .unwrap_or_default()
        };
        if !player::ffi::initialize_ui_language(
            engine.as_mut(),
            &cxx_qt_lib::QString::from(language.code()),
        ) {
            return Err(StartupError::Translation);
        }
        let failed = Arc::new(AtomicBool::new(false));
        let flag = failed.clone();
        let _connection = engine.as_mut().on_object_creation_failed(move |_, _| {
            flag.store(true, Ordering::Relaxed);
        });
        engine
            .as_mut()
            .load(&QUrl::from("qrc:/qt/qml/MinimalViewer/qml/Main.qml"));
        if failed.load(Ordering::Relaxed) {
            return Err(StartupError::Interface);
        }
    }
    app.as_mut().ok_or(StartupError::Application)?.exec();
    Ok(())
}
