mod audio;
mod channels;
mod comments;
mod diagnostics;
mod epg;
mod epg_events;
mod network;
mod playback;
mod player;
mod settings;
mod subtitles;
mod video_stats;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl};
use tracing_subscriber::EnvFilter;

fn main() {
    initialize_tracing();
    apply_temporary_xcb_workaround();
    cxx_qt::init_qml_module!("MirakurunViewer");
    player::ffi::configure_qt_quick_open_gl();
    let mut app = QGuiApplication::new();

    if let Err(error) = playback::preload() {
        tracing::error!(%error, "Could not initialize playback");
        std::process::exit(1);
    }

    let mut engine = QQmlApplicationEngine::new();
    if let Some(mut engine) = engine.as_mut() {
        let language = settings::Settings::load()
            .map(|s| s.language)
            .unwrap_or_else(|_| "system".to_owned());
        if !player::ffi::initialize_ui_language(engine.as_mut(), &QString::from(language)) {
            tracing::error!("Could not load UI translation");
            std::process::exit(1);
        }
        engine.load(&QUrl::from("qrc:/qt/qml/MirakurunViewer/qml/Main.qml"));
    }
    if let Some(app) = app.as_mut() {
        app.exec();
    }
}

fn initialize_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .init();
}

fn apply_temporary_xcb_workaround() {
    #[cfg(target_os = "linux")]
    {
        let wayland_session =
            std::env::var_os("WAYLAND_DISPLAY").is_some_and(|value| !value.is_empty());
        let xwayland_available = std::env::var_os("DISPLAY").is_some_and(|value| !value.is_empty());

        if std::env::var_os("QT_QPA_PLATFORM").is_none() && wayland_session && xwayland_available {
            // Temporary compatibility workaround for native Wayland GL texture corruption in
            // qml6glsink. Remove this default after the upstream Qt/GStreamer/NVIDIA path is fixed.
            // Related upstream report: https://gitlab.freedesktop.org/gstreamer/gstreamer/-/work_items/5178
            // SAFETY: This runs at process startup, before Qt or any worker thread is initialized.
            unsafe {
                std::env::set_var("QT_QPA_PLATFORM", "xcb");
            }
            tracing::info!(
                "Using the temporary xcb compatibility workaround for video rendering; \
                 set QT_QPA_PLATFORM=wayland explicitly to test the native Wayland path"
            );
        }
    }
}
