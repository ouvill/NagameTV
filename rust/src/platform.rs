//! Linux startup defaults; environment hints are not device validation.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Packaging {
    Native,
    Flatpak,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogBackend {
    Portal,
    QtQuick,
}

/// Prepared before Qt starts; consumed after its platform theme has loaded.
pub struct DialogSetup {
    packaging: Packaging,
}

impl DialogSetup {
    /// Select the portal platform theme before Qt or any worker starts.
    ///
    /// # Safety
    /// No other thread may access the process environment during this call.
    /// QGuiApplication must not have been constructed yet.
    pub unsafe fn prepare() -> Self {
        // Qt's portal theme delegates appearance to the desktop theme. Choosing
        // it explicitly also enables portals outside Flatpak, without changing
        // the X11/Wayland backend or display scaling.
        unsafe { std::env::set_var("QT_QPA_PLATFORMTHEME", "xdgdesktopportal") };
        Self {
            packaging: if std::path::Path::new("/.flatpak-info").exists() {
                Packaging::Flatpak
            } else {
                Packaging::Native
            },
        }
    }

    /// Finish before loading QML. The application borrow proves Qt is initialized.
    pub fn finish(self, _app: &cxx_qt_lib::QGuiApplication) -> DialogBackend {
        match self.packaging {
            Packaging::Flatpak => {
                // The runtime supplies Qt's portal plugin. Never disable native
                // dialogs here: the document portal grants access outside the sandbox.
                tracing::info!("Using desktop portal dialogs (Flatpak)");
                DialogBackend::Portal
            }
            Packaging::Native => match portal_available() {
                Ok(()) => {
                    tracing::info!("Using desktop portal dialogs");
                    DialogBackend::Portal
                }
                Err(reason) => {
                    // Qt's portal plugin can fall back to the desktop's GTK3
                    // dialogs. Avoid that path when portal support is unavailable.
                    crate::qt::ffi::use_qt_quick_dialogs();
                    tracing::info!(%reason, "Desktop portal unavailable; using Qt Quick dialogs");
                    DialogBackend::QtQuick
                }
            },
        }
    }
}

#[derive(Debug, thiserror::Error)]
enum PortalError {
    #[error("Qt xdgdesktopportal platform theme is not loaded")]
    ThemeUnavailable,
    #[error("Could not query FileChooser portal: {0}")]
    Query(#[from] cxx::Exception),
    #[error("FileChooser portal version {0} does not support folders")]
    UnsupportedVersion(u32),
}

fn portal_available() -> Result<(), PortalError> {
    if !crate::qt::ffi::portal_theme_loaded() {
        return Err(PortalError::ThemeUnavailable);
    }
    let version = crate::qt::ffi::portal_file_chooser_version()?;
    // FileChooser version 3 introduced directory selection, which is needed by
    // the screenshot folder picker as well as the recording file picker.
    if version < 3 {
        return Err(PortalError::UnsupportedVersion(version));
    }
    Ok(())
}

/// Let Qt select the display backend, except for VA's Wayland/EGL requirement.
/// Explicit environment settings always take precedence.
///
/// # Safety
/// Call only at process startup before Qt, GStreamer or worker threads start.
/// No other thread may read or write the process environment during this call.
pub unsafe fn configure_at_startup() {
    let mode = crate::playback::deinterlace::Mode::from_environment();
    if matches!(mode, Ok(crate::playback::deinterlace::Mode::VaApi))
        && std::env::var_os("QT_QPA_PLATFORM").is_none()
    {
        // qml6glsink <= 1.28.2 assumes GLX for xcb, even if Qt uses EGL.
        // VA DMA_DRM import requires EGL, so this backend needs Wayland.
        // A missing Wayland display is an error, not a reason to use X11/CPU.
        unsafe {
            std::env::set_var("QT_QPA_PLATFORM", "wayland");
        }
    }
    if matches!(mode, Ok(crate::playback::deinterlace::Mode::VaApi))
        && std::env::var_os("GST_GL_PLATFORM").is_none()
    {
        // DMA_DRM import requires EGL. Explicit choices remain authoritative
        // and incompatible choices fail, without falling back to CPU mapping.
        unsafe {
            std::env::set_var("GST_GL_PLATFORM", "egl");
        }
    }
}
