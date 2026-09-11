//! Qt boundary for browser-only EPG summaries. None owns no projection resources.
use super::ffi;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::{pin::Pin, time::Instant};

impl ffi::Player {
    pub fn browser_open(mut self: Pin<&mut Self>, open: bool) {
        if open {
            self.as_mut().refresh_channels(true);
            if self.rust().epg_enabled {
                self.as_mut().refresh_epg();
            }
        }
        if open == self.rust().browser_projection.is_some() {
            return;
        }
        self.as_mut().rust_mut().browser_projection = open.then(Default::default);
        self.as_mut().rust_mut().next_current_program = Instant::now();
        self.as_mut().set_channel_program_data(QString::from("[]"));
        self.as_mut()
            .set_channel_visibility_data(QString::from("[]"));
        self.set_channel_program_now(0.0);
    }
    pub(super) fn poll_channel_programs(mut self: Pin<&mut Self>, now: Option<u64>) {
        let update = {
            let mut this = self.as_mut().rust_mut();
            let this = &mut *this;
            let Some(projection) = &mut this.browser_projection else {
                return;
            };
            let current_time = if this.epg_enabled { now } else { None };
            this.epg
                .browser_presentation(projection, &this.entries, current_time)
        };
        match update {
            Ok(Some(json)) => {
                let visible = self
                    .rust()
                    .browser_projection
                    .as_ref()
                    .map(|projection| QString::from(projection.visible_json.as_str()))
                    .unwrap_or_else(|| QString::from("[]"));
                self.as_mut().set_channel_visibility_data(visible);
                self.as_mut().set_channel_program_data(QString::from(json));
            }
            Ok(None) => {}
            Err(error) => {
                tracing::error!("Channel program presentation failed: {error}");
                self.as_mut().set_channel_program_data(QString::from("[]"));
            }
        }
        self.set_channel_program_now(now.map_or(0.0, |value| value as f64));
    }
}
