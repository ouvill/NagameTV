//! Qt orchestration for server changes, channel acquisition and selection.
//! Pure catalog projection and identity rules remain in `channels`.
use super::{PlaybackStatus, StatusFailure, channel_refresh, channels, ffi};
use crate::services;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::{pin::Pin, time::Instant};

impl ffi::Player {
    /// Reports request acceptance; HTTP completion is delivered later by poll_channels.
    pub fn connect_server(mut self: Pin<&mut Self>, server: QString) -> bool {
        // Reject invalid input before cancelling requests or stopping the current
        // broadcast. An input error changes only the status shown to the user.
        let server = match services::server_url(&server.to_string()) {
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
        if self.rust().channel_refresh.enabled() && self.server().to_string() == server {
            self.as_mut().rust_mut().connection_pending = true;
            if !self.rust().request.is_busy() {
                self.as_mut().refresh_channels(true);
                if self.rust().epg_enabled {
                    self.as_mut().refresh_epg();
                }
                self.as_mut().update_status(PlaybackStatus::Loading);
            }
            self.as_mut().set_loading(true);
            return true;
        }

        self.as_mut().rust_mut().epg_events.configure(None);
        self.as_mut().rust_mut().catalog_selection = channels::SelectionPolicy::Initial;
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
        self.as_mut().rust_mut().connection_pending = false;
        self.as_mut().set_loading(false);
        self.as_mut().rust_mut().epg.configure(None);
        self.as_mut().set_epg_data(QString::from("[]"));
        if let Err(error) = self.as_mut().end_stream() {
            self.playback_failed(error);
            return false;
        }
        self.as_mut().set_channel_program_data(QString::from("[]"));
        self.as_mut().rust_mut().entries.clear();
        self.as_mut().set_channel_data(QString::from("[]"));
        self.as_mut().set_selected(-1);

        // Keep the candidate separate from persisted preferences until the HTTP
        // response has been parsed successfully. Other settings and shutdown may
        // be saved while this request is in flight or after it has failed.
        self.as_mut().rust_mut().connection_pending = true;
        self.as_mut().rust_mut().request.request(server.clone());
        self.as_mut()
            .rust_mut()
            .channel_refresh
            .requested(Instant::now());
        self.as_mut().set_server(QString::from(server));
        self.as_mut().configure_epg_events();
        self.as_mut().set_loading(true);
        self.update_status(PlaybackStatus::Loading);
        true
    }
    pub(super) fn finish_connection(mut self: Pin<&mut Self>, success: bool) {
        if !std::mem::take(&mut self.as_mut().rust_mut().connection_pending) {
            return;
        }
        let count = i32::try_from(self.rust().entries.len()).unwrap_or(i32::MAX);
        self.as_mut().connection_finished(success, count);
    }
    pub fn select(mut self: Pin<&mut Self>, index: i32) {
        if index < 0 || index as usize >= self.rust().entries.len() {
            return;
        }
        let id = self.rust().entries[index as usize].id;
        self.as_mut()
            .rust_mut()
            .preferences
            .preferences_mut()
            .service_id = id.to_string();
        self.as_mut().set_selected(index);
        self.record_diagnostic(viewer_diagnostics::recorder::Event::ChannelSelected);
        self.as_mut().save_settings();
        self.play();
    }
    /// Publish a complete catalog or report a projection error to the UI poll loop.
    pub(super) fn poll_channels(mut self: Pin<&mut Self>) -> Result<(), serde_json::Error> {
        let fetched = {
            let mut this = self.as_mut().rust_mut();
            let this = &mut *this;
            this.network
                .as_ref()
                .and_then(|network| this.request.poll(network))
        };
        if let Some(result) = fetched {
            self.as_mut().set_loading(false);
            match result {
                Ok(entries) => {
                    // Validate the presentation before committing the candidate.
                    let presentation = (self.rust().entries != entries)
                        .then(|| channels::presentation(&entries, &self.server().to_string()))
                        .transpose()?;
                    if self.rust().connection_pending {
                        let server = self.server().to_string();
                        self.as_mut()
                            .rust_mut()
                            .preferences
                            .preferences_mut()
                            .apply_overrides(Some(server), None);
                        self.as_mut().save_settings();
                    }
                    // Unchanged catalogs must not rebuild the guide or browser payloads.
                    if let Some(presentation) = presentation {
                        let presentation = QString::from(presentation);
                        let selected = channels::selected_after_update(
                            self.rust().catalog_selection,
                            &self.rust().entries,
                            self.rust().selected,
                            &entries,
                            self.rust().preferences.preferences(),
                        )
                        .and_then(|index| i32::try_from(index).ok())
                        .unwrap_or(-1);
                        self.as_mut().rust_mut().entries = entries;
                        self.as_mut().rust_mut().activity.dirty = true;
                        self.as_mut().rust_mut().guide_dirty = true;
                        // Physical channel metadata also affects subchannel visibility.
                        // Reset the small browser projection even if EPG revision is unchanged.
                        if self.rust().browser_projection.is_some() {
                            self.as_mut().rust_mut().browser_projection = Some(Default::default());
                        }
                        self.as_mut().rust_mut().next_current_program = Instant::now();
                        self.as_mut().set_channel_data(presentation);
                        self.as_mut().set_selected(selected);
                        self.as_mut().configure_epg();
                    }
                    if !self.rust().entries.is_empty() {
                        self.as_mut().rust_mut().catalog_selection =
                            channels::SelectionPolicy::Preserve;
                    }
                    let status = if self.rust().entries.is_empty() {
                        PlaybackStatus::Empty
                    } else if usize::try_from(self.rust().selected)
                        .ok()
                        .is_some_and(|index| index < self.rust().entries.len())
                    {
                        PlaybackStatus::Ready
                    } else {
                        PlaybackStatus::Select
                    };
                    if self.rust().active_service.is_none() {
                        self.as_mut().update_status(status);
                    }
                    self.as_mut().finish_connection(true);
                    if self.rust().autoplay_pending && self.rust().selected >= 0 {
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
        Ok(())
    }
}
