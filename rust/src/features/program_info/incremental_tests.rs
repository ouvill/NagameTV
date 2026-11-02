use super::*;
use serde_json::{Value, json};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct Fixture {
    server: MockServer,
    network: Network,
    runtime: tokio::runtime::Runtime,
}
impl Fixture {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let runtime = tokio::runtime::Runtime::new()?;
        let server = runtime.block_on(MockServer::start());
        Ok(Self {
            server,
            network: Network::new()?,
            runtime,
        })
    }
    fn mount(&self, mock: Mock) {
        self.runtime.block_on(mock.mount(&self.server));
    }
    fn load(&self) -> ProgramInfo {
        let mut feature = ProgramInfo::default();
        feature.configure(Some(self.server.uri()));
        assert!(feature.poll(&self.network).started);
        assert_eq!(
            finish(&mut feature, &self.network, Instant::now()).completed,
            Some(Completion::Succeeded)
        );
        feature
    }
    fn verify(&self) {
        self.runtime.block_on(self.server.verify());
    }
}
fn finish(feature: &mut ProgramInfo, network: &Network, now: Instant) -> Update {
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        let update = feature.poll_at(network, now);
        if update.completed.is_some() {
            return update;
        }
        assert!(Instant::now() < deadline, "EPG operation did not finish");
        thread::sleep(Duration::from_millis(1));
    }
}
fn program(id: u64, service: u16, name: &str) -> Value {
    json!({"id":id,"networkId":10,"serviceId":service,"eventId":id,"startAt":100,"duration":500,"name":name})
}
fn channels() -> Vec<crate::channels::Channel> {
    crate::channels::parse(
        br#"[
        {"id":9000000001,"name":"A","type":1,"networkId":10,"serviceId":1},
        {"id":9000000002,"name":"B","type":1,"networkId":10,"serviceId":2}
    ]"#,
    )
    .unwrap()
}
fn change(kind: &str, data: Value) -> Changes {
    let mut changes = Changes::default();
    viewer_epg_events::Decoder::default()
        .push_changes(
            &serde_json::to_vec(&json!({"resource":"program","type":kind,"data":data})).unwrap(),
            &mut changes,
        )
        .unwrap();
    changes
}
fn full_mock(body: Value) -> Mock {
    Mock::given(method("GET"))
        .and(path("/api/programs"))
        .and(|request: &wiremock::Request| request.url.query().is_none())
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
}
fn name(feature: &ProgramInfo, service: u16) -> Option<&str> {
    feature
        .snapshot
        .current(
            Some(BroadcastService {
                network_id: 10,
                service_id: service,
            }),
            100,
        )?
        .name
        .as_deref()
}

#[test]
fn identical_updates_change_neither_revision_nor_projection_and_never_refetch() -> TestResult {
    let fixture = Fixture::new()?;
    let entry = program(1, 1, "original");
    fixture.mount(full_mock(json!([entry])).expect(1));
    let mut feature = fixture.load();
    let revision = feature.revision;
    let mut projection = presentation::Projection::default();
    let service = Some(BroadcastService {
        network_id: 10,
        service_id: 1,
    });
    feature.current_presentation(&mut projection, service, 100)?;
    for _ in 0..211 {
        assert!(!feature.events(change("update", entry.clone()), &channels()));
        assert_eq!(feature.revision, revision);
        assert!(!feature.poll(&fixture.network).started);
    }
    let update = feature.current_presentation(&mut projection, service, 100)?;
    assert!(update.data.is_none());
    feature.events(change("create", program(2, 2, "created")), &channels());
    assert_eq!(feature.snapshot.len(), 2);
    feature.events(change("update", program(1, 1, "changed")), &channels());
    assert_eq!(name(&feature, 1), Some("changed"));
    feature.events(change("remove", json!({"id":2})), &channels());
    assert_eq!(feature.snapshot.len(), 1);
    let revision = feature.revision;
    feature.events(change("remove", json!({"id":2})), &channels());
    assert_eq!(feature.revision, revision);
    fixture.verify();
    Ok(())
}

