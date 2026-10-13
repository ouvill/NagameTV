//! QObject commands delegate eligibility and request ordering to the Rust owner.
use super::{ffi, status::tr};
use crate::epgstation::reservations::{Error, Program};
use crate::services::FetchError;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::{
    pin::Pin,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

fn now_ms() -> Result<u64, Error> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| Error::Unavailable)
        .and_then(|duration| u64::try_from(duration.as_millis()).map_err(|_| Error::Unavailable))
}
impl ffi::Player {
    pub fn recording_reservations_enabled(&self) -> bool {
        self.rust()
            .preferences
            .preferences()
            .recording_reservations_enabled
    }
    pub fn reservations_enabled(&self) -> bool {
        self.rust().recording_reservations.enabled()
    }
    pub fn reservation_busy(&self) -> bool {
        self.rust().recording_reservations.busy()
    }
    pub fn reservation_error(&self) -> QString {
        let Some(error) = self.rust().recording_reservations.error() else {
            return QString::default();
        };
        match error {
            FetchError::Parse(Error::Unavailable) => {
                tr("Connect to EPGStation and enable recording reservations first.")
            }
            FetchError::Parse(Error::Busy) => tr("A reservation operation is already running."),
            FetchError::Parse(Error::ProgramChanged) => tr(
                "Program information has changed or the program has ended. Refresh the guide and try again.",
            ),
            FetchError::Parse(Error::AlreadyReserved) => {
                tr("This program already has a reservation. Refresh the reservation list.")
            }
            FetchError::Parse(Error::NotCancellable) => {
                tr("Only a single manual reservation that has not started can be cancelled.")
            }
            FetchError::Parse(Error::RefreshAfterWrite(_)) => tr(
                "The reservation was changed, but its status could not be refreshed. Refresh the reservation list before trying again.",
            ),
            FetchError::Parse(Error::Http(error))
                if error
                    .status()
                    .is_some_and(|status| matches!(status.as_u16(), 401 | 403)) =>
            {
                tr("Authentication failed. Check your username and password, then sign in again.")
            }
            FetchError::Parse(Error::Invalid | Error::Json { .. } | Error::Limit) => {
                tr("EPGStation returned an invalid or oversized reservation list.")
            }
            FetchError::Parse(Error::Status(_) | Error::Http(_)) | FetchError::Network(_) => tr(
                "Could not update recording reservations. Check the connection and refresh the reservation list.",
            ),
        }
    }
    pub fn configure_recording_reservations(mut self: Pin<&mut Self>, enabled: bool) {
        if enabled && !self.rust().recording_reservations_allowed {
            return;
        }
        if self.recording_reservations_enabled() != enabled {
            self.as_mut().rust_mut().preferences.change(
                crate::settings::Change::RecordingReservationsEnabled(enabled),
            );
            self.as_mut().poll_recording_reservations();
            self.as_mut().recording_reservations_enabled_changed();
        }
        self.save_settings();
    }
    fn reservation_program(&self, key: &QString) -> Result<Program, Error> {
        if !self.rust().epg_enabled
            || !matches!(
                self.rust().guide,
                crate::features::program_info::guide::Guide::Showing(_)
            )
        {
            return Err(Error::Unavailable);
        }
        self.rust()
            .epg
            .reservation_program(&key.to_string(), self.rust().catalog.channels())
            .map_err(|_| Error::ProgramChanged)
    }
    pub fn reservation_state(&self, program_key: QString) -> QString {
        let Ok(program) = self.reservation_program(&program_key) else {
            return QString::default();
        };
        QString::from(
            self.rust()
                .recording_reservations
                .snapshot()
                .status(program)
                .map_or("", crate::epgstation::reservations::Status::as_str),
        )
    }
    pub fn can_reserve_program(&self, program_key: QString) -> bool {
        match (self.reservation_program(&program_key), now_ms()) {
            (Ok(program), Ok(now)) => self.rust().recording_reservations.can_reserve(program, now),
            _ => false, // Unavailable selections and clocks expose no action.
        }
    }
    pub fn can_cancel_reservation(&self, program_key: QString) -> bool {
        match (self.reservation_program(&program_key), now_ms()) {
            (Ok(program), Ok(now)) => self.rust().recording_reservations.can_cancel(program, now),
            _ => false,
        }
    }
    pub fn reserve_program(self: Pin<&mut Self>, program_key: QString) -> bool {
        self.change_reservation(program_key, false)
    }
    pub fn cancel_reservation(self: Pin<&mut Self>, program_key: QString) -> bool {
        self.change_reservation(program_key, true)
    }
    fn change_reservation(mut self: Pin<&mut Self>, key: QString, cancel: bool) -> bool {
        self.as_mut().poll_recording_reservations();
        let target = self
            .reservation_program(&key)
            .and_then(|program| Ok((program, now_ms()?)));
        let this = self.as_mut().rust_mut().get_mut();
        let accepted = match (target, this.network.as_ref()) {
            (Ok((program, now)), Some(network)) => {
                if cancel {
                    this.recording_reservations.cancel(network, program, now)
                } else {
                    this.recording_reservations.reserve(network, program, now)
                }
            }
            (Err(error), _) => this.recording_reservations.reject(error),
            (Ok(_), None) => this.recording_reservations.reject(Error::Unavailable),
        };
        self.publish_reservations();
        accepted
    }
    pub fn refresh_reservations(mut self: Pin<&mut Self>) -> bool {
        self.as_mut().poll_recording_reservations();
        let this = self.as_mut().rust_mut().get_mut();
        let accepted = match this.network.as_ref() {
            Some(network) => this.recording_reservations.refresh(network),
            None => this.recording_reservations.reject(Error::Unavailable),
        };
        self.publish_reservations();
        accepted
    }
    pub(super) fn poll_recording_reservations(mut self: Pin<&mut Self>) {
        let now = Instant::now();
        let state = self.as_mut().rust_mut().get_mut();
        let before = state.recording_reservations.revision();
        state.recording_library.synchronize_reservations(
            state
                .preferences
                .preferences()
                .recording_reservations_enabled,
            &mut state.recording_reservations,
            now,
        );
        if let Some(network) = &state.network {
            state.recording_reservations.poll(network, now);
        }
        if before != state.recording_reservations.revision() {
            self.publish_reservations();
        }
    }
    fn publish_reservations(mut self: Pin<&mut Self>) {
        let snapshot = self.rust().recording_reservations.snapshot();
        self.as_mut()
            .rust_mut()
            .guide_model
            .pin_mut()
            .replace_reservations(snapshot);
        self.reservations_changed();
    }
}
