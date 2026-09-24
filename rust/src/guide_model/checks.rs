//! Hardware-free production model checks, including Qt's structural validator.
use super::*;
use crate::features::program_info::{guide::DayWindow, model::parse};
use cxx_qt_lib::{QMap, QMapPair_QString_QVariant};
fn field(row: QVariant, key: &str) -> QVariant {
    row.value::<QMap<QMapPair_QString_QVariant>>()
        .unwrap()
        .get(&QString::from(key))
        .unwrap()
        .clone()
}
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let channels: std::sync::Arc<[_]> = crate::channels::parse(
        br#"[{"id":18446744073709551615,"networkId":1,"serviceId":1,"name":"A","type":1}]"#,
    )?
    .into();
    let snapshot = parse(
        br#"[
        {"id":1,"networkId":1,"serviceId":1,"startAt":100,"duration":100,"name":"A"},
        {"id":2,"networkId":1,"serviceId":1,"startAt":150,"duration":100,"name":"B"}
    ]"#,
    )?;
    let mut model = ffi::make_guide_model();
    let mut filter = ffi::make_filter();
    ffi::check_guide_model(model.pin_mut());
    ffi::check_filter(filter.pin_mut());
    ffi::check_candidates(model.pin_mut().rust_mut().candidates.pin_mut());
    ffi::set_source(filter.pin_mut(), model.pin_mut());
    model.pin_mut().replace(View::new(
        snapshot,
        channels,
        DayWindow::new(0., 1000.)?,
        7,
    )?);
    assert_eq!(model.count(), 3);
    let row = model.row(1);
    assert_eq!(
        field(row.clone(), "scheduleState").value::<i32>(),
        Some(ffi::ScheduleState::Conflict.repr)
    );
    let key = field(row, "watchKey").value::<QString>().unwrap();
    assert!(key.to_string().contains("18446744073709551615"));
    assert_eq!(
        model.action(key.clone(), 160.),
        ffi::WatchAction::WatchChannel.repr
    );
    model.pin_mut().select_candidates(key.clone());
    assert_eq!(model.rust().candidates.count(), 2);
    assert_eq!(
        field(model.details(key.clone(), 1), "name")
            .value::<QString>()
            .unwrap()
            .to_string(),
        "B"
    );
    assert_eq!(
        field(model.nearest(0, 160.), "watchKey").value::<QString>(),
        Some(key.clone())
    );
    model.pin_mut().replace(View::default());
    assert_eq!(model.count(), 0);
    assert_eq!(model.rust().candidates.count(), 0);
    assert!(!model.lookup(key.clone()).is_valid());
    assert_eq!(model.action(key, 160.), ffi::WatchAction::Unavailable.repr);
    drop(model); // Qt clears the source of the remaining proxy.
    println!(
        "Guide models: conflicts, exact identities, candidates, actions, reset and source lifetime passed"
    );
    Ok(())
}