#[test]
fn service_notifications_replace_only_the_target_and_support_both_endpoints() -> TestResult {
    for query_fallback in [false, true] {
        let fixture = Fixture::new()?;
        fixture.mount(full_mock(json!([program(1, 1, "old"), program(2, 2, "keep")])).expect(1));
        let response = json!([program(3, 1, "new")]);
        fixture.mount(
            Mock::given(path("/api/services/9000000001/programs"))
                .respond_with(if query_fallback {
                    ResponseTemplate::new(404)
                } else {
                    ResponseTemplate::new(200).set_body_json(response.clone())
                })
                .expect(1),
        );
        if query_fallback {
            fixture.mount(
                Mock::given(path("/api/programs"))
                    .and(wiremock::matchers::query_param("networkId", "10"))
                    .and(wiremock::matchers::query_param("serviceId", "1"))
                    .respond_with(ResponseTemplate::new(200).set_body_json(response))
                    .expect(1),
            );
        }
        let mut feature = fixture.load();
        let mut decoder = viewer_epg_events::sse::Decoder::default();
        let mut changes = Changes::default();
        for _ in 0..200 {
            decoder.push_changes(
                b"event: epg.programs-updated\ndata: {\"serviceId\":9000000001}\n\n",
                &mut changes,
            )?;
        }
        assert!(!feature.events(changes, &channels()));
        assert!(feature.poll(&fixture.network).started);
        assert_eq!(
            finish(&mut feature, &fixture.network, Instant::now()).completed,
            Some(Completion::Succeeded)
        );
        assert_eq!(name(&feature, 1), Some("new"));
        assert_eq!(name(&feature, 2), Some("keep"));
        assert_eq!(feature.snapshot.len(), 2);
        assert!(!feature.poll(&fixture.network).started);
        fixture.verify();
    }
    Ok(())
}

#[test]
fn events_received_during_fetch_override_the_older_http_snapshot() -> TestResult {
    let fixture = Fixture::new()?;
    let count = Arc::new(AtomicUsize::new(0));
    let requests = count.clone();
    fixture.mount(
        Mock::given(path("/api/programs"))
            .respond_with(move |_: &wiremock::Request| {
                let previous = requests.fetch_add(1, Ordering::SeqCst);
                ResponseTemplate::new(200)
                    .set_body_json(json!([program(1, 1, "old")]))
                    .set_delay(if previous == 0 {
                        Duration::ZERO
                    } else {
                        Duration::from_millis(40)
                    })
            })
            .expect(2),
    );
    let mut feature = fixture.load();
    feature.refresh();
    assert!(feature.poll(&fixture.network).started);
    feature.events(change("update", program(1, 1, "during fetch")), &channels());
    feature.events(
        change("create", program(2, 2, "new service program")),
        &channels(),
    );
    assert_eq!(name(&feature, 1), Some("old"));
    finish(&mut feature, &fixture.network, Instant::now());
    assert_eq!(name(&feature, 1), Some("during fetch"));
    assert_eq!(name(&feature, 2), Some("new service program"));
    assert!(!feature.poll(&fixture.network).started);
    fixture.verify();
    Ok(())
}

#[test]
fn failures_back_off_despite_pending_refresh_and_preserve_the_previous_snapshot() -> TestResult {
    let fixture = Fixture::new()?;
    let count = Arc::new(AtomicUsize::new(0));
    let requests = count.clone();
    fixture.mount(
        Mock::given(path("/api/programs"))
            .respond_with(move |_: &wiremock::Request| {
                match requests.fetch_add(1, Ordering::SeqCst) {
                    0 | 7 => {
                        ResponseTemplate::new(200).set_body_json(json!([program(1, 1, "retained")]))
                    }
                    _ => ResponseTemplate::new(503),
                }
            })
            .expect(9),
    );
    let mut feature = fixture.load();
    let mut now = Instant::now();
    feature.refresh();
    assert!(feature.poll_at(&fixture.network, now).started);
    for (index, seconds) in [5, 10, 20, 40, 60, 60].into_iter().enumerate() {
        for _ in 0..100 {
            feature.refresh();
        }
        let update = finish(&mut feature, &fixture.network, now);
        assert_eq!(update.completed, Some(Completion::Failed));
        assert!(!update.started);
        assert_eq!(name(&feature, 1), Some("retained"));
        feature.refresh();
        assert!(
            !feature
                .poll_at(
                    &fixture.network,
                    now + Duration::from_secs(seconds) - Duration::from_nanos(1)
                )
                .started
        );
        assert_eq!(count.load(Ordering::SeqCst), index + 2);
        now += Duration::from_secs(seconds);
        assert!(feature.poll_at(&fixture.network, now).started);
    }
    assert_eq!(
        finish(&mut feature, &fixture.network, now).completed,
        Some(Completion::Succeeded)
    );
    feature.refresh();
    assert!(feature.poll_at(&fixture.network, now).started);
    assert_eq!(
        finish(&mut feature, &fixture.network, now).completed,
        Some(Completion::Failed)
    );
    assert!(
        matches!(feature.acquisition,Acquisition::Idle { outcome:Outcome::Failed {retry_at,..},.. } if retry_at==now+RETRY_INITIAL)
    );
    fixture.verify();
    Ok(())
}

