//! The GUI lifetime is owned on the process main thread.
#[cfg(target_os = "linux")]
use crate::platform;
use crate::{diagnostics, features, logging, memory, playback, settings};
use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};
use std::ffi::OsStr;
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicBool, Ordering},
};

const QT_SCALE_FACTOR: &str = "QT_SCALE_FACTOR";
static LAUNCHED_UI_SCALE: OnceLock<settings::UiScale> = OnceLock::new();

pub(crate) fn launched_ui_scale() -> Option<settings::UiScale> {
    LAUNCHED_UI_SCALE.get().copied()
}

#[derive(Debug, thiserror::Error)]
enum ScaleFactorError {
    #[error("the value is not valid Unicode")]
    NonUnicode,
    #[error("the value is not a number: {0}")]
    Parse(#[from] std::num::ParseFloatError),
    #[error("the resulting factor must be positive and finite")]
    Range,
}

fn combined_scale_factor(
    scale: settings::UiScale,
    inherited: Option<&OsStr>,
) -> Result<f64, ScaleFactorError> {
    let Some(inherited) = inherited else {
        return Ok(scale.factor());
    };
    let factor = scale.factor()
        * inherited
            .to_str()
            .ok_or(ScaleFactorError::NonUnicode)?
            .trim()
            .parse::<f64>()?;
    if factor.is_finite() && factor > 0.0 {
        Ok(factor)
    } else {
        Err(ScaleFactorError::Range)
    }
}

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
    #[cfg(target_os = "linux")]
    #[error("Could not install quit signal handlers: {0}")]
    QuitSignals(#[source] cxx::Exception),
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
    #[cfg(target_os = "linux")]
    quit_signal_fd: i32,
}

impl LoadedApplication {
    pub fn exec(mut self) {
        // Keep an early signal queued until exec can honor the quit request.
        #[cfg(target_os = "linux")]
        super::ffi::watch_quit_signals(self.app.pin_mut(), self.quit_signal_fd, log_quit_signal);
        self.app.pin_mut().exec();
    }
}

/// Prepare the process GUI before any application worker starts.
///
/// # Safety
/// Call on the process main thread, before Qt initialization or any other
/// thread can access the environment modified by display setup.
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
    // Keep the applied preference even if the file changes before Player loads it.
    let scale = *LAUNCHED_UI_SCALE.get_or_init(|| launch.ui_scale);
    if scale != settings::UiScale::Standard {
        let inherited = std::env::var_os(QT_SCALE_FACTOR);
        let factor = match combined_scale_factor(scale, inherited.as_deref()) {
            Ok(factor) => factor,
            Err(error) => {
                tracing::warn!(%error, "Invalid inherited QT_SCALE_FACTOR, using the saved UI scale");
                scale.factor()
            }
        };
        // SAFETY: Still on the process main thread, before Qt, GStreamer or
        // any worker that could access the environment.
        unsafe { std::env::set_var(QT_SCALE_FACTOR, factor.to_string()) };
        tracing::info!(
            ui_scale = scale.code(),
            qt_scale_factor = factor,
            "Applied startup UI scale"
        );
    }
    if let Some(startup) = display {
        tracing::info!(
            startup_display = startup.code(),
            "Startup display set by argument"
        );
        settings::startup_display::OVERRIDE
            .set(startup)
            .map_err(|_| Error::DisplayAlreadyInitialized)?;
    }
    #[cfg(target_os = "linux")]
    let quit_signal_fd = super::ffi::open_quit_signal_pipe().map_err(Error::QuitSignals)?;
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
    let app = {
        let mut app = app;
        super::ffi::install_key_normalizer(app.pin_mut());
        app
    };
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
        #[cfg(target_os = "linux")]
        quit_signal_fd,
    })
}

#[cfg(target_os = "linux")]
fn log_quit_signal(number: i32) {
    let signal = match number {
        libc::SIGTERM => "SIGTERM",
        libc::SIGINT => "SIGINT",
        libc::SIGHUP => "SIGHUP",
        _ => "unknown",
    };
    tracing::info!(signal, number, "Application quit requested by signal");
}

#[cfg(test)]
mod tests {
    use super::*;
    use settings::UiScale;

    #[test]
    fn combines_the_saved_scale_with_the_inherited_factor() {
        for (scale, standalone, combined) in [
            (UiScale::Standard, 1.0, 1.5),
            (UiScale::Percent125, 1.25, 1.875),
            (UiScale::Percent150, 1.5, 2.25),
            (UiScale::Percent200, 2.0, 3.0),
        ] {
            assert_eq!(combined_scale_factor(scale, None).unwrap(), standalone);
            assert_eq!(
                combined_scale_factor(scale, Some(OsStr::new(" 1.5 "))).unwrap(),
                combined
            );
        }
    }

    #[test]
    fn rejects_unusable_inherited_values_and_products() {
        for value in ["", "125%", "0", "-1", "NaN", "inf", "1e308"] {
            assert!(
                combined_scale_factor(UiScale::Percent200, Some(OsStr::new(value))).is_err(),
                "{value:?}"
            );
        }
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            assert!(matches!(
                combined_scale_factor(UiScale::Percent150, Some(OsStr::from_bytes(b"\xff"))),
                Err(ScaleFactorError::NonUnicode)
            ));
        }
    }
}
