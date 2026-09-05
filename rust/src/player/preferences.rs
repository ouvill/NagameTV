use super::ffi;
use crate::settings::Settings;
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QStringList};
use std::pin::Pin;

impl ffi::Player {
    pub fn change_language(mut self: Pin<&mut Self>, language: QString) -> bool {
        let preference = crate::settings::normalize_language(&language.to_string());
        let language = QString::from(preference);
        let effective = ffi::apply_ui_language(&language);
        if effective.is_empty() {
            tracing::error!("Could not load UI translation");
            self.as_mut()
                .set_status(QString::from("Could not load UI translation"));
            return false;
        }
        self.as_mut().set_language(language);
        self.as_mut().set_ui_language(effective);
        self.as_mut().save_settings();
        true
    }

    pub fn connect_server(mut self: Pin<&mut Self>, server: QString) -> bool {
        let server = server.to_string().trim().trim_end_matches('/').to_owned();
        if !reqwest::Url::parse(&server)
            .is_ok_and(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
        {
            self.as_mut().set_status(QString::from(
                "Enter a server URL starting with http:// or https://",
            ));
            return false;
        }
        if self.as_ref().server().to_string() != server {
            self.as_mut().stop();
            if *self.as_ref().playing() {
                return false;
            }
            // Cancel IO and drop any queued payload before changing servers.
            self.as_mut().rust_mut().catalog_request.cancel();
            if let Some(network) = self.as_ref().rust().network.as_ref() {
                network.epg().publish(std::sync::Arc::default());
            }
            self.as_mut().set_server(QString::from(server));
            self.as_mut().set_service_id(QString::default());
            self.as_mut().set_channel_name(QString::default());
            self.as_mut().set_program_name(QString::default());
            self.as_mut().set_program_description(QString::default());
            self.as_mut().set_channel_logo_url(QString::default());
            self.as_mut().rust_mut().subtitle_cue = None;
            self.as_mut().set_subtitle_text(QString::default());
            self.as_mut().set_subtitle_data(QString::default());
            self.as_mut().rust_mut().service_ids.clear();
            self.as_mut().rust_mut().jikkyo_ids.clear();
            self.as_mut().set_services(QStringList::default());
            self.as_mut().set_channel_types(QStringList::default());
            self.as_mut().set_program_titles(QStringList::default());
            self.as_mut()
                .set_program_descriptions(QStringList::default());
            self.as_mut().set_program_starts(QStringList::default());
            self.as_mut().set_program_durations(QStringList::default());
            self.as_mut().set_channel_logo_urls(QStringList::default());
            self.as_mut().set_jikkyo_forces(QStringList::default());
            self.as_mut().set_guide_program_ids(QStringList::default());
            self.as_mut()
                .set_guide_channel_indices(QStringList::default());
            self.as_mut().set_guide_titles(QStringList::default());
            self.as_mut().set_guide_descriptions(QStringList::default());
            self.as_mut().set_guide_starts(QStringList::default());
            self.as_mut().set_guide_durations(QStringList::default());
            self.as_mut().set_guide_genres(QStringList::default());
            self.as_mut().rust_mut().current_program_start = 0;
            self.as_mut().rust_mut().current_program_duration = 0;
            self.as_mut().set_program_progress(0.0);
            self.as_mut().restart_comments();
        }
        self.as_mut().save_settings();
        self.as_mut().refresh_channels();
        true
    }

    pub fn save_settings(mut self: Pin<&mut Self>) {
        let settings = Settings {
            server: self.as_ref().server().to_string(),
            language: self.as_ref().language().to_string(),
            service_id: self.as_ref().service_id().to_string(),
            volume: (*self.as_ref().volume()).clamp(0.0, 100.0),
            danmaku_enabled: *self.as_ref().danmaku_enabled(),
            comment_font_size: (*self.as_ref().comment_font_size()).clamp(12.0, 48.0),
            comment_opacity: (*self.as_ref().comment_opacity()).clamp(0.1, 1.0),
            comment_speed: (*self.as_ref().comment_speed()).clamp(0.5, 2.0),
            subtitles_enabled: *self.as_ref().subtitles_enabled(),
        };
        if let Err(error) = settings.save() {
            tracing::warn!(%error, "Could not save settings");
            self.as_mut()
                .set_status(QString::from(format!("Could not save settings: {error}")));
        }
    }
}
