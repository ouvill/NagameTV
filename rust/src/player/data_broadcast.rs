mod diagnostics;
mod mode;
use super::ffi;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
pub(super) use mode::Mode;
use std::pin::Pin;

impl ffi::Player {
    pub fn data_broadcast_enabled(&self) -> bool {
        self.rust().preferences.preferences().data_broadcast_enabled
    }

    pub fn configure_data_broadcast(mut self: Pin<&mut Self>, enabled: bool) {
        if enabled && !self.rust().data_broadcast_allowed {
            return;
        }
        if self.data_broadcast_enabled() != enabled {
            self.as_mut()
                .rust_mut()
                .preferences
                .change(crate::settings::Change::DataBroadcastEnabled(enabled));
            // Commit receiver, mode and endpoint before notifying any observer.
            self.as_mut().change_stream_state(|state| state);
            self.as_mut().data_broadcast_enabled_changed();
        }
        self.save_settings();
    }

    pub fn data_broadcast_prefetch(&self) -> bool {
        self.rust()
            .preferences
            .preferences()
            .data_broadcast_prefetch
    }

    pub fn configure_data_broadcast_prefetch(mut self: Pin<&mut Self>, enabled: bool) {
        if self.data_broadcast_prefetch() != enabled {
            self.as_mut()
                .rust_mut()
                .preferences
                .change(crate::settings::Change::DataBroadcastPrefetch(enabled));
            // Reconcile via the same committed projection as stream changes,
            // including endpoint/mode notifications if applying the request fails.
            self.as_mut().change_stream_state(|state| state);
            self.as_mut().data_broadcast_prefetch_changed();
        }
        self.save_settings();
    }

    pub fn data_broadcast_receiving(&self) -> bool {
        self.rust()
            .media
            .data_broadcast()
            .is_some_and(crate::features::data_broadcast::Session::receiving)
    }

    pub fn data_broadcast_console(&self, level: i32, message: QString, source: QString, line: i32) {
        diagnostics::record_console(level, &message.to_string(), &source.to_string(), line);
    }

    pub fn data_broadcast_available(&self) -> bool {
        self.rust().media.data_broadcast().is_some()
    }

    pub fn data_broadcast_connected(&self) -> bool {
        self.rust()
            .media
            .data_broadcast()
            .is_some_and(crate::features::data_broadcast::Session::connected)
    }

    pub fn data_broadcast_requested(&self) -> bool {
        self.rust().data_broadcast_mode.requested()
    }
    pub fn data_broadcast_activate(&self) -> bool {
        self.rust().data_broadcast_mode.activate()
    }

    pub fn data_broadcast_open(mut self: Pin<&mut Self>, open: bool) -> bool {
        if open && !self.data_broadcast_enabled() {
            return false;
        }
        let before_requested = self.data_broadcast_requested();
        let before_activate = self.data_broadcast_activate();
        let before_endpoint = self.rust().data_broadcast_endpoint.clone();
        {
            let mut this = self.as_mut().rust_mut();
            let this = &mut *this;
            if open {
                this.data_broadcast_mode.open(&this.stream_state);
            } else {
                this.data_broadcast_mode.dismiss();
            }
        }
        synchronize(&mut self.as_mut().rust_mut());
        let accepted = self.data_broadcast_requested() == open;
        if before_requested != self.data_broadcast_requested() {
            self.as_mut().data_broadcast_requested_changed();
        }
        if before_activate != self.data_broadcast_activate() {
            self.as_mut().data_broadcast_activate_changed();
        }
        if before_endpoint != self.rust().data_broadcast_endpoint {
            self.as_mut().data_broadcast_endpoint_changed();
        }
        accepted
    }

    pub fn data_broadcast_url(&self) -> QString {
        self.rust().data_broadcast_endpoint.clone()
    }
}

/// Reconcile user intent with the playback-owned receive session before Qt observes it.
/// A same-service reconnect can temporarily have intent without an endpoint.
pub(super) fn synchronize(this: &mut super::PlayerRust) {
    this.data_broadcast_endpoint = QString::default();
    let enabled = this.preferences.preferences().data_broadcast_enabled;
    if let Err(error) = this.media.configure_data_broadcast(enabled) {
        tracing::error!(
            operation = "configure data broadcast receiver",
            error = &error as &dyn std::error::Error
        );
        this.data_broadcast_mode.dismiss();
    }
    if !enabled {
        this.data_broadcast_mode.close();
        return;
    }
    if let Some(session) = this.media.data_broadcast() {
        this.data_broadcast_mode
            .observe(&this.stream_state, session.entry());
        let demand = this.data_broadcast_mode.demand(
            this.preferences.preferences().data_broadcast_prefetch,
            &this.stream_state,
        );
        if let Err(error) = session.configure(demand) {
            tracing::error!(
                operation = "apply data broadcast intent",
                error = &error as &dyn std::error::Error
            );
            // Do not turn a failed automatic startup into a retry on every
            // projection of the same PMT.
            this.data_broadcast_mode.dismiss();
        } else if this.data_broadcast_mode.requested() {
            this.data_broadcast_endpoint = QString::from(session.url());
        }
    }
}
