use super::*;

impl Runtime {
    /// Explicit failure-path fixture: no device, socket, file or Qt initialization.
    pub(crate) fn unavailable_for_test(state: State) -> Self {
        let (epg_tx, epg_rx) = mpsc::sync_channel(1);
        let (comment_tx, comment_rx) = mpsc::sync_channel(256);
        Self {
            state,
            playback: None,
            network: None,
            catalog_request: Default::default(),
            catalog_id: 0,
            epg_task: None,
            epg_tx,
            epg_rx,
            comment_task: None,
            comment_generation: Arc::new(AtomicU64::new(0)),
            comment_tx,
            comment_rx,
            presentation: vec![],
            log_directory: None,
            recorder: None,
            ui: Default::default(),
        }
    }
}

#[test]
fn playback_effect_failure_is_reduced_before_dispatch_returns() {
    let mut runtime = Runtime::unavailable_for_test(State::new(
        viewer_core::settings::Settings {
            service_id: "101".into(),
            ..Default::default()
        },
        "en".into(),
        false,
    ));
    runtime.dispatch(Event::Play);
    assert!(
        matches!(runtime.state().playback(), app::PlaybackState::Failed(error) if error.summary == "Could not initialize the player")
    );
    assert!(!runtime.state().playback().active());
    assert!(runtime.state().subtitle().is_none());
}

#[test]
fn catalog_effect_failure_completes_the_pending_request() {
    let mut runtime =
        Runtime::unavailable_for_test(State::new(Default::default(), "en".into(), false));
    runtime.dispatch(Event::Refresh { force: true });
    assert!(!runtime.state().loading());
    assert_eq!(runtime.state().status(), "Network runtime is unavailable");
}

#[test]
fn translation_is_deferred_to_presentation_without_mutating_preferences() {
    let mut runtime =
        Runtime::unavailable_for_test(State::new(Default::default(), "en".into(), false));
    runtime.dispatch(Event::ChangeLanguage("ja".into()));
    assert_eq!(runtime.state().ui_language(), "en");
    assert!(
        matches!(runtime.take_presentation().as_slice(), [Presentation::ApplyLanguage(language)] if language == "ja")
    );
    assert!(runtime.take_presentation().is_empty());
    assert!(!runtime.request(Event::LanguageApplied {
        preference: "ja".into(),
        result: Err("missing translation".into())
    }));
    assert_eq!(runtime.state().ui_language(), "en");
}
