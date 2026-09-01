#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;

        include!("qt_helpers.h");
        type QQuickItem;
        #[cxx_name = "configureQtQuickOpenGl"]
        fn configure_qt_quick_open_gl();
        #[cxx_name = "qQuickItemAddress"]
        unsafe fn q_quick_item_address(item: *mut QQuickItem) -> usize;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QString, server)]
        #[qproperty(QString, service_id, cxx_name = "serviceId")]
        #[qproperty(QString, status)]
        #[qproperty(bool, playing)]
        #[qproperty(f64, volume)]
        #[qproperty(bool, autoplay)]
        #[qproperty(QString, channel_name, cxx_name = "channelName")]
        #[qproperty(QString, program_name, cxx_name = "programName")]
        #[qproperty(QString, program_description, cxx_name = "programDescription")]
        #[qproperty(f64, program_progress, cxx_name = "programProgress")]
        #[qproperty(QStringList, services)]
        #[qproperty(QStringList, program_titles, cxx_name = "programTitles")]
        #[qproperty(QStringList, program_starts, cxx_name = "programStarts")]
        #[qproperty(QStringList, program_durations, cxx_name = "programDurations")]
        type Player = super::PlayerRust;

        #[qinvokable]
        #[cxx_name = "attachVideoItem"]
        unsafe fn attach_video_item(self: Pin<&mut Player>, item: *mut QQuickItem) -> bool;
        #[qinvokable]
        fn play(self: Pin<&mut Player>);
        #[qinvokable]
        fn stop(self: Pin<&mut Player>);
        #[qinvokable]
        #[cxx_name = "togglePause"]
        fn toggle_pause(self: Pin<&mut Player>);
        #[qinvokable]
        #[cxx_name = "pollEvents"]
        fn poll_events(self: Pin<&mut Player>);
        #[qinvokable]
        #[cxx_name = "refreshChannels"]
        fn refresh_channels(self: Pin<&mut Player>);
        #[qinvokable]
        #[cxx_name = "selectChannel"]
        fn select_channel(self: Pin<&mut Player>, index: i32);
        #[qinvokable]
        #[cxx_name = "changeChannel"]
        fn change_channel(self: Pin<&mut Player>, offset: i32);
    }

    impl cxx_qt::Threading for Player {}
}

use crate::epg::{CurrentProgram, EpgStore, Program, Service};
use crate::network::NetworkRuntime;
use crate::playback::{Playback, PlaybackEvent};
use core::pin::Pin;
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QString, QStringList};
use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct PlayerRust {
    server: QString,
    service_id: QString,
    status: QString,
    playing: bool,
    volume: f64,
    autoplay: bool,
    channel_name: QString,
    program_name: QString,
    program_description: QString,
    program_progress: f64,
    services: QStringList,
    program_titles: QStringList,
    program_starts: QStringList,
    program_durations: QStringList,
    paused: bool,
    applied_volume: f64,
    service_ids: Vec<u64>,
    loading_channels: AtomicBool,
    network: Option<NetworkRuntime>,
    playback: Option<Playback>,
}

impl Default for PlayerRust {
    fn default() -> Self {
        let playback = crate::playback::take_preloaded();
        let network = NetworkRuntime::new();
        let status = playback
            .as_ref()
            .map(|_| ())
            .and(network.as_ref().map(|_| ()))
            .map_or_else(|error| error.clone(), |()| "Ready".to_owned());
        Self {
            server: QString::from(
                std::env::var("MIRAKURUN_SERVER")
                    .unwrap_or_else(|_| "http://127.0.0.1:40772".to_owned()),
            ),
            service_id: QString::from(std::env::var("MIRAKURUN_SERVICE_ID").unwrap_or_default()),
            status: QString::from(status),
            playing: false,
            volume: 70.0,
            autoplay: std::env::var("MIRAKURUN_AUTOPLAY").is_ok_and(|value| value != "0"),
            channel_name: QString::default(),
            program_name: QString::default(),
            program_description: QString::default(),
            program_progress: 0.0,
            services: QStringList::default(),
            program_titles: QStringList::default(),
            program_starts: QStringList::default(),
            program_durations: QStringList::default(),
            paused: false,
            applied_volume: 70.0,
            service_ids: Vec::new(),
            loading_channels: AtomicBool::new(false),
            network: network.ok(),
            playback: playback.ok(),
        }
    }
}