#[test]
fn incremental_normalization_recomputes_shared_metadata_and_drops_unplaceable_updates() -> TestResult
{
    let source = json!({"id":1,"networkId":10,"serviceId":1,"eventId":1,"startAt":100,"duration":500,"name":"source", "series":{"name":"","episode":0,"lastEpisode":0}});
    let shared = json!({"id":2,"networkId":10,"serviceId":2,"eventId":2,"startAt":100,"duration":500,"relatedItems":[{"type":"shared","serviceId":1,"eventId":1}]});
    let fixture = Fixture::new()?;
    fixture.mount(full_mock(json!([source, shared])).expect(1));
    let mut feature = fixture.load();
    assert_eq!(name(&feature, 2), Some("source"));
    let revision = feature.revision;
    feature.events(change("update", source.clone()), &channels());
    assert_eq!(feature.revision, revision);
    feature.events(change("update", program(1, 1, "replacement")), &channels());
    assert_eq!(name(&feature, 2), Some("replacement"));
    feature.events(change("remove", json!({"id":1})), &channels());
    assert_eq!(name(&feature, 2), None);
    feature.events(change("create", source), &channels());
    assert_eq!(name(&feature, 2), Some("source"));
    let mut unknown = program(1, 1, "no time");
    unknown["startAt"] = Value::Null;
    feature.events(change("update", unknown), &channels());
    assert_eq!(feature.snapshot.len(), 1);
    assert_eq!(name(&feature, 2), None);
    assert!(feature.snapshot.storage().source > 0);
    fixture.verify();
    Ok(())
}

#[test]
fn unknown_services_request_catalog_and_full_reconciliation() -> TestResult {
    let fixture = Fixture::new()?;
    fixture.mount(full_mock(json!([])).expect(2));
    let mut feature = fixture.load();
    let mut changes = Changes::default();
    changes.service(99);
    assert!(feature.events(changes, &channels()));
    assert!(feature.poll(&fixture.network).started);
    finish(&mut feature, &fixture.network, Instant::now());
    fixture.verify();
    Ok(())
}

#[test]
fn invalid_event_and_foreign_service_response_preserve_valid_data() -> TestResult {
    let fixture = Fixture::new()?;
    fixture.mount(full_mock(json!([program(1, 1, "keep")])).expect(2));
    fixture.mount(
        Mock::given(path("/api/services/9000000001/programs"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!([program(2, 2, "foreign")])),
            )
            .expect(1),
    );
    let mut feature = fixture.load();
    let revision = feature.revision;
    let mut invalid = program(1, 1, "invalid");
    invalid["startAt"] = json!("bad");
    feature.events(change("update", invalid), &channels());
    assert_eq!(feature.revision, revision);
    assert_eq!(name(&feature, 1), Some("keep"));
    assert!(feature.poll(&fixture.network).started);
    finish(&mut feature, &fixture.network, Instant::now());
    let mut changes = Changes::default();
    changes.service(9_000_000_001);
    feature.events(changes, &channels());
    assert!(feature.poll(&fixture.network).started);
    assert_eq!(
        finish(&mut feature, &fixture.network, Instant::now()).completed,
        Some(Completion::Failed)
    );
    assert_eq!(name(&feature, 1), Some("keep"));
    fixture.verify();
    Ok(())
}

