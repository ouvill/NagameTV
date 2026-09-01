#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

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
    }
}

use crate::playback::{Playback, PlaybackEvent};
use core::pin::Pin;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::ffi::c_void;

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
    paused: bool,
    applied_volume: f64,
    playback: Option<Playback>,
}

impl Default for PlayerRust {
    fn default() -> Self {
        let playback = crate::playback::take_preloaded();
        let status = match &playback {
            Ok(_) => "Ready".to_owned(),
            Err(error) => error.clone(),
        };
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
            paused: false,
            applied_volume: 70.0,
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
}