impl ffi::Player {
    pub unsafe fn attach_video_item(mut self: Pin<&mut Self>, item: *mut ffi::QQuickItem) -> bool {
        let address = unsafe { ffi::q_quick_item_address(item) };
        let result = {
            let mut rust = self.as_mut().rust_mut();
            let Some(playback) = rust.playback.as_mut() else {
                return false;
            };
            playback.attach_video_item(address as *mut c_void)
        };
        match result {
            Ok(()) => true,
            Err(error) => {
                self.as_mut().set_status(QString::from(error));
                false
            }
        }
    }

    pub fn play(mut self: Pin<&mut Self>) {
        let server = self.as_ref().server().to_string();
        let service_id = self.as_ref().service_id().to_string().parse::<u64>();
        let result = match (self.as_ref().rust().playback.as_ref(), service_id) {
            (Some(playback), Ok(id)) if id > 0 => playback.play_service(&server, id),
            (None, _) => Err("Could not initialize the player".to_owned()),
            _ => Err("Enter a valid Mirakurun service ID".to_owned()),
        };
        match result {
            Ok(()) => {
                self.as_mut().rust_mut().paused = false;
                self.as_mut().set_playing(true);
                self.as_mut().set_status(QString::from("Connecting..."));
            }
            Err(error) => self.as_mut().set_status(QString::from(error)),
        }
    }

    pub fn stop(mut self: Pin<&mut Self>) {
        if let Some(playback) = self.as_ref().rust().playback.as_ref() {
            playback.stop();
        }
        self.as_mut().rust_mut().paused = false;
        self.as_mut().set_playing(false);
        self.as_mut().set_status(QString::from("Stopped"));
    }

    pub fn toggle_pause(mut self: Pin<&mut Self>) {
        if !*self.as_ref().playing() {
            return;
        }
        let paused = !self.as_ref().rust().paused;
        self.as_mut().rust_mut().paused = paused;
        if let Some(playback) = self.as_ref().rust().playback.as_ref() {
            playback.set_paused(paused);
        }
        self.as_mut()
            .set_status(QString::from(if paused { "Paused" } else { "Playing" }));
    }

    pub fn poll_events(mut self: Pin<&mut Self>) {
        let event = self
            .as_ref()
            .rust()
            .playback
            .as_ref()
            .map_or(PlaybackEvent::Error, Playback::drain_events);
        match event {
            PlaybackEvent::None => {}
            PlaybackEvent::Playing => self.as_mut().set_status(QString::from("Playing")),
            PlaybackEvent::Ended => {
                self.as_mut().set_playing(false);
                self.as_mut().set_status(QString::from("Stream ended"));
            }
            PlaybackEvent::Error => {
                self.as_mut().set_playing(false);
                self.as_mut().set_status(QString::from("Playback error"));
            }
        }
        let volume = *self.as_ref().volume();
        if (volume - self.as_ref().rust().applied_volume).abs() > f64::EPSILON {
            if let Some(playback) = self.as_ref().rust().playback.as_ref() {
                playback.set_volume(volume);
            }
            self.as_mut().rust_mut().applied_volume = volume;
        }
    }

