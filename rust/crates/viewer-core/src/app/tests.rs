use super::*;
use crate::{
    channels::build_catalog,
    epg::{Program, Service},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn initial() -> State {
    State::new(
        Settings {
            server: "http://a.test".into(),
            ..Default::default()
        },
        "en".into(),
        false,
    )
}
fn fixture(now: u64) -> Result<(Arc<ChannelCatalog>, Arc<EpgSnapshot>), serde_json::Error> {
    let services: Vec<Service> = serde_json::from_str(
        r#"[
        {"id":101,"serviceId":101,"networkId":1,"type":1,"name":"NHK BS","channel":{"type":"BS","channel":"BS01"}},
        {"id":200,"serviceId":200,"networkId":1,"type":1,"name":"BS10","channel":{"type":"BS","channel":"BS03"}}
    ]"#,
    )?;
    let programs: Vec<Program> = serde_json::from_str(
        r#"[
        {"id":1,"eventId":1,"serviceId":101,"networkId":1,"startAt":1000,"duration":1000,"name":"News","description":"First programme",
        "audios":[{"componentTag":16,"componentType":2,"isMain":true,"langs":["jpn"]}]},
        {"id":2,"eventId":2,"serviceId":101,"networkId":1,"startAt":3000,"duration":1000,"name":"Weather"},
        {"id":3,"eventId":3,"serviceId":200,"networkId":1,"startAt":1000,"duration":5000,"name":"Film"}
    ]"#,
    )?;
    let epg = Arc::new(EpgSnapshot::new(services, programs, now));
    Ok((Arc::new(build_catalog(&epg, now)), epg))
}
fn loaded() -> Result<State, serde_json::Error> {
    let state = update(&initial(), Event::Tick(1500)).state;
    let transition = update(&state, Event::Refresh { force: true });
    Ok(update(
        &transition.state,
        Event::CatalogLoaded {
            request: 1,
            result: Ok(fixture(1500)?),
        },
    )
    .state)
}
fn select(state: &State, id: u64) -> Transition {
    let index = state
        .catalog
        .channels
        .iter()
        .position(|c| c.id == id)
        .expect("fixture channel");
    update(state, Event::SelectChannel(index))
}

#[test]
fn selection_preserves_previous_snapshot_and_emits_typed_effects() -> TestResult {
    let before = loaded()?;
    let after = select(&before, 101);
    assert_eq!(before.selected(), None);
    assert_eq!(after.state.selected(), Some(101));
    assert_eq!(
        after
            .state
            .current_program()
            .and_then(|p| p.name.as_deref()),
        Some("News")
    );
    assert_eq!(after.state.progress(), 0.5);
    assert!(Arc::ptr_eq(before.epg(), after.state.epg()));
    assert!(Arc::ptr_eq(before.catalog(), after.state.catalog()));
    assert!(after.effects.iter().any(
        |e| matches!(e, Effect::StartPlayback { service: 101, program: Some(program), .. }
        if program.audios[0].langs == ["jpn"])
    ));
    assert!(
        after
            .effects
            .iter()
            .any(|e| matches!(e, Effect::SaveSettings(settings) if settings.service_id == "101"))
    );
    assert!(
        after.effects.iter().any(
            |e| matches!(e, Effect::ConnectComments { channel: Some(id), .. } if id == "jk101")
        )
    );
    Ok(())
}

#[test]
fn missing_current_program_clears_details_progress_and_audio() -> TestResult {
    let selected = select(&loaded()?, 101).state;
    let gap = update(&selected, Event::Tick(2000));
    assert!(gap.state.current_program().is_none());
    assert_eq!(gap.state.progress(), 0.0);
    assert!(
        gap.effects
            .iter()
            .any(|e| matches!(e, Effect::SetAudioProgram(None)))
    );
    assert_eq!(
        selected.current_program().and_then(|p| p.name.as_deref()),
        Some("News")
    );
    let next = update(&gap.state, Event::Tick(3000));
    assert_eq!(
        next.state.current_program().and_then(|p| p.name.as_deref()),
        Some("Weather")
    );
    assert!(
        next.effects
            .iter()
            .any(|e| matches!(e, Effect::SetAudioProgram(Some(p)) if p.start_at == 3000))
    );
    Ok(())
}

