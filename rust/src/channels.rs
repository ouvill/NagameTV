//! Channel selection and guide projection. No Qt, network IO or wall-clock reads.
use crate::comments::jikkyo_id;
use crate::epg::{CurrentProgram, EpgSnapshot, Service};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct ProgramSignature {
    event_id: u16,
    start_at: u64,
    duration: u64,
}

pub(crate) struct Channel {
    pub id: u64,
    pub has_logo_data: bool,
    pub label: String,
    channel_number: u16,
    channel_priority: u8,
    service_id: u16,
    physical_channel: PhysicalChannel,
    pub program: Option<ProgramSummary>,
    network_id: u16,
    pub channel_type: String,
    pub jikkyo_id: Option<String>,
    pub jikkyo_force: Option<u64>,
}

/// The fields exist together only when EPG identifies a current programme.
pub(crate) struct ProgramSummary {
    event_id: u16,
    pub title: String,
    pub description: String,
    pub start_at: u64,
    pub duration: u64,
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct PhysicalChannel {
    network_id: u16,
    channel_type: String,
    channel: String,
}

pub(crate) struct GuideProgram {
    pub id: u64,
    pub channel_index: usize,
    pub title: String,
    pub description: String,
    pub start_at: u64,
    pub duration: u64,
    pub genre: u8,
}

pub(crate) struct ChannelCatalog {
    pub channels: Vec<Channel>,
    pub guide: Vec<GuideProgram>,
    pub guide_start: u64,
}

pub(crate) fn build_catalog(snapshot: &EpgSnapshot, now: u64) -> ChannelCatalog {
    let channels = build_channels(&snapshot.services, &snapshot.current_programs(now));
    // QML presents seven local calendar days. Keep a one-day margin before now
    // so today's programmes are available regardless of the local UTC offset,
    // plus enough future data to cover the final tab completely.
    let guide_start = now.saturating_sub(24 * 60 * 60 * 1_000);
    let guide_end = now.saturating_add(8 * 24 * 60 * 60 * 1_000);
    let mut guide = snapshot
        .programs_between(guide_start, guide_end)
        .into_iter()
        .filter_map(|program| {
            let channel_index = channels.iter().position(|channel| {
                channel.network_id == program.network_id && channel.service_id == program.service_id
            })?;
            Some(GuideProgram {
                id: program.id,
                channel_index,
                title: program.name.clone().unwrap_or_default(),
                description: program.description.clone().unwrap_or_default(),
                start_at: program.start_at,
                duration: program.duration,
                genre: program.genres.first().map_or(15, |genre| genre.lv1),
            })
        })
        .collect::<Vec<_>>();
    guide.sort_unstable_by_key(|program| (program.channel_index, program.start_at));
    ChannelCatalog {
        channels,
        guide,
        guide_start,
    }
}

fn build_channels(services: &[Service], programs: &[CurrentProgram]) -> Vec<Channel> {
    let current_programs = programs
        .iter()
        .map(|program| ((program.network_id, program.service_id), program))
        .collect::<HashMap<_, _>>();
    let mut channels = services
        .iter()
        .filter(|service| service.service_type == 1)
        .map(|service| {
            let remote_key = service.remote_control_key_id.unwrap_or(0);
            let is_terrestrial = service.channel.channel_type == "GR";
            let channel_number = if is_terrestrial {
                remote_key
            } else {
                service.service_id
            };
            let channel_priority = match service.channel.channel_type.as_str() {
                "GR" => 0,
                "BS" => 1,
                "CS" => 2,
                "SKY" => 3,
                _ => 4,
            };
            let program = current_programs.get(&(service.network_id, service.service_id));
            Channel {
                id: service.id,
                has_logo_data: service.has_logo_data,
                label: if is_terrestrial && remote_key == 0 {
                    format!("--   {}", service.name)
                } else if is_terrestrial {
                    format!("{remote_key:02}   {}", service.name)
                } else {
                    format!("{:03}   {}", service.service_id, service.name)
                },
                channel_number,
                channel_priority,
                service_id: service.service_id,
                physical_channel: PhysicalChannel {
                    network_id: service.network_id,
                    channel_type: service.channel.channel_type.clone(),
                    channel: service.channel.channel.clone(),
                },
                program: program.map(|program| ProgramSummary {
                    event_id: program.event_id,
                    title: program.name.clone().unwrap_or_default(),
                    description: program.description.clone().unwrap_or_default(),
                    start_at: program.start_at,
                    duration: program.duration,
                }),
                network_id: service.network_id,
                channel_type: service.channel.channel_type.clone(),
                jikkyo_id: jikkyo_id(
                    &service.channel.channel_type,
                    service.service_id,
                    &service.name,
                ),
                jikkyo_force: None,
            }
        })
        .collect::<Vec<_>>();
    channels.sort_by(|a, b| {
        (
            a.channel_priority,
            a.channel_number == 0,
            a.channel_number,
            &a.label,
            a.service_id,
        )
            .cmp(&(
                b.channel_priority,
                b.channel_number == 0,
                b.channel_number,
                &b.label,
                b.service_id,
            ))
    });
    let mut main_broadcasts = HashMap::<PhysicalChannel, Option<ProgramSignature>>::new();
    let mut broadcasts = HashSet::new();
    channels.retain(|channel| {
        let signature = channel.program.as_ref().map(|program| ProgramSignature {
            event_id: program.event_id,
            start_at: program.start_at,
            duration: program.duration,
        });
        let Some(main_program) = main_broadcasts.get(&channel.physical_channel) else {
            main_broadcasts.insert(channel.physical_channel.clone(), signature);
            broadcasts.insert((channel.physical_channel.clone(), signature));
            return true;
        };
        matches!(
            (main_program, &signature),
            (Some(main), Some(subchannel)) if main != subchannel
        ) && broadcasts.insert((channel.physical_channel.clone(), signature))
    });
    channels
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::epg::ServiceChannel;

    fn service(service_id: u16, name: &str) -> Service {
        Service {
            id: 32_000_000_u64 + u64::from(service_id),
            service_id,
            network_id: 32_000,
            name: name.to_owned(),
            service_type: 1,
            has_logo_data: true,
            remote_control_key_id: Some(1),
            channel: ServiceChannel {
                channel_type: "GR".to_owned(),
                channel: "26".to_owned(),
            },
        }
    }

    fn program(service_id: u16, event_id: u16) -> CurrentProgram {
        CurrentProgram {
            audios: Vec::new(),
            event_id,
            service_id,
            network_id: 32_000,
            start_at: 1_000,
            duration: 1_800,
            name: Some(format!("Program {event_id}")),
            description: Some(format!("Description {event_id}")),
        }
    }

    #[test]
    fn hides_subchannel_during_simulcast() {
        let services = [service(100, "Main"), service(101, "Sub")];
        let channels = build_channels(&services, &[program(100, 10), program(101, 10)]);
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].service_id, 100);
    }

    #[test]
    fn keeps_subchannel_during_split_programming() {
        let services = [service(100, "Main"), service(101, "Sub")];
        let channels = build_channels(&services, &[program(100, 10), program(101, 11)]);
        assert_eq!(channels.len(), 2);
    }

    #[test]
    fn hides_subchannel_when_program_information_is_missing() {
        let services = [service(100, "Main"), service(101, "Sub")];
        let channels = build_channels(&services, &[program(100, 10)]);
        assert_eq!(channels.len(), 1);
    }

    #[test]
    fn missing_program_is_distinct_from_a_program_with_no_title() {
        let services = [service(100, "Main")];
        let missing = build_channels(&services, &[]);
        assert!(missing[0].program.is_none());

        let mut current = program(100, 10);
        current.name = None;
        current.description = None;
        let channels = build_channels(&services, &[current]);
        let present = channels[0].program.as_ref().unwrap();
        assert!(present.title.is_empty());
        assert_eq!(present.start_at, 1_000);
        assert_eq!(present.duration, 1_800);
    }

    #[test]
    fn guide_follows_visible_channel_order_and_program_boundaries() {
        use crate::epg::{EpgStore, Program};

        let store = EpgStore::default();
        let mut second = service(200, "Second");
        second.remote_control_key_id = Some(2);
        second.channel.channel = "27".into();
        let scheduled = |service_id, event_id, start_at| Program {
            id: u64::from(event_id),
            event_id,
            service_id,
            network_id: 32_000,
            start_at,
            duration: 1_000,
            name: Some(format!("Program {event_id}")),
            description: None,
            audios: Vec::new(),
            genres: Vec::new(),
        };
        store.replace(
            vec![second, service(101, "Sub"), service(100, "Main")],
            vec![
                scheduled(200, 30, 1_000),
                scheduled(100, 11, 2_000),
                scheduled(101, 10, 1_000),
                scheduled(100, 10, 1_000),
                scheduled(999, 40, 1_000),
            ],
            1_000,
        );
        let snapshot = store.snapshot();
        let catalog = build_catalog(&snapshot, 1_500);
        assert_eq!(catalog.guide_start, 0);
        assert_eq!(
            catalog
                .channels
                .iter()
                .map(|c| c.service_id)
                .collect::<Vec<_>>(),
            [100, 200]
        );
        assert_eq!(
            catalog
                .guide
                .iter()
                .map(|p| (p.channel_index, p.id))
                .collect::<Vec<_>>(),
            [(0, 10), (0, 11), (1, 30)]
        );
        assert!(
            catalog
                .guide
                .iter()
                .all(|p| p.genre == 15 && p.description.is_empty())
        );
        assert_eq!(
            catalog.channels[0].program.as_ref().unwrap().title,
            "Program 10"
        );

        let next = build_catalog(&snapshot, 2_000);
        assert_eq!(
            next.channels[0].program.as_ref().unwrap().title,
            "Program 11"
        );
        assert!(next.channels[1].program.is_none());
        // Projection borrows the snapshot and leaves the previous result intact.
        assert_eq!(
            catalog.channels[0].program.as_ref().unwrap().title,
            "Program 10"
        );
        assert_eq!(snapshot.program_count, 5);
    }
}
