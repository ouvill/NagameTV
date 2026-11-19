//! Linux startup defaults; environment hints are not device validation.

mod portal;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogBackend {
    Portal,
    QtQuick,
}

/// Prepared before Qt starts; consumed after its platform theme has loaded.
pub struct DialogSetup(Dialogs);

enum Dialogs {
    Flatpak,
    Probed(Result<(), portal::ChooserError>),
    BypassPortal,
}

impl DialogSetup {
    /// Select the portal platform theme before Qt or any worker starts.
    ///
    /// # Safety
    /// No other thread may access the process environment during this call.
    /// QGuiApplication must not have been constructed yet.
    pub unsafe fn prepare() -> Self {
        let dialogs = if std::path::Path::new("/.flatpak-info").exists() {
            Dialogs::Flatpak
        } else {
            match portal::probe() {
                Ok(chooser) => Dialogs::Probed(chooser),
                Err(error) => {
                    tracing::warn!(
                        error = &error as &dyn std::error::Error,
                        "Desktop portal probe failed; using Qt Quick dialogs and direct system network connections"
                    );
                    Dialogs::BypassPortal
                }
            }
        };
        // Qt tries this explicit theme before its automatic portal selection.
        // An empty theme name would let Qt pick xdgdesktopportal again in a snap.
        if matches!(dialogs, Dialogs::BypassPortal) {
            unsafe {
                std::env::set_var("QT_QPA_PLATFORMTHEME", "generic");
                std::env::set_var("GIO_USE_PROXY_RESOLVER", "dummy");
            }
            crate::qt::ffi::disable_qt_system_proxy();
        } else {
            unsafe { std::env::set_var("QT_QPA_PLATFORMTHEME", "xdgdesktopportal") };
        }
        Self(dialogs)
    }

    /// Finish before loading QML. The application borrow proves Qt is initialized.
    pub fn finish(self, _app: &cxx_qt_lib::QGuiApplication) -> DialogBackend {
        match self.0 {
            Dialogs::Flatpak => {
                // The runtime supplies Qt's portal plugin. Never disable native
                // dialogs here: the document portal grants access outside the sandbox.
                tracing::info!("Using desktop portal dialogs (Flatpak)");
                DialogBackend::Portal
            }
            Dialogs::Probed(Ok(())) => {
                if crate::qt::ffi::portal_theme_loaded() {
                    tracing::info!("Using desktop portal dialogs");
                    DialogBackend::Portal
                } else {
                    tracing::warn!(
                        "Qt xdgdesktopportal theme was not loaded; using Qt Quick dialogs"
                    );
                    use_quick_dialogs()
                }
            }
            Dialogs::Probed(Err(error)) => {
                tracing::warn!(
                    error = &error as &dyn std::error::Error,
                    "Desktop portal file selection unavailable; using Qt Quick dialogs"
                );
                use_quick_dialogs()
            }
            // The startup probe already reported this outcome.
            Dialogs::BypassPortal => use_quick_dialogs(),
        }
    }
}

fn use_quick_dialogs() -> DialogBackend {
    crate::qt::ffi::use_qt_quick_dialogs();
    DialogBackend::QtQuick
}

#[cfg(feature = "native_tests")]
#[derive(Clone, Copy, clap::ValueEnum)]
pub enum PortalFixture {
    Available,
    OldChooser,
    Bypass,
}

#[cfg(feature = "native_tests")]
pub fn check_portal_fixture(fixture: PortalFixture) -> i32 {
    // SAFETY: A fresh test subprocess has no Qt application or worker threads.
    let DialogSetup(actual) = unsafe { DialogSetup::prepare() };
    match fixture {
        PortalFixture::Available => assert!(matches!(actual, Dialogs::Probed(Ok(())))),
        PortalFixture::OldChooser => assert!(matches!(
            actual,
            Dialogs::Probed(Err(portal::ChooserError::UnsupportedVersion(2)))
        )),
        PortalFixture::Bypass => assert!(matches!(actual, Dialogs::BypassPortal)),
    }
    let theme = if matches!(fixture, PortalFixture::Bypass) {
        assert_eq!(
            std::env::var("GIO_USE_PROXY_RESOLVER").as_deref(),
            Ok("dummy")
        );
        "generic"
    } else {
        assert_eq!(
            std::env::var("GIO_USE_PROXY_RESOLVER").as_deref(),
            Ok("fixture")
        );
        "xdgdesktopportal"
    };
    assert_eq!(std::env::var("QT_QPA_PLATFORMTHEME").as_deref(), Ok(theme));
    0
}

/// Let Qt select the display backend, except for VA's Wayland/EGL requirement.
/// Explicit environment settings always take precedence.
///
/// # Safety
/// Call only at process startup before Qt, GStreamer or worker threads start.
/// No other thread may read or write the process environment during this call.
pub unsafe fn configure_at_startup(mode: crate::playback::deinterlace::Mode) {
    if mode != crate::playback::deinterlace::Mode::VaApi {
        return;
    }
    if std::env::var_os("QT_QPA_PLATFORM").is_none() {
        // qml6glsink <= 1.28.2 assumes GLX for xcb, even if Qt uses EGL.
        // VA DMA_DRM import requires EGL, so this backend needs Wayland.
        // A missing Wayland display is an error, not a reason to use X11/CPU.
        unsafe {
            std::env::set_var("QT_QPA_PLATFORM", "wayland");
        }
    }
    if std::env::var_os("GST_GL_PLATFORM").is_none() {
        // DMA_DRM import requires EGL. Explicit choices remain authoritative
        // and incompatible choices fail, without falling back to CPU mapping.
        unsafe {
            std::env::set_var("GST_GL_PLATFORM", "egl");
        }
    }
}
