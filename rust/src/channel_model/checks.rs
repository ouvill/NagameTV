//! Hardware-free checks of the production models, with Qt's own model tester attached.
use super::*;
use cxx_qt_lib::{QMap, QMapPair_QString_QVariant};

fn field(row: QVariant, name: &str) -> QVariant {
    row.value::<QMap<QMapPair_QString_QVariant>>()
        .unwrap()
        .get(&QString::from(name))
        .unwrap()
        .clone()
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    const CHANNELS: &[u8] = br#"[
        {"id":10,"name":"First","type":1,"channel":{"type":"GR"}},
        {"id":18446744073709551615,"serviceId":200,"name":"Satellite","type":1,"hasLogoData":true,"channel":{"type":"BS"}}
    ]"#;
    let mut catalog = crate::channels::catalog::Catalog::default();
    catalog.replace(crate::channels::parse(CHANNELS)?, Some(u64::MAX));
    let mut source = ffi::make_channel_model();
    let mut filter = ffi::make_channel_filter_model();
    ffi::check_source_model(source.pin_mut());
    ffi::check_filter_model(filter.pin_mut());
    ffi::set_test_source(filter.pin_mut(), source.pin_mut());
    source
        .pin_mut()
        .replace(catalog.snapshot(), "http://localhost:40772/");
    assert_eq!(source.count(), 2);
    assert_eq!(filter.count(), 2);
    assert_eq!(
        field(source.row(1), "serviceId")
            .value::<QString>()
            .unwrap()
            .to_string(),
        u64::MAX.to_string()
    );
    assert_eq!(
        field(source.row(1), "logo")
            .value::<QString>()
            .unwrap()
            .to_string(),
        "http://localhost:40772/api/services/18446744073709551615/logo"
    );
    assert!(!source.data(&QModelIndex::default(), INDEX).is_valid());
    assert_eq!(
        source.row_count(&source.model_index(0, 0, &QModelIndex::default())),
        0
    );
    filter.pin_mut().set_band(QString::from("BS"));
    assert_eq!(filter.count(), 1);
    assert_eq!(filter.row_for_channel(1), 0);
    assert_eq!(field(filter.row(0), "channelIndex").value::<i32>(), Some(1));
    filter.pin_mut().set_visibility(ffi::Visibility::Listed);
    assert_eq!(filter.count(), 0, "an empty explicit set hides all rows");
    filter.pin_mut().set_opening_index(1);
    assert_eq!(
        filter.count(),
        1,
        "keep the viewed channel even when hidden by EPG"
    );
    filter.pin_mut().set_band(QString::from("GR"));
    assert_eq!(
        filter.count(),
        0,
        "opening channel cannot override the band filter"
    );
    assert_eq!(
        catalog.selected().unwrap().id,
        u64::MAX,
        "view filtering never selects"
    );
    filter.pin_mut().set_visibility(ffi::Visibility::All);
    assert_eq!(filter.count(), 1);
    let revision = *source.revision();
    let mut reversed = crate::channels::parse(CHANNELS)?;
    reversed.reverse();
    catalog.replace(reversed, None);
    source
        .pin_mut()
        .replace(catalog.snapshot(), "http://replacement");
    assert_ne!(*source.revision(), revision);
    assert_eq!(filter.row_for_channel(1), 0);
    assert_eq!(
        field(filter.row(0), "serviceId")
            .value::<QString>()
            .unwrap()
            .to_string(),
        "10"
    );
    assert_eq!(catalog.selected_index(), Some(0));
    drop(source);
    assert_eq!(
        filter.count(),
        0,
        "Qt clears a destroyed source without dangling pointers"
    );
    println!(
        "Channel models: roles, exact IDs, filtering, replacement and source destruction passed"
    );
    Ok(())
}
