mod policy;

use super::ffi;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use policy::{SelectionAction, adjacent_index};
use std::pin::Pin;

impl ffi::Player {
    pub fn select_channel(mut self: Pin<&mut Self>, index: i32) {
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        let Some(&service_id) = self.as_ref().rust().service_ids.get(index) else {
            return;
        };
        let current = self.as_ref().service_id().to_string().parse::<u64>().ok();
        if SelectionAction::for_request(current, service_id, *self.as_ref().playing())
            == SelectionAction::Keep
        {
            return;
        }
        self.as_ref().record_diagnostics("channel_selected");
        let channel_name = self
            .as_ref()
            .services()
            .get(index as isize)
            .cloned()
            .unwrap_or_default();
        let program_name = self
            .as_ref()
            .program_titles()
            .get(index as isize)
            .cloned()
            .unwrap_or_default();
        let program_description = self
            .as_ref()
            .program_descriptions()
            .get(index as isize)
            .cloned()
            .unwrap_or_default();
        let program_start = self
            .as_ref()
            .program_starts()
            .get(index as isize)
            .and_then(|value| value.to_string().parse::<u64>().ok())
            .unwrap_or(0);
        let program_duration = self
            .as_ref()
            .program_durations()
            .get(index as isize)
            .and_then(|value| value.to_string().parse::<u64>().ok())
            .unwrap_or(0);
        let logo_url = self
            .as_ref()
            .channel_logo_urls()
            .get(index as isize)
            .cloned()
            .unwrap_or_default();
        self.as_mut()
            .set_service_id(QString::from(service_id.to_string()));
        self.as_mut().set_channel_name(channel_name);
        self.as_mut().set_program_name(program_name);
        self.as_mut().set_program_description(program_description);
        self.as_mut().set_channel_logo_url(logo_url);
        self.as_mut().rust_mut().current_program_start = program_start;
        self.as_mut().rust_mut().current_program_duration = program_duration;
        self.as_mut().restart_comments();
        self.as_mut().save_settings();
        self.play();
    }

    pub fn change_channel(self: Pin<&mut Self>, offset: i32) {
        let current = self.as_ref().service_id().to_string().parse::<u64>().ok();
        if let Some(next) = adjacent_index(&self.as_ref().rust().service_ids, current, offset) {
            self.select_channel(next);
        }
    }
}

pub(super) fn service_logo_url(server: &str, service_id: u64) -> String {
    format!(
        "{}/api/services/{service_id}/logo",
        server.trim().trim_end_matches('/')
    )
}
