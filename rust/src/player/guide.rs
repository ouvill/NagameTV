//! Calendar selection owns no EPG copy. Closed guides cannot accept day changes.
use super::ffi;
use crate::features::program_info::guide::{DayWindow, Guide};
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::pin::Pin;

impl ffi::Player {
    pub fn guide_visible(&self) -> bool {
        match self.rust().guide {
            Guide::Closed => false,
            Guide::AwaitingDay | Guide::Showing(_) => true,
        }
    }

    pub fn guide_open(mut self: Pin<&mut Self>, open: bool) {
        let open = open && self.rust().epg_enabled;
        if open == self.guide_visible() {
            return;
        }
        if open {
            self.as_mut().refresh_channels(true);
            self.as_mut().refresh_epg();
        }
        self.as_mut().rust_mut().guide = if open {
            Guide::AwaitingDay
        } else {
            Guide::Closed
        };
        self.as_mut()
            .set_guide_visibility_data(QString::from("null"));
        self.as_mut().rust_mut().next_current_program = std::time::Instant::now();
        self.as_mut().rust_mut().guide_dirty = false;
        self.as_mut().rust_mut().guide_error = None;
        self.as_mut().refresh_epg_status();
        self.as_mut().set_epg_data(QString::from("[]"));
        // The QML Loader may synchronously request its first day on this signal.
        // Publish visibility only after the backend is ready to accept that day.
        self.guide_visible_changed();
    }
    pub(super) fn poll_guide_visibility(self: Pin<&mut Self>, now: Option<u64>) {
        if matches!(self.rust().guide, Guide::Closed) {
            return;
        }
        // Only channel indices cross Qt. Reuse the EPG; no card summaries or
        // full schedule serialization is needed when the current broadcast changes.
        let indices = self.rust().epg.visible_channels(&self.rust().entries, now);
        match serde_json::to_string(&indices) {
            Ok(json) => self.set_guide_visibility_data(QString::from(json)),
            Err(error) => tracing::error!("Guide channel visibility failed: {error}"),
        }
    }
    pub fn guide_day(mut self: Pin<&mut Self>, start: f64, end: f64) {
        if matches!(self.rust().guide, Guide::Closed) {
            return;
        }
        let window = match DayWindow::new(start, end) {
            Ok(window) => window,
            Err(error) => {
                tracing::error!("{error}");
                return;
            }
        };
        if matches!(self.rust().guide, Guide::Showing(previous) if previous == window) {
            return;
        }
        self.as_mut().rust_mut().guide = Guide::Showing(window);
        self.as_mut().rust_mut().guide_dirty = true;
        self.as_mut().rust_mut().guide_error = None;
        // QML changes the calendar bounds in this same event. Replace the data
        // before returning to rendering, without an empty frame or a poll delay.
        self.as_mut().publish_guide();
        self.refresh_epg_status();
    }
    pub fn watch_program(mut self: Pin<&mut Self>, key: QString) -> QString {
        use crate::features::program_info::watch::Error;
        let result = (|| {
            if !self.rust().epg_enabled || !matches!(self.rust().guide, Guide::Showing(_)) {
                return Err(Error::Unavailable);
            }
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .and_then(|time| u64::try_from(time.as_millis()).ok())
                .ok_or(Error::Unavailable)?;
            let index =
                self.rust()
                    .epg
                    .watch_channel(&key.to_string(), &self.rust().entries, now)?;
            i32::try_from(index).map_err(|_| Error::Unavailable)
        })();
        match result {
            Ok(index) => {
                self.as_mut().guide_open(false);
                self.select(index);
                QString::default()
            }
            Err(error) => QString::from(match error {
                Error::Unavailable => "Program information has changed. Select the program again.",
                Error::NotLive => "This program is not currently on air.",
            }),
        }
    }
    pub fn refresh_epg(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().epg.refresh();
    }
}
