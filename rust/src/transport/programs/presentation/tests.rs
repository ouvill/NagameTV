use super::*;

fn program() -> Program {
    Program {
        event_id: 1,
        service_id: 2,
        network_id: 3,
        transport_stream_id: 4,
        start_at: Some(1_800_000_000_000),
        duration: Some(60_000),
        name: "番組\"\\\n".into(),
        description: "説明".into(),
        extended: "補足".into(),
        genres: Vec::new(),
        audios: Box::default(),
    }
}

fn project(cache: &mut Cache, program: Program) -> Data {
    cache.project(program, "局".into(), "提供".into(), Some((0, 60_000)), true)
}

#[test]
fn corrections_and_clock_changes_invalidate_only_the_affected_body() {
    let mut cache = Cache::default();
    let original = project(&mut cache, program());
    assert!(original.same_body(&project(&mut cache, program())));
    let mut correction = program();
    correction.description = "訂正".into();
    let corrected = project(&mut cache, correction);
    assert!(!original.same_body(&corrected));
    assert_eq!(corrected.0.value["description"], "訂正\n\n補足");
    let shifted = cache.project(
        program(),
        "局".into(),
        "提供".into(),
        Some((1, 60_001)),
        true,
    );
    assert!(!original.same_body(&shifted));
    assert_eq!(shifted.0.value["playbackStartMs"], 1);
    assert!(original.same_body(&project(&mut cache, program())));
    let renamed = cache.project(
        program(),
        "新局名".into(),
        "提供".into(),
        Some((0, 60_000)),
        true,
    );
    assert!(!original.same_body(&renamed));
    assert_eq!(renamed.0.value["station"], "新局名");
    assert_eq!(cache.bodies.len(), MAX_PROGRAM_BODIES);
    assert!(
        !original.same_body(&project(&mut cache, program())),
        "evicted body is rebuilt"
    );
}

#[test]
fn cached_json_keeps_nested_objects_and_escaping() {
    let body = project(&mut Cache::default(), program());
    let encoded = serde_json::to_string(&serde_json::json!({"data": body})).unwrap();
    let decoded: Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded["data"]["name"], program().name);
    assert_eq!(decoded["data"]["description"], "説明\n\n補足");
    assert_eq!(decoded["data"]["source"], "broadcast_ts");
    assert_eq!(decoded["data"]["progressKnown"], true);
}

#[test]
fn epg_enrichment_is_reused_until_its_revision_changes() {
    let original = project(&mut Cache::default(), program());
    let mut cache = Enrichment::default();
    let first = cache.apply(&original, 1, |value| value["name"] = "EPG補完".into());
    let same = cache.apply(&original, 1, |_| {
        panic!("unchanged EPG must not be applied twice")
    });
    assert!(first.same_body(&same));
    let corrected = cache.apply(&original, 2, |value| value["name"] = "訂正".into());
    assert_eq!(corrected.title(), "訂正");
    assert_eq!(original.title(), program().name);
    assert!(original.same_body(&cache.apply(&original, 3, |_| {})));
    cache.apply(&original, 4, |_| {});
    assert_eq!(cache.bodies.len(), MAX_PROGRAM_BODIES);
}

#[test]
fn publication_clears_initial_and_previous_data_without_repeating_notifications() {
    let mut publication = Publication::default();
    assert!(
        publication.update(None),
        "first empty snapshot clears a prior source's UI"
    );
    assert!(!publication.update(None));
    let body = project(&mut Cache::default(), program());
    assert!(publication.update(Some(body.clone())));
    assert!(!publication.update(Some(body)));
    assert!(
        !publication.update(Some(project(&mut Cache::default(), program()))),
        "equal rebuilt body"
    );
    assert!(publication.update(None));
    assert_eq!(publication.json(), "null");
    assert!(!publication.update(None));
}
