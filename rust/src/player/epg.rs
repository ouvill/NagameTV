//! Connect the EPG lifecycle to Qt projections without owning another program snapshot.
use super::ffi;
use crate::features::program_info::Completion;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::pin::Pin;
use viewer_diagnostics::recorder::Event;

impl ffi::Player {
    pub(super) fn configure_epg_events(mut self: Pin<&mut Self>) {
        let endpoint = (self.rust().epg_enabled && self.rust().channel_refresh.enabled())
            .then(|| format!("{}/api/events/stream?resource=program", self.server()));
        self.as_mut().rust_mut().epg_events.configure(endpoint);
    }
    pub(super) fn configure_epg(mut self: Pin<&mut Self>) {
        self.as_mut().configure_epg_events();
        let server = if self.rust().epg_enabled && !self.rust().entries.is_empty() {
            Some(self.server().to_string())
        } else {
            None
        };
        self.as_mut().rust_mut().epg.configure(server);
        if !self.rust().epg_enabled {
            self.as_mut().guide_open(false);
        }
    }
    pub(super) fn poll_epg_events(mut self: Pin<&mut Self>) {
        let events = {
            let mut this = self.as_mut().rust_mut();
            let this = &mut *this;
            this.network
                .as_ref()
                .map(|network| network.poll_epg_events(&mut this.epg_events))
        };
        match events {
            Some(Ok(update)) => {
                if let Some(error) = update.failure {
                    eprintln!("EPG event stream: {error}");
                }
                if update.refresh {
                    self.as_mut().rust_mut().epg.refresh();
                    self.as_mut().refresh_channels(true);
                }
            }
            Some(Err(error)) => eprintln!("EPG event subscription: {error}"),
            None => {}
        }
    }
    pub(super) fn poll_epg(mut self: Pin<&mut Self>) {
        let update = {
            let mut this = self.as_mut().rust_mut();
            let this = &mut *this;
            this.network.as_ref().map(|network| this.epg.poll(network))
        };
        if let Some(update) = update {
            if let Some(completed) = update.completed {
                self.record_diagnostic(match completed {
                    Completion::Succeeded => Event::EpgFetchFinished,
                    Completion::Failed => Event::EpgFetchFailed,
                });
            }
            if update.started {
                self.record_diagnostic(Event::EpgFetchStarted);
            }
        }
        let service = self
            .rust()
            .entries
            .get(self.rust().selected as usize)
            .and_then(|s| s.broadcast);
        self.as_mut().poll_current_program(service);
        if let crate::features::program_info::guide::Guide::Showing(window) = self.rust().guide
            && (self.rust().guide_revision != self.rust().epg.revision
                || self.rust().guide_service != service
                || self.rust().guide_dirty)
        {
            let data = match self.rust().epg.grid_view(&self.rust().entries, window) {
                Ok(data) => {
                    self.as_mut().rust_mut().guide_error = None;
                    data
                }
                Err(error) => {
                    eprintln!("Program guide presentation failed: {error}");
                    self.as_mut().rust_mut().guide_error = Some(error);
                    "[]".into()
                }
            };
            let revision = self.rust().epg.revision;
            self.as_mut().rust_mut().guide_revision = revision;
            self.as_mut().rust_mut().guide_service = service;
            self.as_mut().rust_mut().guide_dirty = false;
            self.as_mut().set_epg_data(QString::from(data));
        }
        self.refresh_epg_status();
    }
}
