use super::*;

const KEYWORD: &str = "検索 & + #";

fn row(id: u64, video: u64) -> Value {
    let mut row = record(id, video);
    row["channelId"] = Value::Null;
    row
}

fn response(rows: Vec<Value>, total: u64) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({"records": rows, "total": total}))
}

fn page(fixture: &Fixture, offset: u64, response: ResponseTemplate) {
    fixture.mount(
        Mock::given(method("GET"))
            .and(path("/api/recorded"))
            .and(query_param("keyword", KEYWORD))
            .and(query_param("offset", offset.to_string()))
            .and(query_param("limit", "50"))
            .and(query_param("isHalfWidth", "false"))
            .respond_with(response)
            .up_to_n_times(1)
            .expect(1),
    );
}

fn second_page(fixture: &Fixture) -> Result<Library, Box<dyn std::error::Error>> {
    page(fixture, 0, response(vec![row(1, 111)], 151));
    page(fixture, 50, response(vec![row(2, 222)], 151));
    let mut library = Library::default();
    assert!(!library.refresh(&fixture.network));
    library.open(
        &fixture.network,
        &fixture.server.uri(),
        KEYWORD,
        Login::Current,
    )?;
    assert!(!library.refresh(&fixture.network), "initial load is busy");
    fixture.finish(&mut library).ok_or("initial page")?;
    library.next(&fixture.network);
    fixture.finish(&mut library).ok_or("second page")?;
    Ok(library)
}

#[test]
fn chooser_opened_during_refresh_keeps_the_selected_recording_after_reordering() -> TestResult {
    let fixture = Fixture::new()?;
    let mut library = second_page(&fixture)?;
    let mut selected = row(2, 222);
    selected["videoFiles"] = json!([
        {"id":222,"type":"ts","size":1880},
        {"id":223,"type":"encoded","size":2048}
    ]);
    page(&fixture, 50, response(vec![selected.clone()], 151));
    assert!(library.refresh(&fixture.network));
    fixture.finish(&mut library).ok_or("multi-file recording")?;
    let expected = library.files(2);
    page(
        &fixture,
        50,
        response(vec![row(3, 333), selected], 151).set_delay(Duration::from_millis(60)),
    );
    assert!(library.refresh(&fixture.network));
    assert_eq!(library.select_recording(Some(2)), expected);
    assert_eq!(library.selected_files(), expected);
    fixture.finish(&mut library).ok_or("refreshed selection")?;
    assert_eq!(library.rows()[1].id, 2);
    assert_eq!(library.selected_files(), expected);
    fixture.runtime.block_on(fixture.server.verify());
    Ok(())
}

#[test]
fn chooser_reloads_changed_file_ids_names_and_order_from_the_refreshed_recording() -> TestResult {
    let fixture = Fixture::new()?;
    let mut library = second_page(&fixture)?;
    assert_eq!(library.select_recording(Some(2))[0].id, 222);
    let mut updated = row(2, 333);
    updated["videoFiles"] = json!([
        {"id":444,"type":"encoded","size":2048,"name":"New encode","filename":"new.mp4"},
        {"id":333,"type":"ts","size":1880,"name":"Original","filename":"new.ts"}
    ]);
    page(&fixture, 50, response(vec![updated], 151));
    assert!(library.refresh(&fixture.network));
    assert_eq!(library.selected_files()[0].id, 222);
    fixture.finish(&mut library).ok_or("changed files")?;
    let files = library.selected_files();
    assert_eq!(files.len(), 2);
    assert_eq!((files[0].id, files[0].filename.as_str()), (333, "new.ts"));
    assert_eq!(
        (
            files[1].id,
            files[1].name.as_str(),
            files[1].filename.as_str()
        ),
        (444, "New encode", "new.mp4")
    );
    assert!(library.video_url(2, 222).is_none());
    assert!(library.video_url(2, 333).is_some());
    fixture.runtime.block_on(fixture.server.verify());
    Ok(())
}

#[test]
fn removed_recording_clears_the_chooser_without_reopening_it_if_the_id_returns() -> TestResult {
    let fixture = Fixture::new()?;
    let mut library = second_page(&fixture)?;
    assert!(!library.select_recording(Some(2)).is_empty());
    for rows in [vec![row(3, 333)], vec![row(2, 444)]] {
        page(&fixture, 50, response(rows, 151));
        assert!(library.refresh(&fixture.network));
        fixture.finish(&mut library).ok_or("updated recordings")?;
        assert!(library.selected_files().is_empty());
    }
    assert_eq!(library.files(2)[0].id, 444);
    fixture.runtime.block_on(fixture.server.verify());
    Ok(())
}

