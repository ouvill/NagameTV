use super::*;
use crate::channels::BroadcastService;
use crate::features::program_info::{guide::DayWindow, model::parse, view::View, watch};
use proptest::prelude::*;
const SERVICE: Option<BroadcastService> = Some(BroadcastService {
    network_id: 1,
    service_id: 1,
});
fn parse_programs(programs: serde_json::Value) -> super::super::model::Snapshot {
    parse(&serde_json::to_vec(&programs).unwrap()).unwrap()
}
fn entry(id: u64, start: u64, duration: u64) -> serde_json::Value {
    serde_json::json!({"id":id,"eventId":id,"networkId":1,"serviceId":1,"startAt":start,"duration":duration,"name":format!("P{id}")})
}
#[test]
fn nested_and_chained_overlaps_have_exact_nonoverlapping_segments() {
    let snapshot = parse_programs(serde_json::json!([
        entry(1, 100, 200),
        entry(2, 150, 50),
        entry(3, 250, 100)
    ]));
    let segments = snapshot.segments(SERVICE);
    assert_eq!(
        segments
            .iter()
            .map(|s| (s.start, s.end))
            .collect::<Vec<_>>(),
        [(100, 150), (150, 200), (200, 250), (250, 300), (300, 350)]
    );
    assert!(matches!(
        snapshot.resolution(SERVICE, 160),
        Resolution::Conflict(2)
    ));
    assert!(snapshot.current(SERVICE, 160).is_none());
    assert_eq!(snapshot.current(SERVICE, 200).unwrap().id, 1);
    assert_eq!(snapshot.current(SERVICE, 300).unwrap().id, 3);
    assert_eq!(snapshot.resolution(SERVICE, 350), Resolution::Gap);
}
#[test]
fn unknown_end_is_not_a_one_millisecond_program_or_a_known_next_boundary() {
    let snapshot = parse_programs(serde_json::json!([
        entry(1, 100, 1),
        entry(3, 150, 0),
        entry(2, 200, 100)
    ]));
    assert_eq!(snapshot.program(0).end(), End::Unknown);
    assert!(matches!(
        snapshot.resolution(SERVICE, 199),
        Resolution::Single(_)
    ));
    assert!(snapshot.current(SERVICE, 199).is_none());
    assert_eq!(snapshot.program(0).progress(199), 0.);
    assert_eq!(snapshot.current(SERVICE, 200).unwrap().id, 2);
    assert_eq!(snapshot.program(0).duration, 1, "wire value is preserved");
}
#[test]
fn duplicate_ids_with_different_metadata_are_retained_and_order_independent() {
    let a = entry(1, 100, 100);
    let mut b = a.clone();
    b["name"] = "revised".into();
    let first = parse_programs(serde_json::json!([a.clone(), b.clone(), a.clone()]));
    let second = parse_programs(serde_json::json!([b, a]));
    assert_eq!(first.len(), 2);
    assert_eq!(first.program(0), second.program(0));
    assert!(matches!(
        first.resolution(SERVICE, 150),
        Resolution::Conflict(2)
    ));
    assert!(first.exact(SERVICE.unwrap(), 1, 100).is_none());
}
#[test]
fn exact_ts_enrichment_does_not_use_the_current_program_winner() {
    let snapshot = parse_programs(serde_json::json!([entry(1, 100, 100), entry(2, 100, 200)]));
    assert!(snapshot.current(SERVICE, 100).is_none());
    assert_eq!(snapshot.exact(SERVICE.unwrap(), 1, 100).unwrap().id, 1);
}
#[test]
fn same_start_conflict_retains_candidates_and_channel_action_checks_server_generation() {
    let snapshot = parse_programs(serde_json::json!([entry(1, 100, 100), entry(2, 100, 200)]));
    let channels: std::sync::Arc<[_]> =
        crate::channels::parse(br#"[{"id":99,"networkId":1,"serviceId":1,"name":"A","type":1}]"#)
            .unwrap()
            .into();
    let view = View::new(
        snapshot.clone(),
        channels.clone(),
        DayWindow::new(120., 250.).unwrap(),
        5,
    )
    .unwrap();
    let cell = &view.cells()[0];
    assert_eq!((cell.begin, cell.end), (120, 200));
    assert_eq!(view.candidates(&cell.key).len(), 2);
    assert_eq!(view.action(&cell.key, 150), Some(watch::Action::Channel));
    assert!(watch::resolve(&snapshot, 6, &cell.key, &channels, 150).is_err());
    assert!(watch::resolve(&snapshot, 5, &cell.key, &channels, 200).is_err());
}
#[test]
fn many_simultaneous_candidates_do_not_materialize_quadratic_pairs() {
    let snapshot = parse_programs(serde_json::Value::Array(
        (0..10000).map(|i| entry(i, 100, 100)).collect(),
    ));
    assert_eq!(snapshot.segments(SERVICE).len(), 1);
    assert_eq!(
        snapshot.resolution(SERVICE, 150),
        Resolution::Conflict(10000)
    );
    assert_eq!(snapshot.candidates(SERVICE, 150).len(), 10000);
}
proptest! {
    #[test]
    fn sweep_matches_exhaustive_queries_and_is_invariant_under_input_reversal(entries in prop::collection::vec((0u64..1000, 2u64..200),0..80), now in 0u64..1200) {
        let input:Vec<_>=entries.iter().enumerate().map(|(i,&(start,duration))|entry(i as u64,start,duration)).collect();
        let first=parse_programs(input.clone().into());
        let reversed=parse_programs(input.into_iter().rev().collect::<Vec<_>>().into());
        let count=entries.iter().filter(|&&(start,duration)|start<=now&&now<start+duration).count();
        prop_assert_eq!(first.resolution(SERVICE,now),reversed.resolution(SERVICE,now));
        prop_assert_eq!(first.candidates(SERVICE,now).len(),count);
        match count {0=>prop_assert_eq!(first.resolution(SERVICE,now),Resolution::Gap),1=>prop_assert!(first.current(SERVICE,now).is_some()),n=>prop_assert_eq!(first.resolution(SERVICE,now),Resolution::Conflict(n))}
        prop_assert!(first.segments(SERVICE).windows(2).all(|s|s[0].end<=s[1].start));
        prop_assert!(first.segments(SERVICE).len()<=entries.len()*2);
    }
}