    pub fn refresh_channels(mut self: Pin<&mut Self>) {
        if self
            .as_ref()
            .rust()
            .loading_channels
            .swap(true, Ordering::AcqRel)
        {
            return;
        }
        let server = self.as_ref().server().to_string();
        let qt_thread = self.qt_thread();
        let network = self
            .as_ref()
            .rust()
            .network
            .as_ref()
            .map(|network| (network.handle(), network.client(), network.epg()));
        let Some((runtime, client, epg)) = network else {
            self.as_ref()
                .rust()
                .loading_channels
                .store(false, Ordering::Release);
            self.as_mut()
                .set_status(QString::from("Network runtime is unavailable"));
            return;
        };
        self.as_mut()
            .set_status(QString::from("Loading channels..."));
        runtime.spawn(async move {
            let result = fetch_services(&client, &epg, &server).await;
            let _ = qt_thread.queue(move |mut player| {
                player
                    .as_ref()
                    .rust()
                    .loading_channels
                    .store(false, Ordering::Release);
                match result {
                    Ok(services) => {
                        let labels = services
                            .iter()
                            .map(|service| QString::from(&service.label))
                            .collect::<QStringList>();
                        let program_titles = services
                            .iter()
                            .map(|service| QString::from(&service.program_title))
                            .collect::<QStringList>();
                        let program_starts = services
                            .iter()
                            .map(|service| QString::from(service.program_start.to_string()))
                            .collect::<QStringList>();
                        let program_durations = services
                            .iter()
                            .map(|service| QString::from(service.program_duration.to_string()))
                            .collect::<QStringList>();
                        player.as_mut().rust_mut().service_ids =
                            services.iter().map(|service| service.id).collect();
                        player.as_mut().set_services(labels);
                        player.as_mut().set_program_titles(program_titles);
                        player.as_mut().set_program_starts(program_starts);
                        player.as_mut().set_program_durations(program_durations);
                        if let Ok(current_id) =
                            player.as_ref().service_id().to_string().parse::<u64>()
                            && let Some(index) = player
                                .as_ref()
                                .rust()
                                .service_ids
                                .iter()
                                .position(|id| *id == current_id)
                        {
                            let label = player.as_ref().services().get(index as isize).cloned();
                            if let Some(label) = label {
                                player.as_mut().set_channel_name(label);
                            }
                        }
                        if !*player.as_ref().playing() {
                            player.as_mut().set_status(QString::from("Ready"));
                        }
                    }
                    Err(error) => player.as_mut().set_status(QString::from(error)),
                }
            });
        });
    }

    pub fn select_channel(mut self: Pin<&mut Self>, index: i32) {
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        let Some(&service_id) = self.as_ref().rust().service_ids.get(index) else {
            return;
        };
        let channel_name = self
            .as_ref()
            .services()
            .get(index as isize)
            .map(|value| value.to_string())
            .unwrap_or_default();
        self.as_mut()
            .set_service_id(QString::from(service_id.to_string()));
        self.as_mut().set_channel_name(QString::from(channel_name));
        self.play();
    }

    pub fn change_channel(self: Pin<&mut Self>, offset: i32) {
        let count = self.as_ref().rust().service_ids.len();
        if count == 0 {
            return;
        }
        let current_id = self.as_ref().service_id().to_string().parse::<u64>().ok();
        let current = current_id
            .and_then(|id| {
                self.as_ref()
                    .rust()
                    .service_ids
                    .iter()
                    .position(|item| *item == id)
            })
            .unwrap_or(0);
        let next = (current as i64 + i64::from(offset)).rem_euclid(count as i64) as i32;
        self.select_channel(next);
    }
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct ProgramSignature {
    event_id: u16,
    start_at: u64,
    duration: u64,
}

struct Channel {
    id: u64,
    label: String,
    remote_key: u16,
    channel_priority: u8,
    service_id: u16,
    physical_channel: String,
    program_signature: Option<ProgramSignature>,
    program_title: String,
    program_start: u64,
    program_duration: u64,
}

async fn fetch_services(
    client: &reqwest::Client,
    epg: &EpgStore,
    server: &str,
) -> Result<Vec<Channel>, String> {
    let api = server.trim().trim_end_matches('/');
    let services = client
        .get(format!("{api}/api/services"))
        .send()
        .await
        .map_err(|error| format!("Could not load channels: {error}"))?
        .error_for_status()
        .map_err(|error| format!("Could not load channels: {error}"))?
        .json::<Vec<Service>>()
        .await
        .map_err(|error| format!("Invalid Mirakurun service list: {error}"))?;
    let programs = client
        .get(format!("{api}/api/programs"))
        .send()
        .await
        .map_err(|error| format!("Could not load programs: {error}"))?
        .error_for_status()
        .map_err(|error| format!("Could not load programs: {error}"))?
        .json::<Vec<Program>>()
        .await
        .map_err(|error| format!("Invalid Mirakurun program list: {error}"))?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("Could not read system time: {error}"))?
        .as_millis() as u64;
    epg.replace(services, programs, now);
    let snapshot = epg.snapshot();
    let current_programs = snapshot.current_programs(now);
    Ok(build_channels(&snapshot.services, current_programs))
}

