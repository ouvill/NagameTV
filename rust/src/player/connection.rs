//! Qt orchestration for server changes, channel acquisition and selection.
//! Pure catalog projection and identity rules remain in `channels`.
use super::{PlaybackStatus, StatusFailure, channel_refresh, ffi};
use crate::services;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::{pin::Pin, time::Instant};

impl ffi::Player {
    pub fn server_configured(&self) -> bool {
        !self
            .rust()
            .preferences
            .preferences()
            .server
            .trim()
            .is_empty()
    }

    pub fn loading(&self) -> bool {
        self.rust().pending_server.is_some()
    }

    fn begin_connection(mut self: Pin<&mut Self>, server: services::ServerUrl) {
        let was_loading = self.loading();
        self.as_mut().rust_mut().pending_server = Some(server.clone());
        self.as_mut().set_server(QString::from(server.as_str()));
        if !was_loading {
            self.loading_changed();
        }
    }

    pub(super) fn cancel_connection(mut self: Pin<&mut Self>) {
        if self.as_mut().rust_mut().pending_server.take().is_some() {
            self.loading_changed();
        }
    }

    fn confirm_server(mut self: Pin<&mut Self>, server: &services::VerifiedServer) {
        let was_configured = self.server_configured();
        self.as_mut().rust_mut().preferences.confirm_server(server);
        self.as_mut().save_settings();
        if was_configured != self.server_configured() {
            self.server_configured_changed();
        }
    }

