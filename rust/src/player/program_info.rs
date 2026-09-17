//! Current-program projection runs on a one-second tick or a catalog/selection change.
use super::ffi;
use crate::channels::BroadcastService;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::{
    pin::Pin,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

impl ffi::Player {
    pub(super) fn poll_current_program(
        mut self: Pin<&mut Self>,
        service: Option<BroadcastService>,
    ) {
        if self.recording() || self.media_active() {
            // Live channel/guide views remain usable during recording, but their
            // wall clock is never fed into the recording's program timeline.
            if Instant::now() >= self.rust().next_current_program {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .ok()
                    .and_then(|d| u64::try_from(d.as_millis()).ok());
                self.as_mut().poll_channel_programs(now);
                self.as_mut().poll_guide_visibility(now);
                self.as_mut().rust_mut().next_current_program =
                    Instant::now() + Duration::from_secs(1);
            }
            // Stream state publishes position, TS metadata and progress together.
            return;
        }

        let now = Instant::now();
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|duration| u64::try_from(duration.as_millis()).ok());
        // An invalid wall clock cannot identify a current broadcast.
        let service = now_ms.and(service);
        if now < self.rust().next_current_program
            && !self
                .rust()
                .current_projection
                .is_stale(self.rust().epg.revision, service)
        {
            return;
        }
        self.as_mut().poll_channel_programs(now_ms);
        self.as_mut().poll_guide_visibility(now_ms);
        let update = {
            let mut this = self.as_mut().rust_mut();
            let this = &mut *this;
            this.next_current_program = now + Duration::from_secs(1);
            this.epg.current_presentation(
                &mut this.current_projection,
                service,
                now_ms.unwrap_or(0),
            )
        };
        match update {
            Ok(update) => {
                if let Some(data) = update.data {
                    self.as_mut().set_current_program_data(QString::from(data));
                }
                self.set_program_progress(update.progress);
            }
            Err(error) => {
                tracing::error!("Current program presentation failed: {error}");
                self.as_mut()
                    .set_current_program_data(QString::from("null"));
                self.set_program_progress(0.0);
            }
        }
    }
}
