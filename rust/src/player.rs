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

use crate::network::NetworkRuntime;
use crate::playback::{Playback, PlaybackEvent};
use core::pin::Pin;
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QString, QStringList};
use serde::Deserialize;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};

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
            .map(|network| (network.handle(), network.client()));
        let Some((runtime, client)) = network else {
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
            let result = fetch_services(&client, &server).await;
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
                        player.as_mut().rust_mut().service_ids =
                            services.iter().map(|service| service.id).collect();
                        player.as_mut().set_services(labels);
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MirakurunService {
    id: u64,
    name: String,
    #[serde(rename = "type")]
    service_type: u16,
    remote_control_key_id: Option<u16>,
    channel: MirakurunChannel,
}

#[derive(Deserialize)]
struct MirakurunChannel {
    #[serde(rename = "type")]
    channel_type: String,
}

struct Channel {
    id: u64,
    label: String,
    remote_key: u16,
    channel_priority: u8,
}

async fn fetch_services(client: &reqwest::Client, server: &str) -> Result<Vec<Channel>, String> {
    let url = format!("{}/api/services", server.trim().trim_end_matches('/'));
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("Could not load channels: {error}"))?;
    let mut channels = response
        .error_for_status()
        .map_err(|error| format!("Could not load channels: {error}"))?
        .json::<Vec<MirakurunService>>()
        .await
        .map_err(|error| format!("Invalid Mirakurun service list: {error}"))?
        .into_iter()
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
            Channel {
                id: service.id,
                label: if remote_key == 0 {
                    format!("--   {}", service.name)
                } else {
                    format!("{remote_key:02}   {}", service.name)
                },
                remote_key,
                channel_priority,
            }
        })
        .collect::<Vec<_>>();
    channels.sort_by(|a, b| {
        (
            a.channel_priority,
            a.remote_key == 0,
            a.remote_key,
            &a.label,
        )
            .cmp(&(
                b.channel_priority,
                b.remote_key == 0,
                b.remote_key,
                &b.label,
            ))
    });
    channels.dedup_by(|a, b| a.label == b.label);
    Ok(channels)
}
