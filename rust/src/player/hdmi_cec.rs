//! QObject projection and input delivery; CEC resources and decisions live in Rust.
use super::ffi;
use crate::{
    hdmi_cec::{Input, InputKey, OSD_NAME},
    qt, settings,
};
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::pin::Pin;

const INPUTS_PER_TICK: usize = 16;

impl ffi::Player {
    pub fn hdmi_cec_enabled(&self) -> bool {
        self.rust().hdmi_cec.enabled()
    }
    pub fn hdmi_cec_allowed(&self) -> bool {
        self.rust().hdmi_cec.allowed()
    }
    pub fn hdmi_cec_state(&self) -> QString {
        QString::from(self.rust().hdmi_cec.snapshot().state.as_str())
    }
    pub fn hdmi_cec_error(&self) -> QString {
        QString::from(self.rust().hdmi_cec.snapshot().state.message())
    }
    pub fn hdmi_cec_devices(&self) -> *mut crate::hdmi_cec_model::ffi::HdmiCecDeviceModel {
        self.rust().hdmi_cec_model.as_ptr().cast_mut()
    }
    pub fn hdmi_cec_device(&self) -> QString {
        QString::from(self.rust().hdmi_cec.selection())
    }
    pub fn hdmi_cec_osd_name(&self) -> QString {
        QString::from(OSD_NAME)
    }
    pub fn hdmi_cec_physical_address(&self) -> QString {
        self.rust()
            .hdmi_cec
            .snapshot()
            .physical_address
            .map(|a| {
                QString::from(format!(
                    "{:x}.{:x}.{:x}.{:x}",
                    (a >> 12) & 0xf,
                    (a >> 8) & 0xf,
                    (a >> 4) & 0xf,
                    a & 0xf
                ))
            })
            .unwrap_or_default()
    }
    pub fn set_hdmi_cec_enabled(mut self: Pin<&mut Self>, enabled: bool) -> bool {
        if !self.as_mut().rust_mut().hdmi_cec.enable(enabled) {
            return false;
        }
        self.as_mut()
            .rust_mut()
            .preferences
            .change(settings::Change::HdmiCecEnabled(enabled));
        self.as_mut().notify_hdmi_cec();
        self.save_settings();
        true
    }
    pub fn select_hdmi_cec_device(mut self: Pin<&mut Self>, path: QString) -> bool {
        let path = path.to_string();
        if !self.as_mut().rust_mut().hdmi_cec.select(path.clone()) {
            tracing::warn!("HDMI-CEC adapter selection rejected: expected /dev/cecN or automatic");
            return false;
        }
        self.as_mut()
            .rust_mut()
            .preferences
            .change(settings::Change::HdmiCecDevice(path));
        self.as_mut().notify_hdmi_cec();
        self.save_settings();
        true
    }
    pub fn refresh_hdmi_cec_devices(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().hdmi_cec.refresh();
        self.notify_hdmi_cec();
    }
    pub(super) fn notify_hdmi_cec(mut self: Pin<&mut Self>) {
        let rows = self.rust().hdmi_cec.snapshot().devices.clone();
        self.as_mut()
            .rust_mut()
            .hdmi_cec_model
            .pin_mut()
            .replace(rows.into());
        self.hdmi_cec_changed();
    }
    pub(super) fn poll_hdmi_cec(mut self: Pin<&mut Self>) {
        if self.as_mut().rust_mut().hdmi_cec.poll() {
            self.as_mut().notify_hdmi_cec();
        }
        for _ in 0..INPUTS_PER_TICK {
            let Some(input) = self.as_mut().rust_mut().hdmi_cec.next_input() else {
                break;
            };
            self.as_mut().dispatch_hdmi_cec(input);
        }
    }
    pub(super) fn dispatch_hdmi_cec(mut self: Pin<&mut Self>, input: Input) {
        let command = match input {
            Input::Command(command) => command,
            Input::Key { key, repeat } => {
                let key = match key {
                    InputKey::Digit(digit) => qt::ffi::cec_digit_key(digit),
                    InputKey::SeekBackward => qt::ffi::cec_seek_key(false),
                    InputKey::SeekForward => qt::ffi::cec_seek_key(true),
                    InputKey::PreviousChapter => qt::ffi::cec_chapter_key(false),
                    InputKey::NextChapter => qt::ffi::cec_chapter_key(true),
                    InputKey::Blue => qt::ffi::cec_blue_key(),
                    InputKey::Red => qt::ffi::cec_red_key(),
                    InputKey::Green => qt::ffi::cec_green_key(),
                    InputKey::Yellow => qt::ffi::cec_yellow_key(),
                };
                self.hdmi_cec_input_requested(key, repeat);
                return;
            }
        };
        match self.as_mut().execute_remote(command) {
            Ok(()) => self.remote_command_executed(viewer_remote::command_kind(command) as i32),
            // Playback operation handlers already recorded their underlying failure.
            Err(viewer_remote::model::CommandError::Playback(_)) => {}
            Err(error) => tracing::warn!(
                error = &error as &dyn std::error::Error,
                "HDMI-CEC player command rejected"
            ),
        }
    }
}