#[test]
fn repeated_selection_is_noop_but_stop_and_failure_allow_retry() -> TestResult {
    let active = select(&loaded()?, 101).state;
    assert!(select(&active, 101).effects.is_empty());
    let stopped = update(&active, Event::PlaybackStopped).state;
    assert!(
        select(&stopped, 101)
            .effects
            .iter()
            .any(|e| matches!(e, Effect::StartPlayback { .. }))
    );
    let failed = update(
        &active,
        Event::PlaybackFailed(Failure {
            summary: "offline".into(),
            details: "network error".into(),
        }),
    )
    .state;
    assert!(
        select(&failed, 101)
            .effects
            .iter()
            .any(|e| matches!(e, Effect::StartPlayback { .. }))
    );
    assert!(
        select(&active, 200)
            .effects
            .iter()
            .any(|e| matches!(e, Effect::StartPlayback { service: 200, .. }))
    );
    Ok(())
}

#[test]
fn server_switch_rejects_old_catalog_even_after_a_b_a() -> TestResult {
    let first = update(&initial(), Event::Refresh { force: true }).state;
    let second = update(&first, Event::ConnectServer("http://b.test".into())).state;
    let third = update(&second, Event::ConnectServer("http://a.test".into())).state;
    for request in [1, 2] {
        let old = update(
            &third,
            Event::CatalogLoaded {
                request,
                result: Ok(fixture(1500)?),
            },
        );
        assert!(old.state.catalog.channels.is_empty());
        assert!(old.state.loading());
        assert!(old.effects.is_empty());
    }
    let accepted = update(
        &third,
        Event::CatalogLoaded {
            request: 3,
            result: Ok(fixture(1500)?),
        },
    );
    assert!(!accepted.state.catalog.channels.is_empty());
    assert!(!accepted.state.loading());
    assert!(
        update(&accepted.state, Event::EpgChanged { generation: 0 })
            .effects
            .is_empty()
    );
    Ok(())
}

#[test]
fn server_change_resets_selection_and_rejects_old_comments() -> TestResult {
    let selected = select(&loaded()?, 101).state;
    let generation = selected.comment_generation();
    let with_comment = update(
        &selected,
        Event::CommentReceived {
            generation,
            comment: Comment {
                time: "now".into(),
                text: "hello".into(),
                source: "test".into(),
            },
            initial: false,
        },
    )
    .state;
    let switched = update(&with_comment, Event::ConnectServer("http://b.test/".into()));
    assert!(switched.state.selected().is_none());
    assert!(switched.state.current_program().is_none());
    assert!(switched.state.comments().is_empty());
    assert!(switched.state.catalog.channels.is_empty());
    assert_eq!(switched.state.preferences.server, "http://b.test");
    assert!(
        switched
            .effects
            .iter()
            .any(|e| matches!(e, Effect::CancelCatalog))
    );
    let stale = update(
        &switched.state,
        Event::CommentStatus {
            generation,
            status: "stale".into(),
            connected: true,
        },
    );
    assert_ne!(stale.state.comment_status(), "stale");
    assert_eq!(with_comment.comments().len(), 1);
    Ok(())
}

#[test]
fn comment_history_is_bounded_and_initial_batch_is_not_presented() {
    let original = initial();
    let mut state = original.clone();
    for n in 0..250 {
        let transition = update(
            &state,
            Event::CommentReceived {
                generation: 0,
                comment: Comment {
                    time: String::new(),
                    text: n.to_string(),
                    source: String::new(),
                },
                initial: n == 0,
            },
        );
        assert_eq!(
            transition
                .effects
                .iter()
                .any(|e| matches!(e, Effect::PresentComment(_))),
            n != 0
        );
        state = transition.state;
    }
    assert!(original.comments().is_empty());
    assert_eq!(state.comments().len(), 200);
    assert_eq!(
        state.comments().front().map(|c| c.text.as_str()),
        Some("50")
    );
    assert_eq!(
        state.comments().back().map(|c| c.text.as_str()),
        Some("249")
    );
}

#[test]
fn preferences_are_finite_and_effects_apply_effective_volume() {
    let state = initial();
    for v in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let next = update(&state, Event::Preference(Preference::Volume(v)));
        assert_eq!(next.state.preferences.volume, state.preferences.volume);
        assert!(next.effects.is_empty());
    }
    let muted = update(&state, Event::Preference(Preference::Muted(true)));
    assert!(matches!(muted.effects.as_slice(), [Effect::SetVolume(0.0)]));
    let changed = update(&muted.state, Event::Preference(Preference::Volume(35.0)));
    assert!(changed.effects.is_empty());
    let audible = update(&changed.state, Event::Preference(Preference::Muted(false)));
    assert!(matches!(
        audible.effects.as_slice(),
        [Effect::SetVolume(35.0)]
    ));
    assert_eq!(state.preferences.volume, 70.0);
}

