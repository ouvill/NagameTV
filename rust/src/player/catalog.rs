use super::{catalog_request::CatalogRequest, ffi};
use crate::network::NetworkRuntime;
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QStringList};
use std::{
    collections::HashMap,
    pin::Pin,
    sync::atomic::Ordering,
    time::{SystemTime, UNIX_EPOCH},
};

impl ffi::Player {
    pub fn refresh_channels(mut self: Pin<&mut Self>) {
        self.as_mut().ensure_epg_event_stream();
        if self.as_ref().rust().catalog_request.is_loading() {
            return;
        }
        let server = self.as_ref().server().to_string();
        let network = self
            .as_ref()
            .rust()
            .network
            .as_ref()
            .map(|network| (network.handle(), network.client()));
        let Some((runtime, client)) = network else {
            self.as_mut()
                .set_status(QString::from("Network runtime is unavailable"));
            return;
        };
        self.as_mut()
            .set_status(QString::from("Loading channels..."));
        self.as_mut().rust_mut().catalog_request = CatalogRequest::start(&runtime, client, server);
        self.as_ref().record_diagnostics("epg_fetch_started");
    }

    pub(super) fn ensure_epg_event_stream(mut self: Pin<&mut Self>) {
        let server = self
            .as_ref()
            .server()
            .to_string()
            .trim()
            .trim_end_matches('/')
            .to_owned();
        if server.is_empty() {
            self.as_mut().rust_mut().epg_task = None;
            self.as_mut().rust_mut().epg_event_server.clear();
            self.as_ref()
                .rust()
                .epg_event_generation
                .fetch_add(1, Ordering::AcqRel);
            return;
        }
        if self.as_ref().rust().epg_event_server == server {
            return;
        }
        let Some((runtime, client)) = self
            .as_ref()
            .rust()
            .network
            .as_ref()
            .map(|network| (network.handle(), network.stream_client()))
        else {
            return;
        };
        let generation = self
            .as_ref()
            .rust()
            .epg_event_generation
            .fetch_add(1, Ordering::AcqRel)
            + 1;
        let events = self.as_ref().rust().epg_events_tx.clone();
        self.as_mut().rust_mut().epg_task = None;
        while self.as_ref().rust().epg_events_rx.try_recv().is_ok() {}
        self.as_mut().rust_mut().epg_event_server = server.clone();
        self.as_mut().rust_mut().epg_task = Some(crate::network::NetworkTask(runtime.spawn(
            crate::epg_events::receive(client, server, generation, events),
        )));
    }

    /// Re-evaluate the current programme from the in-memory EPG snapshot.
    /// This is intentionally network-free so it can run at programme boundaries.

    pub fn refresh_current_programs(mut self: Pin<&mut Self>) {
        let Some(epg) = self
            .as_ref()
            .rust()
            .network
            .as_ref()
            .map(NetworkRuntime::epg)
        else {
            return;
        };
        let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH) else {
            return;
        };
        let snapshot = epg.snapshot();
        if snapshot.services.is_empty() {
            return;
        }
        let current_programs = snapshot
            .current_programs(now.as_millis() as u64)
            .into_iter()
            .map(|program| ((program.network_id, program.service_id), program))
            .collect::<HashMap<_, _>>();
        let services = snapshot
            .services
            .iter()
            .map(|service| (service.id, service))
            .collect::<HashMap<_, _>>();
        let service_ids = self.as_ref().rust().service_ids.clone();
        let program_for = |id: &u64| {
            services
                .get(id)
                .and_then(|service| current_programs.get(&(service.network_id, service.service_id)))
        };
        let titles = service_ids
            .iter()
            .map(|id| {
                QString::from(
                    program_for(id)
                        .and_then(|program| program.name.as_deref())
                        .unwrap_or(""),
                )
            })
            .collect::<QStringList>();
        let descriptions = service_ids
            .iter()
            .map(|id| {
                QString::from(
                    program_for(id)
                        .and_then(|program| program.description.as_deref())
                        .unwrap_or(""),
                )
            })
            .collect::<QStringList>();
        let starts = service_ids
            .iter()
            .map(|id| {
                QString::from(
                    program_for(id)
                        .map_or(0, |program| program.start_at)
                        .to_string(),
                )
            })
            .collect::<QStringList>();
        let durations = service_ids
            .iter()
            .map(|id| {
                QString::from(
                    program_for(id)
                        .map_or(0, |program| program.duration)
                        .to_string(),
                )
            })
            .collect::<QStringList>();
        self.as_mut().set_program_titles(titles);
        self.as_mut().set_program_descriptions(descriptions);
        self.as_mut().set_program_starts(starts);
        self.as_mut().set_program_durations(durations);

        let current_id = self.as_ref().service_id().to_string().parse::<u64>().ok();
        if let Some(playback) = self.as_ref().rust().playback.as_ref() {
            playback.set_audio_program(current_id.as_ref().and_then(program_for).map(|program| {
                crate::audio::AudioProgram {
                    service_id: program.service_id,
                    start_at: program.start_at,
                    audios: program.audios.clone(),
                }
            }));
        }
        if let Some(program) = current_id.as_ref().and_then(program_for) {
            self.as_mut()
                .set_program_name(QString::from(program.name.as_deref().unwrap_or("")));
            self.as_mut().set_program_description(QString::from(
                program.description.as_deref().unwrap_or(""),
            ));
            self.as_mut().rust_mut().current_program_start = program.start_at;
            self.as_mut().rust_mut().current_program_duration = program.duration;
        }
    }

    pub(super) fn poll_epg_events(mut self: Pin<&mut Self>) {
        // This method is called by QML's GUI-thread timer. Workers only send
        // immutable Rust data; QObject properties are changed here.
        let completed = self.as_mut().rust_mut().catalog_request.poll();
        if let Some((server, result)) = completed {
            if self.as_ref().server().to_string() == server {
                self.as_mut().apply_catalog(&server, result);
            } else {
                // A direct write to the public server property can bypass
                // connect_server(). poll() has returned the old request to Idle,
                // so start the current server's request without waiting for an event.
                self.as_mut().refresh_channels();
            }
        }
        if !self.as_ref().rust().catalog_request.is_loading()
            && let Ok(generation) = self.as_ref().rust().epg_events_rx.try_recv()
        {
            if generation
                == self
                    .as_ref()
                    .rust()
                    .epg_event_generation
                    .load(Ordering::Acquire)
            {
                self.as_mut().refresh_channels();
            }
        }
    }
}
