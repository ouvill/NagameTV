//! QObject projection of the Rust-owned HDMI-CEC configuration.
use super::ffi;
use crate::{
    hdmi_cec::{OSD_NAME, SelectionError},
    settings,
};
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::pin::Pin;

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
    pub fn hdmi_cec_external_without_passthrough(&self) -> bool {
        self.rust()
            .hdmi_cec
            .snapshot()
            .state
            .external_without_passthrough()
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
        match self.as_mut().rust_mut().hdmi_cec.select(path.clone()) {
            Ok(()) => {}
            Err(SelectionError::InvalidPath) => return false,
            Err(SelectionError::ReleaseFailed) => {
                self.notify_hdmi_cec();
                return false;
            }
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
}
