use super::*;

const ID: u64 = 7;

fn catalogue(fixture: &Fixture, library: &mut Library) -> TestResult {
    fixture.mount(
        Mock::given(method("GET"))
            .and(path("/tv/api/recorded"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "records": [record(ID,123), record(8,124)], "total": 2
            }))),
    );
    library.open(
        &fixture.network,
        &format!("{}/tv", fixture.server.uri()),
        "",
        Login::Current,
    )?;
    fixture.finish(library);
    assert!(library.loaded());
    Ok(())
}

fn deletion_response(fixture: &Fixture, current: Value, response: ResponseTemplate, writes: u64) {
    fixture.mount(
        Mock::given(method("GET"))
            .and(path("/tv/api/recorded/7"))
            .and(query_param("isHalfWidth", "false"))
            .respond_with(ResponseTemplate::new(200).set_body_json(current))
            .expect(1),
    );
    fixture.mount(
        Mock::given(method("DELETE"))
            .and(path("/tv/api/recorded/7"))
            .respond_with(response)
            .expect(writes),
    );
}

fn finish(fixture: &Fixture, library: &mut Library) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while library.deleting() {
        library.poll(&fixture.network);
        assert!(Instant::now() < deadline, "deletion did not complete");
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn delete_requires_confirmation_and_applies_the_acknowledged_change_once() -> TestResult {
    let fixture = Fixture::new()?;
    let mut library = Library::with_deletion(true);
    catalogue(&fixture, &mut library)?;
    deletion_response(
        &fixture,
        record(ID, 123),
        ResponseTemplate::new(200).set_body_json(json!({"code":200})),
        1,
    );
    assert!(!library.confirm_deletion(Some(&fixture.network), &DeletePlayback::Idle));
    assert!(library.prepare_deletion(ID, &DeletePlayback::Idle));
    assert_eq!(library.deletion_state(), "confirming");
    assert!(!library.refresh(&fixture.network));
    assert!(!library.prepare_deletion(8, &DeletePlayback::Idle));
    assert!(library.dismiss_deletion());
    assert!(!library.confirm_deletion(Some(&fixture.network), &DeletePlayback::Idle));
    assert!(library.prepare_deletion(ID, &DeletePlayback::Idle));
    assert!(library.confirm_deletion(Some(&fixture.network), &DeletePlayback::Idle));
    assert!(!library.confirm_deletion(Some(&fixture.network), &DeletePlayback::Idle));
    assert!(!library.dismiss_deletion());
    assert!(matches!(
        library.open(&fixture.network, &fixture.server.uri(), "", Login::Current),
        Err(Error::DeletionPending)
    ));
    finish(&fixture, &mut library);
    assert_eq!(library.deletion_state(), "deleted");
    assert_eq!(
        library.rows().iter().map(|row| row.id).collect::<Vec<_>>(),
        [8]
    );
    assert_eq!(library.total(), 1);
    Ok(())
}

#[test]
fn delete_feature_defaults_off_and_rechecks_active_playback_at_confirmation() -> TestResult {
    let fixture = Fixture::new()?;
    let mut disabled = Library::default();
    catalogue(&fixture, &mut disabled)?;
    assert!(!disabled.prepare_deletion(ID, &DeletePlayback::Idle));
    let mut library = Library::with_deletion(true);
    catalogue(&fixture, &mut library)?;
    let playback = DeletePlayback::Open(crate::viewing_history::recordings::Source::Epgstation {
        server: format!("{}/tv", fixture.server.uri()),
        recorded_id: ID,
        video_file_id: 124,
    });
    assert!(library.prepare_deletion(ID, &playback));
    assert!(matches!(
        library.deletion_error(),
        Some(DeleteError::Playback)
    ));
    assert!(library.prepare_deletion(ID, &DeletePlayback::Idle));
    assert!(library.confirm_deletion(Some(&fixture.network), &playback));
    assert!(matches!(
        library.deletion_error(),
        Some(DeleteError::Playback)
    ));
    assert_eq!(library.total(), 2);
    let requests = fixture
        .runtime
        .block_on(fixture.server.received_requests())
        .unwrap();
    assert!(
        requests
            .iter()
            .all(|request| request.url.path() != "/tv/api/recorded/7")
    );
    Ok(())
}

#[test]
fn delete_refuses_changed_protected_or_in_progress_recordings() -> TestResult {
    for (key, value) in [
        ("startAt", json!(1001)),
        ("isProtected", json!(true)),
        ("isRecording", json!(true)),
    ] {
        let fixture = Fixture::new()?;
        let mut library = Library::with_deletion(true);
        catalogue(&fixture, &mut library)?;
        let mut current = record(ID, 123);
        current[key] = value;
        deletion_response(&fixture, current, ResponseTemplate::new(500), 0);
        assert!(library.prepare_deletion(ID, &DeletePlayback::Idle));
        assert!(library.confirm_deletion(Some(&fixture.network), &DeletePlayback::Idle));
        finish(&fixture, &mut library);
        assert_eq!(library.deletion_state(), "failed");
        assert!(match key {
            "startAt" => matches!(library.deletion_error(), Some(DeleteError::Changed)),
            "isProtected" => matches!(library.deletion_error(), Some(DeleteError::Protected)),
            _ => matches!(library.deletion_error(), Some(DeleteError::Recording)),
        });
        assert_eq!(library.total(), 2);
    }
    Ok(())
}

#[test]
fn delete_keeps_the_item_when_the_provider_rejects_or_does_not_acknowledge() -> TestResult {
    for response in [
        ResponseTemplate::new(403),
        ResponseTemplate::new(200).set_body_json(json!({"code":500})),
    ] {
        let fixture = Fixture::new()?;
        let mut library = Library::with_deletion(true);
        catalogue(&fixture, &mut library)?;
        deletion_response(&fixture, record(ID, 123), response, 1);
        assert!(library.prepare_deletion(ID, &DeletePlayback::Idle));
        assert!(library.confirm_deletion(Some(&fixture.network), &DeletePlayback::Idle));
        finish(&fixture, &mut library);
        assert!(matches!(
            library.deletion_error(),
            Some(DeleteError::Request(_))
        ));
        assert_eq!(library.rows()[0].id, ID);
        assert_eq!(library.total(), 2);
    }
    Ok(())
}