#[test]
fn backlog_overflow_during_fetch_requests_one_full_followup_and_disable_discards_it() -> TestResult
{
    let fixture = Fixture::new()?;
    fixture.mount(
        Mock::given(path("/api/programs"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!([]))
                    .set_delay(Duration::from_millis(40)),
            )
            .expect(2),
    );
    let mut feature = ProgramInfo::default();
    feature.configure(Some(fixture.server.uri()));
    assert!(feature.poll(&fixture.network).started);
    for id in 0..1500 {
        feature.events(change("remove", json!({"id":id})), &channels());
    }
    assert!(feature.refresh_pending);
    let first = finish(&mut feature, &fixture.network, Instant::now());
    assert!(first.started);
    let second = finish(&mut feature, &fixture.network, Instant::now());
    assert!(!second.started);
    assert!(!feature.poll(&fixture.network).started);
    feature.configure(None);
    feature.events(
        change("create", program(1, 1, "old generation")),
        &channels(),
    );
    assert_eq!(feature.snapshot.len(), 0);
    fixture.verify();
    Ok(())
}

#[test]
fn cumulative_program_limit_and_shared_expansion_limit_are_enforced() -> TestResult {
    let entries: Vec<_> = (0..MAX_PROGRAMS)
        .map(|id| program(id as u64, 1, "a"))
        .collect();
    let snapshot = parse(&serde_json::to_vec(&entries)?)?;
    let Changes::Delta(delta) = change("create", program(MAX_PROGRAMS as u64, 1, "one too many"))
    else {
        unreachable!()
    };
    assert!(matches!(
        snapshot.apply(delta.into_parts().0),
        Err(Error::TooManyPrograms { .. })
    ));
    let mut source = program(1, 1, "source");
    source["description"] = json!("x".repeat(256 * 1024));
    let mut entries = vec![source];
    for id in 2..602 {
        entries.push(
            json!({"id":id,"eventId":id,"networkId":10,"serviceId":id,"startAt":100,"duration":500,
            "relatedItems":[{"type":"shared","serviceId":1,"eventId":1}]}),
        );
    }
    assert!(matches!(
        parse(&serde_json::to_vec(&entries)?),
        Err(Error::TooMuchMemory { .. })
    ));
    Ok(())
}

#[test]
fn foreign_service_with_unknown_time_is_not_mistaken_for_empty_service() -> TestResult {
    let bytes = serde_json::to_vec(&json!([{"id":1,"networkId":10,"serviceId":2,"startAt":null}]))?;
    assert!(matches!(
        model::parse_service(
            &bytes,
            BroadcastService {
                network_id: 10,
                service_id: 1
            }
        ),
        Err(Error::WrongService)
    ));
    Ok(())
}

proptest::proptest! {
    #[test]
    fn incremental_operations_agree_with_rebuilding_the_authoritative_entries(
        operations in proptest::collection::vec((1_u64..8, proptest::bool::ANY, 0_u64..200),1..60)
    ) {
        let mut records=BTreeMap::new();
        let mut snapshot=parse(b"[]").unwrap();
        for (id,remove,duration) in operations {
            let change=if remove {
                records.remove(&id);
                change("remove",json!({"id":id}))
            } else {
                let mut entry=program(id,1,&format!("duration {duration}"));
                entry["duration"]=duration.into();
                entry["startAt"]=(id*100).into();
                records.insert(id,entry.clone());
                change("update",entry)
            };
            let Changes::Delta(delta)=change else { unreachable!() };
            if let Some(updated)=snapshot.apply(delta.into_parts().0).unwrap() { snapshot=updated; }
            let rebuilt=parse(&serde_json::to_vec(&records.values().collect::<Vec<_>>()).unwrap()).unwrap();
            proptest::prop_assert!(snapshot.same_programs(&rebuilt));
        }
    }
}

#[test]
fn service_notification_can_recover_an_empty_catalog_without_starting_epg_early() {
    let mut feature = ProgramInfo::default();
    let mut changes = Changes::default();
    changes.service(9_000_000_001);
    assert!(feature.events(changes, &[]));
    assert!(matches!(feature.status(), Status::Disabled));
    assert_eq!(feature.snapshot.len(), 0);
    assert_eq!(feature.revision, 0);
    assert!(!feature.refresh_pending);
}