#[test]
fn server_change_clears_the_chooser_even_when_recording_and_file_ids_match() -> TestResult {
    let fixture = Fixture::new()?;
    let mut library = second_page(&fixture)?;
    assert!(!library.select_recording(Some(2)).is_empty());
    let other = Fixture::new()?;
    page(&other, 0, response(vec![row(2, 222)], 1));
    library.open(&other.network, &other.server.uri(), KEYWORD, Login::Current)?;
    assert!(library.selected_files().is_empty());
    assert!(library.video_url(2, 222).is_none());
    other.finish(&mut library).ok_or("other server")?;
    assert!(!library.files(2).is_empty());
    assert!(library.selected_files().is_empty());
    fixture.runtime.block_on(fixture.server.verify());
    other.runtime.block_on(other.server.verify());
    Ok(())
}

#[test]
fn search_and_login_clear_selection_even_when_the_same_recording_returns() -> TestResult {
    let fixture = Fixture::new()?;
    let mut library = second_page(&fixture)?;
    fixture.mount(
        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"user":{"id":1}})))
            .expect(1),
    );
    fixture.mount(
        Mock::given(method("GET"))
            .and(path("/api/auth/media-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"token":"test-token"})))
            .expect(1),
    );
    for login in [
        Login::Current,
        Login::password("viewer".into(), "test secret".into())?,
    ] {
        assert!(!library.select_recording(Some(2)).is_empty());
        fixture.mount(
            Mock::given(method("GET"))
                .and(path("/api/recorded"))
                .and(query_param("keyword", "changed search"))
                .respond_with(response(vec![row(2, 222)], 1))
                .up_to_n_times(1)
                .expect(1),
        );
        library.open(
            &fixture.network,
            &fixture.server.uri(),
            "changed search",
            login,
        )?;
        assert!(library.selected_files().is_empty());
        fixture
            .finish(&mut library)
            .ok_or("replacement catalogue")?;
        assert!(!library.files(2).is_empty());
        assert!(library.selected_files().is_empty());
    }
    fixture.runtime.block_on(fixture.server.verify());
    Ok(())
}

#[test]
fn unavailable_files_clear_selection_until_the_recording_is_chosen_again() -> TestResult {
    let fixture = Fixture::new()?;
    let mut library = second_page(&fixture)?;
    for recording in [false, true] {
        assert!(!library.select_recording(Some(2)).is_empty());
        let mut unavailable = row(2, 222);
        unavailable["isRecording"] = json!(recording);
        if !recording {
            unavailable["videoFiles"] = json!([]);
        }
        for updated in [unavailable, row(2, 222)] {
            page(&fixture, 50, response(vec![updated], 151));
            assert!(library.refresh(&fixture.network));
            fixture.finish(&mut library).ok_or("updated availability")?;
            assert!(library.selected_files().is_empty());
        }
    }
    fixture.runtime.block_on(fixture.server.verify());
    Ok(())
}

#[test]
fn refresh_retains_query_and_usable_snapshot_then_replaces_catalogue() -> TestResult {
    let fixture = Fixture::new()?;
    let mut library = second_page(&fixture)?;
    let before = library.rows();
    let revision = library.revision();
    let mut updated = row(2, 333);
    updated["name"] = json!("Updated recording");
    updated["description"] = json!("Updated description");
    updated["thumbnails"] = json!([99]);
    page(
        &fixture,
        50,
        response(vec![updated, row(3, 444)], 152).set_delay(Duration::from_millis(60)),
    );
    assert!(library.refresh(&fixture.network));
    assert!(
        !library.refresh(&fixture.network),
        "do not duplicate a refresh"
    );
    assert!(library.busy() && library.loaded());
    assert_eq!(library.offset(), 50);
    assert_eq!(library.total(), 151);
    assert!(library.has_previous() && library.has_next());
    assert!(Arc::ptr_eq(&before, &library.rows()));
    assert_eq!(library.revision(), revision);
    assert_eq!(library.files(2)[0].id, 222);
    assert!(library.playback_request(2, Some(222)).is_some());
    fixture.finish(&mut library).ok_or("refreshed page")?;
    assert_eq!(library.offset(), 50);
    assert_eq!(library.total(), 152);
    assert_eq!(library.rows()[0].name, "Updated recording");
    assert_eq!(library.rows()[0].description, "Updated description");
    assert_eq!(library.rows()[0].thumbnail_id, Some(99));
    assert_eq!(library.files(2)[0].id, 333);
    assert!(library.playback_request(2, Some(222)).is_none());
    assert!(library.playback_request(2, Some(333)).is_some());
    assert!(library.playback_request(3, Some(444)).is_some());
    assert!(library.error().is_none());
    fixture.runtime.block_on(fixture.server.verify());
    Ok(())
}

#[test]
fn failed_refresh_retains_rows_page_files_and_can_be_retried() -> TestResult {
    let fixture = Fixture::new()?;
    let mut library = second_page(&fixture)?;
    let before = library.rows();
    let selected = library.select_recording(Some(2));
    for failure in [
        ResponseTemplate::new(503),
        ResponseTemplate::new(200).set_body_string("not JSON"),
    ] {
        page(&fixture, 50, failure);
        assert!(library.refresh(&fixture.network));
        assert!(library.error().is_none());
        assert!(fixture.finish(&mut library).is_none());
        assert!(library.error().is_some());
        assert!(library.loaded() && !library.busy());
        assert!(Arc::ptr_eq(&before, &library.rows()));
        assert_eq!(library.offset(), 50);
        assert_eq!(library.total(), 151);
        assert!(library.has_previous() && library.has_next());
        assert_eq!(library.files(2)[0].id, 222);
        assert_eq!(library.selected_files(), selected);
    }
    page(&fixture, 50, response(vec![row(4, 444)], 151));
    assert!(library.refresh(&fixture.network));
    assert!(library.error().is_none());
    fixture.finish(&mut library).ok_or("retry")?;
    assert_eq!(library.rows()[0].id, 4);
    fixture.runtime.block_on(fixture.server.verify());
    Ok(())
}

#[test]
fn deleted_last_page_is_corrected_before_publishing_and_empty_becomes_page_one() -> TestResult {
    let fixture = Fixture::new()?;
    let mut library = second_page(&fixture)?;
    page(&fixture, 50, response(vec![], 1));
    page(&fixture, 0, response(vec![row(8, 888)], 1));
    assert!(library.refresh(&fixture.network));
    fixture.finish(&mut library).ok_or("corrected page")?;
    assert_eq!(library.offset(), 0);
    assert_eq!(library.total(), 1);
    assert_eq!(library.rows()[0].id, 8);
    assert!(!library.has_previous() && !library.has_next());
    page(&fixture, 0, response(vec![], 0));
    assert!(library.refresh(&fixture.network));
    fixture.finish(&mut library).ok_or("empty catalogue")?;
    assert!(library.loaded() && library.rows().is_empty());
    assert_eq!(library.offset(), 0);
    assert_eq!(library.total(), 0);
    fixture.runtime.block_on(fixture.server.verify());
    Ok(())
}

#[test]
fn deleted_all_recordings_clamps_without_an_unnecessary_second_request() -> TestResult {
    let fixture = Fixture::new()?;
    let mut library = second_page(&fixture)?;
    page(&fixture, 50, response(vec![], 0));
    assert!(library.refresh(&fixture.network));
    fixture.finish(&mut library).ok_or("empty catalogue")?;
    assert!(library.loaded() && library.rows().is_empty());
    assert_eq!(library.offset(), 0);
    assert_eq!(library.total(), 0);
    assert_eq!(
        fixture
            .runtime
            .block_on(fixture.server.received_requests())
            .unwrap()
            .len(),
        3
    );
    Ok(())
}

#[test]
fn failed_last_page_correction_keeps_original_snapshot() -> TestResult {
    let fixture = Fixture::new()?;
    let mut library = second_page(&fixture)?;
    let before = library.rows();
    page(&fixture, 50, response(vec![], 1));
    page(&fixture, 0, ResponseTemplate::new(503));
    assert!(library.refresh(&fixture.network));
    assert!(fixture.finish(&mut library).is_none());
    assert!(library.error().is_some());
    assert!(Arc::ptr_eq(&before, &library.rows()));
    assert_eq!(library.total(), 151);
    assert_eq!(library.offset(), 50);
    fixture.runtime.block_on(fixture.server.verify());
    Ok(())
}

#[test]
fn ongoing_deletions_bound_correction_requests_and_keep_the_old_page() -> TestResult {
    let fixture = Fixture::new()?;
    let mut library = second_page(&fixture)?;
    page(&fixture, 100, response(vec![row(3, 333)], 151));
    library.next(&fixture.network);
    fixture.finish(&mut library).ok_or("third page")?;
    let before = library.rows();
    page(&fixture, 100, response(vec![], 51));
    page(&fixture, 50, response(vec![], 1));
    assert!(library.refresh(&fixture.network));
    assert!(fixture.finish(&mut library).is_none());
    assert!(matches!(
        library.error(),
        Some(FetchError::Parse(Error::InvalidCatalogue(_)))
    ));
    assert!(Arc::ptr_eq(&before, &library.rows()));
    assert_eq!(library.offset(), 100);
    fixture.runtime.block_on(fixture.server.verify());
    Ok(())
}

#[test]
fn navigation_retires_refresh_without_publishing_its_late_response() -> TestResult {
    let fixture = Fixture::new()?;
    let mut library = second_page(&fixture)?;
    assert!(!library.select_recording(Some(2)).is_empty());
    page(
        &fixture,
        50,
        response(vec![row(99, 999)], 151).set_delay(Duration::from_millis(100)),
    );
    page(&fixture, 100, response(vec![row(3, 333)], 151));
    assert!(library.refresh(&fixture.network));
    let deadline = Instant::now() + Duration::from_secs(5);
    while fixture
        .runtime
        .block_on(fixture.server.received_requests())
        .unwrap()
        .len()
        < 3
    {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    library.next(&fixture.network);
    assert!(library.selected_files().is_empty());
    assert!(!library.refresh(&fixture.network), "cancellation is busy");
    fixture.finish(&mut library).ok_or("next page")?;
    assert_eq!(library.offset(), 100);
    assert_eq!(library.rows()[0].id, 3);
    assert!(library.files(99).is_empty());
    assert!(library.selected_files().is_empty());
    fixture.runtime.block_on(fixture.server.verify());
    Ok(())
}

#[test]
fn refresh_invalidates_reused_thumbnail_ids_only_after_success() -> TestResult {
    let fixture = Fixture::new()?;
    let mut row = row(1, 111);
    row["thumbnails"] = json!([99]);
    page(&fixture, 0, response(vec![row.clone()], 1));
    let mut library = Library::default();
    library.open(
        &fixture.network,
        &fixture.server.uri(),
        KEYWORD,
        Login::Current,
    )?;
    fixture.finish(&mut library).ok_or("initial page")?;
    let mut previous = String::new();
    for image in [
        vec![0xff, 0xd8, 0xff, 0xd9],
        vec![0xff, 0xd8, 0xff, 0xfe, 0, 3, 1, 0xff, 0xd9],
    ] {
        fixture.mount(
            Mock::given(path("/api/thumbnails/99"))
                .respond_with(ResponseTemplate::new(200).set_body_raw(image, "image/jpeg"))
                .up_to_n_times(1)
                .expect(1),
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while library
            .thumbnail(&fixture.server.uri(), Some(99))
            .is_empty()
        {
            library.poll_thumbnails(&fixture.network, vec![]);
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        }
        let current = library
            .thumbnail(&fixture.server.uri(), Some(99))
            .to_owned();
        assert_ne!(current, previous);
        if !previous.is_empty() {
            break;
        }
        previous = current;
        page(&fixture, 0, ResponseTemplate::new(503));
        assert!(library.refresh(&fixture.network));
        fixture.finish(&mut library);
        assert_eq!(library.thumbnail(&fixture.server.uri(), Some(99)), previous);
        page(&fixture, 0, response(vec![row.clone()], 1));
        assert!(library.refresh(&fixture.network));
        assert_eq!(library.thumbnail(&fixture.server.uri(), Some(99)), previous);
        fixture.finish(&mut library).ok_or("refresh")?;
        assert!(
            library
                .thumbnail(&fixture.server.uri(), Some(99))
                .is_empty()
        );
    }
    fixture.runtime.block_on(fixture.server.verify());
    Ok(())
}