    /// Reports request acceptance; HTTP completion is delivered later by poll_channels.
    pub fn connect_server(mut self: Pin<&mut Self>, server: QString) -> bool {
        // Reject invalid input before cancelling requests or stopping the current
        // broadcast. An input error changes only the status shown to the user.
        let server = match services::ServerUrl::parse(&server.to_string()) {
            Ok(server) => server,
            Err(error) => {
                self.status_error(StatusFailure::Server, error);
                return false;
            }
        };
        if self.rust().network.is_none() {
            self.update_status(PlaybackStatus::NetworkUnavailable);
            return false;
        }

        // A saved startup URL alone is not an established session. Once requests
        // are scheduled, reconnecting to that same URL only refreshes the catalog.
        if self.rust().channel_refresh.enabled() && self.server().to_string() == server.as_str() {
            self.as_mut().begin_connection(server);
            if !self.rust().request.is_busy() {
                self.as_mut().refresh_channels(true);
                if self.rust().epg_enabled {
                    self.as_mut().refresh_epg();
                }
                self.as_mut().update_status(PlaybackStatus::Loading);
            }
            return true;
        }

        self.as_mut().rust_mut().epg_events.configure(None);
        self.as_mut().rust_mut().channel_refresh = channel_refresh::Refresh::Disabled;
        self.as_mut().rust_mut().comments.configure(false, None);
        self.as_mut().set_comment_draft(QString::default());
        self.as_mut().refresh_comment_posting();
        self.as_mut().clear_comment_history();
        self.as_mut().rust_mut().activity.configure(false);
        self.as_mut().set_activity_data(QString::from("[]"));
        self.as_mut().set_comment_program_title(QString::default());
        self.as_mut().clear_playback_failure();
        self.as_mut().rust_mut().request.cancel();
        self.as_mut().cancel_connection();
        self.as_mut().rust_mut().epg.configure(None);
        self.as_mut()
            .rust_mut()
            .guide_model
            .pin_mut()
            .replace(Default::default());
        self.as_mut().cancel_recording_open();
        if let Err(error) = self.as_mut().end_stream() {
            self.playback_failed(error);
            return false;
        }
        self.as_mut().set_channel_program_data(QString::from("[]"));
        self.as_mut()
            .replace_catalog(crate::channels::catalog::Catalog::default(), "");

        // Keep the candidate separate from persisted preferences until the HTTP
        // response has been parsed successfully. Other settings and shutdown may
        // be saved while this request is in flight or after it has failed.
        self.as_mut().rust_mut().request.request(server.clone());
        self.as_mut()
            .rust_mut()
            .channel_refresh
            .requested(Instant::now());
        self.as_mut().begin_connection(server);
        self.as_mut().configure_epg_events();
        self.update_status(PlaybackStatus::Loading);
        true
    }
    pub(super) fn finish_connection(mut self: Pin<&mut Self>, success: bool) {
        if !self.loading() {
            return;
        }
        self.as_mut().cancel_connection();
        let count = i32::try_from(self.rust().catalog.channels().len()).unwrap_or(i32::MAX);
        self.as_mut().connection_finished(success, count);
    }
    pub fn select(mut self: Pin<&mut Self>, index: i32) {
        if index < 0 || index as usize >= self.rust().catalog.channels().len() {
            return;
        }
        let id = self.rust().catalog.channels()[index as usize].id;
        self.as_mut()
            .rust_mut()
            .preferences
            .change(crate::settings::Change::Service(id.to_string()));
        self.as_mut().set_selected(index);
        self.record_diagnostic(viewer_diagnostics::recorder::Event::ChannelSelected);
        self.as_mut().save_settings();
        let attempt = super::stream_state::Attempt::new(
            &self.rust().catalog.channels()[index as usize],
            self.rust().preferences.preferences().timeshift_policy(),
        );
        self.as_mut().clear_playback_failure();
        self.start_stream(attempt);
    }
    /// Accept domain results and publish their Qt presentation on the owning thread.
    pub(super) fn poll_channels(mut self: Pin<&mut Self>) {
        let fetched = {
            let mut this = self.as_mut().rust_mut();
            let this = &mut *this;
            this.network
                .as_ref()
                .and_then(|network| this.request.poll(network))
        };
        if let Some(result) = fetched {
            match result {
                Ok(verified) => {
                    let changed = self.rust().catalog.channels() != verified.channels();
                    if self.rust().pending_server.is_some() {
                        self.as_mut().confirm_server(&verified);
                    }
                    // Unchanged catalogs do not reset views or disturb their cursors.
                    if changed {
                        let server = verified.url().as_str().to_owned();
                        let preferred = self
                            .rust()
                            .preferences
                            .preferences()
                            .service_id
                            .parse()
                            .ok();
                        let before_selected = self.selected();
                        let before_viewing = self.viewing_channel();
                        let before_action = self.playback_action();
                        self.as_mut()
                            .rust_mut()
                            .catalog
                            .replace(verified.into_channels(), preferred);
                        self.as_mut().rust_mut().activity.dirty = true;
                        self.as_mut().rust_mut().guide_dirty = true;
                        // Physical channel metadata also affects subchannel visibility.
                        // Reset the small browser projection even if EPG revision is unchanged.
                        if self.rust().browser_projection.is_some() {
                            self.as_mut().rust_mut().browser_projection = Some(Default::default());
                        }
                        self.as_mut().rust_mut().next_current_program = Instant::now();
                        self.as_mut().publish_catalog(&server);
                        if before_selected != self.selected() {
                            self.as_mut().selected_changed();
                        }
                        if before_action != self.playback_action() {
                            self.as_mut().playback_action_changed();
                        }
                        if before_viewing != self.viewing_channel() {
                            self.as_mut().viewing_channel_changed();
                        }
                        self.as_mut().configure_epg();
                    }
                    let status = if self.rust().catalog.channels().is_empty() {
                        PlaybackStatus::Empty
                    } else if self.rust().catalog.selected().is_some() {
                        PlaybackStatus::Ready
                    } else {
                        PlaybackStatus::Select
                    };
                    if !self.recording() && self.rust().stream_state.active_service().is_none() {
                        self.as_mut().update_status(status);
                    }
                    self.as_mut().finish_connection(true);
                    if self.rust().autoplay_pending && self.selected() >= 0 {
                        self.as_mut().rust_mut().autoplay_pending = false;
                        self.as_mut().play();
                    }
                }
                Err(error) => {
                    // Failed new candidates must not become configured through
                    // an unattended refresh. Existing servers can still recover.
                    if self.rust().preferences.preferences().server != self.server().to_string() {
                        self.as_mut().rust_mut().channel_refresh =
                            channel_refresh::Refresh::Disabled;
                        self.as_mut().configure_epg_events();
                    }
                    self.as_mut()
                        .status_error(StatusFailure::ChannelFetch, error);
                    self.as_mut().finish_connection(false);
                }
            }
        }
    }
}
