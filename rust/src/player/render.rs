//! One-way projection: these fields are never read by application decisions.
use super::{PlayerRust, ffi};
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QStringList};
use std::{pin::Pin, sync::Arc};
use viewer_core::app::PlaybackState;

type Notification = fn(Pin<&mut ffi::Player>);

// Install every field before notifying QML, so signal handlers see a coherent view.
macro_rules! field {
    ($view:ident, $notifications:ident, $name:ident, $signal:ident, $value:expr) => {
        let value = $value;
        if $view.$name != value {
            $view.$name = value;
            $notifications.push(|player: Pin<&mut ffi::Player>| {
                player.$signal();
            });
        }
    };
}

pub(super) fn project(view: &mut PlayerRust) -> Vec<Notification> {
    let state = view.runtime.state().clone();
    let prefs = state.preferences();
    let channel = state.selected_channel();
    let program = state.current_program();
    let failure = match state.playback() {
        PlaybackState::Failed(e) => Some(e),
        _ => None,
    };
    let mut notifications: Vec<Notification> = Vec::new();
    field!(
        view,
        notifications,
        server,
        server_changed,
        QString::from(prefs.server.clone())
    );
    field!(
        view,
        notifications,
        language,
        language_changed,
        QString::from(prefs.language.clone())
    );
    field!(
        view,
        notifications,
        ui_language,
        ui_language_changed,
        QString::from(state.ui_language().to_owned())
    );
    field!(
        view,
        notifications,
        service_id,
        service_id_changed,
        QString::from(
            state
                .selected()
                .map(|id| id.to_string())
                .unwrap_or_default()
        )
    );
    field!(
        view,
        notifications,
        status,
        status_changed,
        QString::from(state.status().to_owned())
    );
    field!(
        view,
        notifications,
        playback_error,
        playback_error_changed,
        QString::from(failure.map(|e| e.summary.clone()).unwrap_or_default())
    );
    field!(
        view,
        notifications,
        playback_error_details,
        playback_error_details_changed,
        QString::from(failure.map(|e| e.details.clone()).unwrap_or_default())
    );
    field!(
        view,
        notifications,
        playing,
        playing_changed,
        state.playback().active()
    );
    field!(view, notifications, volume, volume_changed, prefs.volume);
    field!(
        view,
        notifications,
        audio_muted,
        audio_muted_changed,
        state.audio_muted()
    );
    field!(
        view,
        notifications,
        audio_tracks,
        audio_tracks_changed,
        QString::from(
            serde_json::to_string(state.audio_tracks()).unwrap_or_else(|error| {
                tracing::warn!(%error, "Could not serialize audio options");
                "[]".into()
            })
        )
    );
    field!(
        view,
        notifications,
        audio_error,
        audio_error_changed,
        QString::from(state.audio_error().to_owned())
    );
    field!(
        view,
        notifications,
        danmaku_enabled,
        danmaku_enabled_changed,
        prefs.danmaku_enabled
    );
    field!(
        view,
        notifications,
        comment_font_size,
        comment_font_size_changed,
        prefs.comment_font_size
    );
    field!(
        view,
        notifications,
        comment_opacity,
        comment_opacity_changed,
        prefs.comment_opacity
    );
    field!(
        view,
        notifications,
        comment_speed,
        comment_speed_changed,
        prefs.comment_speed
    );
    field!(
        view,
        notifications,
        subtitles_enabled,
        subtitles_enabled_changed,
        prefs.subtitles_enabled
    );
    field!(
        view,
        notifications,
        autoplay,
        autoplay_changed,
        state.autoplay()
    );
    field!(
        view,
        notifications,
        channel_name,
        channel_name_changed,
        QString::from(channel.map(|c| c.label.clone()).unwrap_or_default())
    );
    field!(
        view,
        notifications,
        program_name,
        program_name_changed,
        QString::from(program.and_then(|p| p.name.clone()).unwrap_or_default())
    );
    field!(
        view,
        notifications,
        program_description,
        program_description_changed,
        QString::from(
            program
                .and_then(|p| p.description.clone())
                .unwrap_or_default()
        )
    );
    field!(
        view,
        notifications,
        channel_logo_url,
        channel_logo_url_changed,
        QString::from(
            channel
                .map(|c| logo(&prefs.server, c.id, c.has_logo_data))
                .unwrap_or_default()
        )
    );
    field!(
        view,
        notifications,
        program_progress,
        program_progress_changed,
        state.progress()
    );
    field!(
        view,
        notifications,
        comment_status,
        comment_status_changed,
        QString::from(state.comment_status().to_owned())
    );
    let catalog_changed = view
        .rendered
        .as_ref()
        .is_none_or(|old| !Arc::ptr_eq(old.catalog(), state.catalog()));
    let programs_changed = catalog_changed
        || view.rendered.as_ref().is_none_or(|old| {
            !Arc::ptr_eq(old.epg(), state.epg()) || old.now_ms() / 1000 != state.now_ms() / 1000
        });
    if catalog_changed {
        let channels = &state.catalog().channels;
        let guide = &state.catalog().guide;
        field!(
            view,
            notifications,
            services,
            services_changed,
            strings(channels.iter().map(|c| c.label.clone()))
        );
        field!(
            view,
            notifications,
            channel_logo_urls,
            channel_logo_urls_changed,
            strings(
                channels
                    .iter()
                    .map(|c| logo(&prefs.server, c.id, c.has_logo_data))
            )
        );
        field!(
            view,
            notifications,
            channel_types,
            channel_types_changed,
            strings(channels.iter().map(|c| c.channel_type.clone()))
        );
        field!(
            view,
            notifications,
            jikkyo_forces,
            jikkyo_forces_changed,
            strings(
                channels
                    .iter()
                    .map(|c| c.jikkyo_force.map(|v| v.to_string()).unwrap_or_default())
            )
        );
        field!(
            view,
            notifications,
            guide_program_ids,
            guide_program_ids_changed,
            strings(guide.iter().map(|p| p.id.to_string()))
        );
        field!(
            view,
            notifications,
            guide_channel_indices,
            guide_channel_indices_changed,
            strings(guide.iter().map(|p| p.channel_index.to_string()))
        );
        field!(
            view,
            notifications,
            guide_titles,
            guide_titles_changed,
            strings(guide.iter().map(|p| p.title.clone()))
        );
        field!(
            view,
            notifications,
            guide_descriptions,
            guide_descriptions_changed,
            strings(guide.iter().map(|p| p.description.clone()))
        );
        field!(
            view,
            notifications,
            guide_starts,
            guide_starts_changed,
            strings(guide.iter().map(|p| p.start_at.to_string()))
        );
        field!(
            view,
            notifications,
            guide_durations,
            guide_durations_changed,
            strings(guide.iter().map(|p| p.duration.to_string()))
        );
        field!(
            view,
            notifications,
            guide_genres,
            guide_genres_changed,
            strings(guide.iter().map(|p| p.genre.to_string()))
        );
        field!(
            view,
            notifications,
            guide_start,
            guide_start_changed,
            QString::from(state.catalog().guide_start.to_string())
        );
    }
    if programs_changed {
        let programs = state
            .catalog()
            .channels
            .iter()
            .map(|c| state.epg().current_program(c.id, state.now_ms()))
            .collect::<Vec<_>>();
        field!(
            view,
            notifications,
            program_titles,
            program_titles_changed,
            strings(
                programs
                    .iter()
                    .map(|p| p.and_then(|p| p.name.clone()).unwrap_or_default())
            )
        );
        field!(
            view,
            notifications,
            program_descriptions,
            program_descriptions_changed,
            strings(
                programs
                    .iter()
                    .map(|p| p.and_then(|p| p.description.clone()).unwrap_or_default())
            )
        );
        field!(
            view,
            notifications,
            program_starts,
            program_starts_changed,
            strings(
                programs
                    .iter()
                    .map(|p| p.map_or(0, |p| p.start_at).to_string())
            )
        );
        field!(
            view,
            notifications,
            program_durations,
            program_durations_changed,
            strings(
                programs
                    .iter()
                    .map(|p| p.map_or(0, |p| p.duration).to_string())
            )
        );
    }
    if view
        .rendered
        .as_ref()
        .is_none_or(|old| !Arc::ptr_eq(old.comments(), state.comments()))
    {
        field!(
            view,
            notifications,
            comment_times,
            comment_times_changed,
            strings(state.comments().iter().map(|c| c.time.clone()))
        );
        field!(
            view,
            notifications,
            comment_texts,
            comment_texts_changed,
            strings(state.comments().iter().map(|c| c.text.clone()))
        );
        field!(
            view,
            notifications,
            comment_sources,
            comment_sources_changed,
            strings(state.comments().iter().map(|c| c.source.clone()))
        );
    }
    let subtitle_changed =
        view.rendered
            .as_ref()
            .is_none_or(|old| match (old.subtitle(), state.subtitle()) {
                (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
                (None, None) => false,
                _ => true,
            });
    if subtitle_changed {
        let (text, data) = state
            .subtitle()
            .map(|cue| {
                serde_json::to_string(cue.as_ref())
                    .map(|data| (cue.text.clone(), data))
                    .unwrap_or_else(|error| {
                        tracing::warn!(%error, "Could not serialize subtitle cue");
                        Default::default()
                    })
            })
            .unwrap_or_default();
        field!(
            view,
            notifications,
            subtitle_text,
            subtitle_text_changed,
            QString::from(text)
        );
        field!(
            view,
            notifications,
            subtitle_data,
            subtitle_data_changed,
            QString::from(data)
        );
    }
    view.rendered = Some(state);
    notifications
}
impl ffi::Player {
    pub(super) fn render(mut self: Pin<&mut Self>) {
        let notifications = project(&mut self.as_mut().rust_mut());
        for notify in notifications {
            notify(self.as_mut());
        }
    }
}
fn strings(values: impl Iterator<Item = String>) -> QStringList {
    values.map(QString::from).collect()
}
fn logo(server: &str, id: u64, available: bool) -> String {
    if available {
        format!(
            "{}/api/services/{id}/logo",
            server.trim().trim_end_matches('/')
        )
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use viewer_core::{
        app::{self, Event, State},
        channels::build_catalog,
        epg::{EpgSnapshot, Program, Service},
        settings::Settings,
    };

    #[test]
    fn projects_program_gap_without_reading_stale_qt_properties()
    -> Result<(), Box<dyn std::error::Error>> {
        let services: Vec<Service> = serde_json::from_str(
            r#"[{"id":101,"serviceId":101,"networkId":1,"type":1,"name":"BS","channel":{"type":"BS","channel":"1"}}]"#,
        )?;
        let programs: Vec<Program> = serde_json::from_str(
            r#"[{"id":1,"eventId":1,"serviceId":101,"networkId":1,"startAt":1000,"duration":1000,"name":"News","description":"Description"}]"#,
        )?;
        let epg = Arc::new(EpgSnapshot::new(services, programs, 1500));
        let state = State::new(
            Settings {
                service_id: "101".into(),
                ..Default::default()
            },
            "en".into(),
            false,
        );
        let state = app::update(&state, Event::Tick(1500)).state;
        let state = app::update(&state, Event::Refresh { force: true }).state;
        // Pure result fixture: no network request or playback device is started.
        let state = app::update(
            &state,
            Event::CatalogLoaded {
                request: 1,
                result: Ok((Arc::new(build_catalog(&epg, 1500)), epg)),
            },
        )
        .state;
        let mut view =
            PlayerRust::from_runtime(crate::runtime::Runtime::unavailable_for_test(state));
        assert_eq!(view.program_name.to_string(), "News");
        assert_eq!(view.program_description.to_string(), "Description");
        assert_eq!(view.program_progress, 0.5);
        assert_eq!(
            view.program_titles.get(0).map(ToString::to_string),
            Some("News".into())
        );
        assert!(
            project(&mut view).is_empty(),
            "unchanged projection emits no notifications"
        );
        // Even a corrupted display field cannot become input to the application.
        view.program_name = QString::from("stale view");
        view.runtime.dispatch(Event::Tick(2000));
        assert!(!project(&mut view).is_empty());
        assert!(view.program_name.is_empty());
        assert!(view.program_description.is_empty());
        assert_eq!(view.program_progress, 0.0);
        assert_eq!(
            view.program_starts.get(0).map(ToString::to_string),
            Some("0".into())
        );
        assert_eq!(
            view.program_durations.get(0).map(ToString::to_string),
            Some("0".into())
        );
        assert!(project(&mut view).is_empty());
        Ok(())
    }
}