#[test]
fn scheduler_coalesces_refresh_and_autoplay_runs_once_after_video_readiness() {
    let state = State::new(
        Settings {
            service_id: "101".into(),
            ..Default::default()
        },
        "en".into(),
        true,
    );
    let ticking = update(&state, Event::Tick(100_000));
    assert!(ticking.effects.is_empty());
    let ready = update(&ticking.state, Event::VideoReady);
    assert!(
        ready
            .effects
            .iter()
            .any(|e| matches!(e, Effect::StartPlayback { service: 101, .. }))
    );
    assert!(
        ready
            .effects
            .iter()
            .any(|e| matches!(e, Effect::LoadCatalog { .. }))
    );
    assert!(update(&ready.state, Event::VideoReady).effects.is_empty());
    assert!(
        update(&ready.state, Event::Refresh { force: true })
            .effects
            .is_empty()
    );
    let complete = update(
        &ready.state,
        Event::CatalogLoaded {
            request: 1,
            result: Err("offline".into()),
        },
    )
    .state;
    assert!(update(&complete, Event::Tick(399_999)).effects.is_empty());
    assert!(
        update(&complete, Event::Tick(400_000))
            .effects
            .iter()
            .any(|e| matches!(e, Effect::LoadCatalog { .. }))
    );
}

#[test]
fn settings_and_language_results_reenter_the_core() {
    let before = initial();
    let requested = update(&before, Event::ChangeLanguage("ja".into()));
    assert_eq!(requested.state.ui_language(), "en");
    assert!(matches!(requested.effects.as_slice(), [Effect::ApplyLanguage(v)] if v == "ja"));
    let failed = update(
        &requested.state,
        Event::LanguageApplied {
            preference: "ja".into(),
            result: Err("no translation".into()),
        },
    );
    assert_eq!(failed.state.ui_language(), "en");
    assert!(failed.effects.is_empty());
    let applied = update(
        &requested.state,
        Event::LanguageApplied {
            preference: "ja".into(),
            result: Ok("ja".into()),
        },
    );
    assert_eq!(applied.state.ui_language(), "ja");
    assert!(matches!(applied.effects.as_slice(), [Effect::SaveSettings(s)] if s.language == "ja"));
    let save_failed = update(
        &applied.state,
        Event::SettingsSaved(Err("read-only filesystem".into())),
    );
    assert_eq!(
        save_failed.state.status(),
        "Could not save settings: read-only filesystem"
    );
    assert_eq!(before.ui_language(), "en");
}

#[test]
fn invalid_server_has_no_side_effects() {
    let before = initial();
    for server in ["", "garbage", "file:///etc/passwd"] {
        let after = update(&before, Event::ConnectServer(server.into()));
        assert_eq!(after.state.preferences.server, before.preferences.server);
        assert!(after.effects.is_empty());
    }
}

#[test]
fn subtitles_commit_only_after_success_and_clear_on_stop() -> TestResult {
    let selected = select(&loaded()?, 101).state;
    let requested = update(&selected, Event::Preference(Preference::Subtitles(true)));
    assert!(!requested.state.preferences.subtitles_enabled);
    assert!(matches!(
        requested.effects.as_slice(),
        [Effect::SetSubtitles(true)]
    ));
    let failed = update(
        &requested.state,
        Event::SubtitlesApplied {
            enabled: true,
            result: Err(Failure {
                summary: "decoder unavailable".into(),
                details: "lock poisoned".into(),
            }),
        },
    );
    assert!(!failed.state.preferences.subtitles_enabled);
    assert!(matches!(failed.state.playback(), PlaybackState::Failed(_)));
    let enabled = update(
        &requested.state,
        Event::SubtitlesApplied {
            enabled: true,
            result: Ok(()),
        },
    )
    .state;
    let cue = Arc::new(SubtitleCue {
        text: "caption".into(),
        ..SubtitleCue::clear(0)
    });
    let shown = update(&enabled, Event::SubtitleObserved(Some(cue))).state;
    assert_eq!(shown.subtitle().map(|s| s.text.as_str()), Some("caption"));
    let stopped = update(&shown, Event::PlaybackStopped).state;
    assert!(stopped.subtitle().is_none());
    assert!(shown.subtitle().is_some());
    let late = update(&stopped, Event::SubtitleObserved(shown.subtitle().cloned()));
    assert!(late.state.subtitle().is_none());
    Ok(())
}
