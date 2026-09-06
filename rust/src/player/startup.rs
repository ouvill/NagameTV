//! Assemble startup preferences and runtime resources before handling UI events.
use super::PlayerRust;
use crate::features::program_info::ProgramInfo;
use crate::{features, playback, services, settings};
use cxx_qt_lib::QString;
use std::time::Instant;

impl Default for PlayerRust {
    fn default() -> Self {
        // main sets PLAN before registering/constructing any Qt Player object.
        let mut plan = *features::PLAN
            .get()
            .expect("LaunchPlan initialized before Qt");
        let (mut preferences, settings_error) = if plan.locked {
            (
                settings::Session::transient(settings::Preferences {
                    server: String::new(),
                    volume: settings::Volume::from(50.0),
                    subtitles_enabled: plan.subtitles,
                    epg_enabled: plan.epg,
                    ..Default::default()
                }),
                String::new(),
            )
        } else {
            match settings::settings_path().and_then(settings::Session::open) {
                Ok(session) => (session, String::new()),
                Err(error) => {
                    eprintln!("Settings load failed: {error}");
                    (
                        settings::Session::transient(settings::Preferences::default()),
                        error.to_string(),
                    )
                }
            }
        };
        preferences.preferences_mut().apply_overrides(
            std::env::var("MIRAKURUN_SERVER").ok(),
            std::env::var("MIRAKURUN_SERVICE_ID").ok(),
        );
        if !plan.locked {
            plan.subtitles = preferences.preferences().subtitles_enabled;
            plan.epg = preferences.preferences().epg_enabled;
        }
        let volume_level = preferences.preferences().volume.fraction();
        let playback = playback::take_preloaded();
        if let Some(playback) = &playback {
            playback.set_volume(volume_level);
        }
        let network = services::Network::new();
        let status = network
            .as_ref()
            .err()
            .map(ToString::to_string)
            .unwrap_or_else(|| "サーバーに接続してください".into());
        Self {
            server: QString::from(preferences.preferences().server.clone()),
            status: QString::from(status),
            channel_data: QString::from("[]"),
            selected: -1,
            loading: false,
            subtitles_enabled: plan.subtitles,
            epg_enabled: plan.epg,
            subtitles_allowed: !plan.locked || plan.subtitles,
            epg_allowed: !plan.locked || plan.epg,
            subtitles_active: false,
            subtitle_display: true,
            subtitle_data: QString::default(),
            subtitle_status: QString::from("停止中"),
            epg_data: QString::from("[]"),
            epg_status: QString::from("無効"),
            diagnostics: QString::default(),
            volume_level,
            settings_error: QString::from(settings_error),
            preferences,
            autoplay_pending: std::env::var("MIRAKURUN_AUTOPLAY").as_deref() == Ok("1"),
            subtitle_session: None,
            epg: ProgramInfo::default(),
            active_service: None,
            resume_retry_used: false,
            guide_visible: false,
            guide_revision: 0,
            guide_service: None,
            next_guide: Instant::now(),
            next_diagnostic: Instant::now(),
            request: None,
            network: network.ok(),
            playback,
            entries: vec![],
        }
    }
}
