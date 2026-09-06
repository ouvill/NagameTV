//! Calendar selection owns no EPG copy. Closed guides cannot accept day changes.
use super::ffi;
use crate::features::program_info::guide::{DayWindow, Guide};
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::pin::Pin;

impl ffi::Player {
    pub fn guide_open(mut self: Pin<&mut Self>, open: bool) {
        self.as_mut().rust_mut().guide = if open && self.rust().epg_enabled {
            Guide::AwaitingDay
        } else {
            Guide::Closed
        };
        self.as_mut().rust_mut().guide_dirty = false;
        self.set_epg_data(QString::from("[]"));
    }
    pub fn guide_day(mut self: Pin<&mut Self>, start: f64, end: f64) {
        if matches!(self.rust().guide, Guide::Closed) {
            return;
        }
        let window = match DayWindow::new(start, end) {
            Ok(window) => window,
            Err(error) => {
                eprintln!("{error}");
                return;
            }
        };
        if matches!(self.rust().guide, Guide::Showing(previous) if previous == window) {
            return;
        }
        self.as_mut().rust_mut().guide = Guide::Showing(window);
        self.as_mut().rust_mut().guide_dirty = true;
        self.set_epg_data(QString::from("[]"));
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
            Err(error) => QString::from(error.to_string()),
        }
    }
    pub fn refresh_epg(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().epg.refresh();
    }
}
