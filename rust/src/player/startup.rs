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
        let (preferences, settings_error) = if plan.locked {
            (
                settings::Loaded::transient(settings::Preferences {
                    server: String::new(),
                    volume: settings::Volume::from(50.0),
                    show_subtitles: plan.subtitles,
                    comments_enabled: plan.comments,
                    // Explicit feature experiments enable the commentary display
                    // as before, independently of normal startup defaults.
                    danmaku_enabled: plan.comments,
                    ..Default::default()
                }),
                String::new(),
            )
        } else {
            match settings::settings_path().and_then(settings::Loaded::open) {
                Ok(session) => (session, String::new()),
                Err(error) => {
                    tracing::error!("Settings load failed: {error}");
                    (
                        settings::Loaded::transient(settings::Preferences::default()),
                        error.to_string(),
                    )
                }
            }
        };
        let preferences = preferences.activate(
            std::env::var("MIRAKURUN_SERVER").ok(),
            std::env::var("MIRAKURUN_SERVICE_ID").ok(),
        );
        if !plan.locked {
            plan.comments = preferences.preferences().comments_enabled;
        }
        let autoplay_pending = settings::autoplay_requested(
            preferences.preferences().autoplay,
            std::env::var("MIRAKURUN_AUTOPLAY").ok().as_deref(),
        );
        let audio_output =
            playback::audio_output::Output::Audible(preferences.preferences().volume);
        let volume_level = audio_output.volume().fraction();
        let playback = playback::take_preloaded();
        if let Some(playback) = &playback {
            playback.set_audio_output(audio_output);
        }
        let network = services::Network::new();
        let lifecycle_status = match &network {
            Ok(_) => super::lifecycle::Status::Connect,
            Err(error) => super::lifecycle::Status::Failure(
                super::lifecycle::Failure::Network,
                error.to_string(),
            ),
        };
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
                    log_error = error.to_string();
                    None
                }
            },
            Err(_) => None,
        };
        let mut player = Self {
            language: QString::from(preferences.preferences().language.code()),
            ui_language: super::ffi::current_ui_language(),
            diagnostic_recorder,
            diagnostic_ui: Default::default(),
            subtitle_cells: 0,
            error_log,
            log_error: QString::from(log_error),
            server: QString::from(preferences.preferences().server.clone()),
            pending_server: None,
            status: lifecycle_status.render(),
            lifecycle_status,
            playback_error: QString::default(),
            playback_message: QString::default(),
            channel_data: QString::from("[]"),
            channel_program_data: QString::from("[]"),
            channel_visibility_data: QString::from("[]"),
            guide_visibility_data: QString::from("null"),
            channel_program_now: 0.0,
            browser_projection: None,
            selected: -1,
            catalog_selection: Default::default(),
            stream_state: Default::default(),
            timeline: Default::default(),
            timeshift_bytes_per_second: 0.0,
            live_timeline: QString::from("null"),
            transport_error: QString::default(),
            recording_loader: Default::default(),
            file_error: QString::default(),
            subtitles_enabled: plan.subtitles,
            epg_enabled: plan.epg,
            comments_enabled: plan.comments,
            comments_allowed: !plan.locked || plan.comments,
            danmaku_enabled: preferences.preferences().danmaku_enabled,
            comment_font_size: preferences.preferences().comment_font_size.into(),
            comment_opacity: preferences.preferences().comment_opacity.into(),
            comment_speed: preferences.preferences().comment_speed.into(),
            comment_shadow_enabled: preferences.preferences().comment_shadow_enabled,
            comment_program_title: QString::default(),
            comment_model: crate::comment_model::ffi::make_comment_model(),
            activity_data: QString::from("[]"),
            activity: Default::default(),
            comment_status: super::status::tr("Disabled"),
            comment_draft: QString::default(),
            comment_post_status: QString::default(),
            comment_post_target: QString::default(),
            comment_post_available: false,
            comment_post_busy: false,
            comment_send_on_enter: preferences.preferences().comment_send_on_enter,
            comments: Default::default(),
            subtitles_active: false,
            subtitle_display: preferences.preferences().show_subtitles,
            subtitle_data: QString::default(),
            subtitle_status: super::status::tr("Stopped"),
            subtitle_phase: Default::default(),
            epg_data: QString::from("[]"),
            epg_events: Default::default(),
            epg_status: super::status::tr("Disabled"),
            guide_error: None,
            current_program_data: QString::from("null"),
            program_progress: 0.0,
            current_projection: Default::default(),
            next_current_program: Instant::now(),
            diagnostics: QString::default(),
            feature_metrics: None,
            volume_level,
            audio_muted: audio_output.muted(),
            audio_output,
            settings_error: QString::from(settings_error),
            screenshot_error: QString::default(),
            preferences,
            autoplay_pending,
            epg: ProgramInfo::default(),
            guide: Default::default(),
            guide_dirty: false,
            guide_revision: 0,
            next_diagnostic: Instant::now(),
            request: Default::default(),
            channel_refresh: Default::default(),
            network: network.ok(),
            remote: crate::remote::Control::load(plan.locked),
            media: playback::Session::new(playback),
            entries: vec![],
        };
        player.start_remote_if_requested();
        player
    }
}
