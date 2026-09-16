//! Linux startup compatibility policy; environment hints are not device validation.
use std::ffi::OsStr;

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
                    crate::player::ffi::use_qt_quick_dialogs();
                    tracing::info!(%reason, "Desktop portal unavailable; using Qt Quick dialogs");
                    DialogBackend::QtQuick
                }
            },
        }
    }
}

fn portal_available() -> Result<(), String> {
    if !crate::player::ffi::portal_theme_loaded() {
        return Err("Qt xdgdesktopportal platform theme is not loaded".into());
    }
    let version =
        crate::player::ffi::portal_file_chooser_version().map_err(|error| error.to_string())?;
    // FileChooser version 3 introduced directory selection, which is needed by
    // the screenshot folder picker as well as the recording file picker.
    if version < 3 {
        return Err(format!(
            "FileChooser portal version {version} does not support folders"
        ));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Policy {
    Preserve,
    XcbCompatibility,
}

impl Policy {
    fn select(qpa: Option<&OsStr>, wayland: Option<&OsStr>, x11: Option<&OsStr>) -> Self {
        if qpa.is_none()
            && wayland.is_some_and(|value| !value.is_empty())
            && x11.is_some_and(|value| !value.is_empty())
        {
            Self::XcbCompatibility
        } else {
            Self::Preserve
        }
    }
}

/// Apply main's temporary qml6glsink Wayland compatibility default.
///
/// # Safety
/// Call only at process startup before Qt, GStreamer or worker threads start.
/// No other thread may read or write the process environment during this call.
pub unsafe fn configure_at_startup() {
    let policy = Policy::select(
        std::env::var_os("QT_QPA_PLATFORM").as_deref(),
        std::env::var_os("WAYLAND_DISPLAY").as_deref(),
        std::env::var_os("DISPLAY").as_deref(),
    );
    if policy == Policy::XcbCompatibility {
        // Preserve main's workaround until native Wayland rendering is verified.
        // Related report: https://gitlab.freedesktop.org/gstreamer/gstreamer/-/work_items/5178
        // SAFETY: The caller guarantees exclusive startup access to the environment.
        // Both literals are nonempty keys/valid values with no embedded NUL or '=' in the key.
        unsafe { std::env::set_var("QT_QPA_PLATFORM", "xcb") };
        tracing::info!(
            "Using xcb compatibility mode for video rendering; \
             set QT_QPA_PLATFORM=wayland explicitly to test native Wayland"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStrExt;

    #[test]
    fn compatibility_requires_both_display_hints_and_no_override() {
        for (wayland, x11, expected) in [
            (None, None, Policy::Preserve),
            (Some(""), Some(""), Policy::Preserve),
            (None, Some(":99"), Policy::Preserve),
            (Some(""), Some(":99"), Policy::Preserve),
            (Some("wayland-0"), None, Policy::Preserve),
            (Some("wayland-0"), Some(""), Policy::Preserve),
            (Some("wayland-0"), Some(":99"), Policy::XcbCompatibility),
        ] {
            assert_eq!(
                Policy::select(None, wayland.map(OsStr::new), x11.map(OsStr::new)),
                expected
            );
        }
    }

    #[test]
    fn explicit_platform_is_preserved_even_if_empty_or_non_unicode() {
        let display = Some(OsStr::new(":99"));
        for override_value in [
            OsStr::new(""),
            OsStr::new("wayland"),
            OsStr::new("xcb"),
            OsStr::from_bytes(b"\xff"),
        ] {
            assert_eq!(
                Policy::select(Some(override_value), display, display),
                Policy::Preserve
            );
        }
        let non_unicode = Some(OsStr::from_bytes(b"\xff"));
        assert_eq!(
            Policy::select(None, non_unicode, non_unicode),
            Policy::XcbCompatibility
        );
    }
}
