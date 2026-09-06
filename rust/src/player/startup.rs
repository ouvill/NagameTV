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
                    comments_enabled: plan.comments,
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
            plan.comments = preferences.preferences().comments_enabled;
        }
        let audio_output =
            playback::audio_output::Output::Audible(preferences.preferences().volume);
        let volume_level = audio_output.volume().fraction();
        let playback = playback::take_preloaded();
        if let Some(playback) = &playback {
            playback.set_audio_output(audio_output);
        }
        let network = services::Network::new();
        let status = network
            .as_ref()
            .err()
            .map(ToString::to_string)
            .unwrap_or_else(|| "サーバーに接続してください".into());
        let error_log = crate::error_log::ErrorLog::new(
            super::ffi::playback_log_directory().to_string().into(),
        );
        let mut log_error = error_log
            .as_ref()
            .err()
            .map(ToString::to_string)
            .unwrap_or_default();
        let diagnostic_recorder = match &error_log {
            Ok(log) => match crate::diagnostics::start(log.directory().to_owned(), plan.locked) {
                Ok(recorder) => recorder,
                Err(error) => {
                    log_error = error;
                    None
                }
            },
            Err(_) => None,
        };
        Self {
            diagnostic_recorder,
            diagnostic_ui: Default::default(),
            subtitle_cells: 0,
            error_log,
            log_error: QString::from(log_error),
            server: QString::from(preferences.preferences().server.clone()),
            status: QString::from(status),
            playback_error: QString::default(),
            channel_data: QString::from("[]"),
            channel_program_data: QString::from("[]"),
            channel_visibility_data: QString::from("[]"),
            channel_program_now: 0.0,
            browser_projection: None,
            selected: -1,
            loading: false,
            playing: false,
            subtitles_enabled: plan.subtitles,
            epg_enabled: plan.epg,
            comments_enabled: plan.comments,
            comments_allowed: !plan.locked || plan.comments,
            danmaku_enabled: preferences.preferences().danmaku_enabled,
            comment_font_size: preferences.preferences().comment_font_size.into(),
            comment_opacity: preferences.preferences().comment_opacity.into(),
            comment_speed: preferences.preferences().comment_speed.into(),
            comment_data: QString::from("[]"),
            comment_status: QString::from("無効"),
            comments_visible: false,
            comments: Default::default(),
            subtitles_allowed: !plan.locked || plan.subtitles,
            epg_allowed: !plan.locked || plan.epg,
            subtitles_active: false,
            subtitle_display: true,
            subtitle_data: QString::default(),
            subtitle_status: QString::from("停止中"),
            epg_data: QString::from("[]"),
            epg_status: QString::from("無効"),
            current_program_data: QString::from("null"),
            program_progress: 0.0,
            current_projection: Default::default(),
            next_current_program: Instant::now(),
            diagnostics: QString::default(),
            volume_level,
            audio_muted: audio_output.muted(),
            audio_output,
            settings_error: QString::from(settings_error),
            preferences,
            autoplay_pending: std::env::var("MIRAKURUN_AUTOPLAY").as_deref() == Ok("1"),
            subtitle_session: None,
            epg: ProgramInfo::default(),
            active_service: None,
            resume_retry_used: false,
            guide: Default::default(),
            guide_dirty: false,
            guide_revision: 0,
            guide_service: None,
            next_diagnostic: Instant::now(),
            request: None,
            network: network.ok(),
            playback,
            entries: vec![],
        }
    }
}