fn build_channels(services: &[Service], programs: Vec<CurrentProgram>) -> Vec<Channel> {
    let current_programs = programs
        .iter()
        .map(|program| ((program.network_id, program.service_id), program))
        .collect::<HashMap<_, _>>();
    let mut channels = services
        .iter()
        .filter(|service| service.service_type == 1)
        .map(|service| {
            let remote_key = service.remote_control_key_id.unwrap_or(0);
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
                label: if remote_key == 0 {
                    format!("--   {}", service.name)
                } else {
                    format!("{remote_key:02}   {}", service.name)
                },
                remote_key,
                channel_priority,
                service_id: service.service_id,
                physical_channel: format!(
                    "{}:{}:{}",
                    service.network_id, service.channel.channel_type, service.channel.channel
                ),
                program_signature: program.map(|program| ProgramSignature {
                    event_id: program.event_id,
                    start_at: program.start_at,
                    duration: program.duration,
                }),
                program_title: program
                    .and_then(|program| program.name.clone())
                    .unwrap_or_default(),
                program_start: program.map_or(0, |program| program.start_at),
                program_duration: program.map_or(0, |program| program.duration),
            }
        })
        .collect::<Vec<_>>();
    channels.sort_by(|a, b| {
        (
            a.channel_priority,
            a.remote_key == 0,
            a.remote_key,
            &a.label,
            a.service_id,
        )
            .cmp(&(
                b.channel_priority,
                b.remote_key == 0,
                b.remote_key,
                &b.label,
                b.service_id,
            ))
    });
    let mut main_broadcasts = HashMap::<String, Option<ProgramSignature>>::new();
    let mut broadcasts = HashSet::new();
    channels.retain(|channel| {
        let Some(main_program) = main_broadcasts.get(&channel.physical_channel) else {
            main_broadcasts.insert(
                channel.physical_channel.clone(),
                channel.program_signature.clone(),
            );
            broadcasts.insert((
                channel.physical_channel.clone(),
                channel.program_signature.clone(),
            ));
            return true;
        };
        matches!(
            (main_program, &channel.program_signature),
            (Some(main), Some(subchannel)) if main != subchannel
        ) && broadcasts.insert((
            channel.physical_channel.clone(),
            channel.program_signature.clone(),
        ))
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
            remote_control_key_id: Some(1),
            channel: ServiceChannel {
                channel_type: "GR".to_owned(),
                channel: "26".to_owned(),
            },
        }
    }

    fn program(service_id: u16, event_id: u16) -> CurrentProgram {
        CurrentProgram {
            event_id,
            service_id,
            network_id: 32_000,
            start_at: 1_000,
            duration: 1_800,
            name: Some(format!("Program {event_id}")),
        }
    }

    #[test]
    fn hides_subchannel_during_simulcast() {
        let services = [service(100, "Main"), service(101, "Sub")];
        let channels = build_channels(&services, vec![program(100, 10), program(101, 10)]);
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].service_id, 100);
    }

    #[test]
    fn keeps_subchannel_during_split_programming() {
        let services = [service(100, "Main"), service(101, "Sub")];
        let channels = build_channels(&services, vec![program(100, 10), program(101, 11)]);
        assert_eq!(channels.len(), 2);
    }

    #[test]
    fn hides_subchannel_when_program_information_is_missing() {
        let services = [service(100, "Main"), service(101, "Sub")];
        let channels = build_channels(&services, vec![program(100, 10)]);
        assert_eq!(channels.len(), 1);
    }
}
