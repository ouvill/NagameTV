use super::{
    fetch::{FetchServicesError, FetchedCatalog},
    ffi,
    selection::service_logo_url,
};
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QStringList};
use std::pin::Pin;

impl ffi::Player {
    pub(super) fn apply_catalog(
        mut self: Pin<&mut Self>,
        server: &str,
        result: Result<FetchedCatalog, FetchServicesError>,
    ) {
        let event = if result.is_ok() {
            "epg_fetch_finished"
        } else {
            "epg_fetch_failed"
        };
        match result {
            Ok(FetchedCatalog {
                catalog: payload,
                snapshot,
            }) => {
                if let Some(network) = self.as_ref().rust().network.as_ref() {
                    network.epg().publish(snapshot);
                }
                let services = payload.channels;
                let labels = services
                    .iter()
                    .map(|service| QString::from(&service.label))
                    .collect::<QStringList>();
                let program_titles = services
                    .iter()
                    .map(|service| {
                        QString::from(
                            service
                                .program
                                .as_ref()
                                .map_or("", |program| program.title.as_str()),
                        )
                    })
                    .collect::<QStringList>();
                let program_descriptions = services
                    .iter()
                    .map(|service| {
                        QString::from(
                            service
                                .program
                                .as_ref()
                                .map_or("", |program| program.description.as_str()),
                        )
                    })
                    .collect::<QStringList>();
                let program_starts = services
                    .iter()
                    .map(|service| {
                        QString::from(
                            service
                                .program
                                .as_ref()
                                .map_or(0, |program| program.start_at)
                                .to_string(),
                        )
                    })
                    .collect::<QStringList>();
                let program_durations = services
                    .iter()
                    .map(|service| {
                        QString::from(
                            service
                                .program
                                .as_ref()
                                .map_or(0, |program| program.duration)
                                .to_string(),
                        )
                    })
                    .collect::<QStringList>();
                let channel_logo_urls = services
                    .iter()
                    .map(|service| {
                        QString::from(if service.has_logo_data {
                            service_logo_url(server, service.id)
                        } else {
                            String::new()
                        })
                    })
                    .collect::<QStringList>();
                let channel_types = services
                    .iter()
                    .map(|service| QString::from(&service.channel_type))
                    .collect::<QStringList>();
                let jikkyo_forces = services
                    .iter()
                    .map(|service| {
                        QString::from(
                            service
                                .jikkyo_force
                                .map(|force| force.to_string())
                                .unwrap_or_default(),
                        )
                    })
                    .collect::<QStringList>();
                self.as_mut().rust_mut().service_ids =
                    services.iter().map(|service| service.id).collect();
                self.as_mut().rust_mut().jikkyo_ids = services
                    .iter()
                    .map(|service| service.jikkyo_id.clone())
                    .collect();
                self.as_mut().set_services(labels);
                self.as_mut().set_program_titles(program_titles);
                self.as_mut().set_program_descriptions(program_descriptions);
                self.as_mut().set_program_starts(program_starts);
                self.as_mut().set_program_durations(program_durations);
                self.as_mut().set_channel_logo_urls(channel_logo_urls);
                self.as_mut().set_channel_types(channel_types);
                self.as_mut().set_jikkyo_forces(jikkyo_forces);
                self.as_mut()
                    .set_guide_start(QString::from(payload.guide_start.to_string()));
                self.as_mut()
                    .set_guide_program_ids(strings(payload.guide.iter().map(|p| p.id.to_string())));
                self.as_mut().set_guide_channel_indices(strings(
                    payload.guide.iter().map(|p| p.channel_index.to_string()),
                ));
                self.as_mut().set_guide_titles(
                    payload
                        .guide
                        .iter()
                        .map(|p| QString::from(&p.title))
                        .collect(),
                );
                self.as_mut().set_guide_descriptions(
                    payload
                        .guide
                        .iter()
                        .map(|p| QString::from(&p.description))
                        .collect(),
                );
                self.as_mut().set_guide_starts(strings(
                    payload.guide.iter().map(|p| p.start_at.to_string()),
                ));
                self.as_mut().set_guide_durations(strings(
                    payload.guide.iter().map(|p| p.duration.to_string()),
                ));
                self.as_mut()
                    .set_guide_genres(strings(payload.guide.iter().map(|p| p.genre.to_string())));
                if let Ok(current_id) = self.as_ref().service_id().to_string().parse::<u64>()
                    && let Some(index) = self
                        .as_ref()
                        .rust()
                        .service_ids
                        .iter()
                        .position(|id| *id == current_id)
                {
                    let channel = &services[index];
                    let label = QString::from(&channel.label);
                    let title = QString::from(
                        channel
                            .program
                            .as_ref()
                            .map_or("", |program| program.title.as_str()),
                    );
                    let description = QString::from(
                        channel
                            .program
                            .as_ref()
                            .map_or("", |program| program.description.as_str()),
                    );
                    let logo = QString::from(if channel.has_logo_data {
                        service_logo_url(server, channel.id)
                    } else {
                        String::new()
                    });
                    let start = channel
                        .program
                        .as_ref()
                        .map_or(0, |program| program.start_at);
                    let duration = channel
                        .program
                        .as_ref()
                        .map_or(0, |program| program.duration);
                    self.as_mut().set_channel_name(label);
                    self.as_mut().set_program_name(title);
                    self.as_mut().set_program_description(description);
                    self.as_mut().set_channel_logo_url(logo);
                    self.as_mut().rust_mut().current_program_start = start;
                    self.as_mut().rust_mut().current_program_duration = duration;
                }
                if !*self.as_ref().playing() {
                    self.as_mut()
                        .set_status(QString::from(if services.is_empty() {
                            "No available channels were found"
                        } else {
                            "Ready"
                        }));
                }
                self.as_mut().restart_comments();
            }
            Err(error) => self.as_mut().set_status(QString::from(error.to_string())),
        }
        self.as_ref().record_diagnostics(event);
    }
}

fn strings(values: impl Iterator<Item = String>) -> QStringList {
    values.map(|value| QString::from(value)).collect()
}
