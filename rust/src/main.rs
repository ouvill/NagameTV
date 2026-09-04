mod comments;
mod epg;
mod network;
mod playback;
mod player;
mod settings;
mod subtitles;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};

fn main() {
    apply_temporary_xcb_workaround();
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
            eprintln!(
                "Using the temporary xcb compatibility workaround for video rendering; \
                 set QT_QPA_PLATFORM=wayland explicitly to test the native Wayland path"
            );
        }
    }
}
