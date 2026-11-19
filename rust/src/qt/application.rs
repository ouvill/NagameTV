//! The GUI lifetime is owned on the process main thread.
#[cfg(target_os = "linux")]
use crate::platform;
use crate::{diagnostics, features, logging, memory, playback, settings};
use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Allocator initialization failed: {0}")]
    Allocator(&'static str),
    #[error("Feature plan was already initialized")]
    PlanAlreadyInitialized,
    #[error("Startup display was already initialized")]
    DisplayAlreadyInitialized,
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

/// Only successful QML loading constructs this owner. Field order destroys the
/// engine and its Player before diagnostics, playback and QGuiApplication.
#[must_use]
pub struct LoadedApplication {
    _engine: cxx::UniquePtr<QQmlApplicationEngine>,
    _diagnostics: diagnostics::Lifetime,
    _preloaded: playback::Preloaded,
    app: cxx::UniquePtr<QGuiApplication>,
}

impl LoadedApplication {
    pub fn exec(mut self) {
        self.app.pin_mut().exec();
    }
}

/// Prepare the process GUI before any application worker starts.
///
/// # Safety
/// Call on the process main thread, before Qt initialization or any other
/// thread can access the environment modified by platform dialog setup.
pub unsafe fn load(
    plan: features::LaunchPlan,
    display: Option<settings::StartupDisplay>,
) -> Result<LoadedApplication, Error> {
    memory::configure().map_err(Error::Allocator)?;
    // Player loads the complete settings session and reports load errors separately.
    let launch = if plan.locked() {
        settings::Preferences::default()
    } else {
        settings::settings_path()
            .and_then(settings::Loaded::open)
            .map(|session| session.preferences().clone())
            .unwrap_or_default()
    };
    let mode = playback::deinterlace::Mode::for_launch(launch.deinterlace)
        .map_err(|error| Error::Playback(error.into()))?;
    // SAFETY: Still before Qt, GStreamer, diagnostics or worker initialization.
    #[cfg(target_os = "linux")]
    let dialogs = unsafe {
        platform::configure_at_startup(mode);
        platform::DialogSetup::prepare()
    };
    tracing::info!("Feature plan: {plan:?}");
    features::PLAN
        .set(plan)
        .map_err(|_| Error::PlanAlreadyInitialized)?;
    if let Some(startup) = display {
        tracing::info!(
            startup_display = startup.code(),
            "Startup display set by argument"
        );
        settings::startup_display::OVERRIDE
            .set(startup)
            .map_err(|_| Error::DisplayAlreadyInitialized)?;
    }
    super::ffi::install_qt_logging(logging::record_qt);
    if diagnostics::requested(plan.locked()) {
        super::ffi::install_qt_gc_logging(diagnostics::record_qt_gc);
    }
    cxx_qt::init_qml_module!("MinimalViewer");
    super::ffi::configure_qt_quick_open_gl();
    let app = QGuiApplication::new();
    // qml6glsink registers its QML video type before loading the UI.
    if app.is_null() {
        return Err(Error::Application);
    }
    #[cfg(target_os = "linux")]
    dialogs.finish(&app);
    let preloaded = playback::preload(mode).map_err(Error::Playback)?;
    // Reverse local drop order keeps diagnostics alive through engine destruction.
    let diagnostics = diagnostics::Lifetime::new()?;
    let mut engine = QQmlApplicationEngine::new();
    {
        let mut engine = engine.as_mut().ok_or(Error::Engine)?;
        let language = launch.language;
        if !super::ffi::initialize_ui_language(
            engine.as_mut(),
            &cxx_qt_lib::QString::from(language.code()),
        ) {
            return Err(Error::Translation);
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
            return Err(Error::Interface);
        }
    }
    Ok(LoadedApplication {
        _engine: engine,
        _diagnostics: diagnostics,
        _preloaded: preloaded,
        app,
    })
}
