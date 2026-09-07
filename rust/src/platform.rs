//! Linux startup compatibility policy; environment hints are not device validation.
use std::ffi::OsStr;

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
        eprintln!(
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
