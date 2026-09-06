use super::*;
use serde_json::json;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn filters_non_tv_and_duplicates_without_truncating_ids() -> TestResult {
    let entries = parse(
        br#"[
        {"id":18446744073709551615,"name":"TV","type":1},
        {"id":18446744073709551615,"name":"Duplicate","type":1},
        {"id":2,"name":"Radio","type":2},
        {"id":0,"name":"Invalid ID","type":1},
        {"id":3,"name":"   ","type":1}
    ]"#,
    )?;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, u64::MAX);
    assert_eq!(entries[0].band, Band::Other);
    assert_eq!(entries[0].label, "--   TV");
    Ok(())
}

#[test]
fn matches_main_broadcast_and_remote_number_order() -> TestResult {
    let make = |id, band, number, remote, name| {
        json!({
            "id": id, "name": name, "type": 1, "serviceId": number,
            "remoteControlKeyId": remote, "channel": {"type": band}
        })
    };
    let mut services = vec![
        make(10, "NEW", 1, None, "Unknown"),
        make(8, "SKY", 200, None, "Sky"),
        make(7, "CS", 100, None, "CS"),
        make(6, "BS", 211, None, "BS 211"),
        make(5, "BS", 101, None, "BS 101"),
        make(4, "GR", 100, None, "No key"),
        make(3, "GR", 101, Some(0), "Zero key"),
        make(2, "GR", 11, Some(10), "Ten"),
        make(1, "GR", 99, Some(2), "Two"),
    ];
    let first = parse(&serde_json::to_vec(&services)?)?;
    assert_eq!(
        first.iter().map(|c| c.id).collect::<Vec<_>>(),
        [1, 2, 4, 3, 5, 6, 7, 8, 10]
    );
    assert_eq!(first[0].label, "02   Two");
    assert_eq!(first[4].label, "101   BS 101");
    services.reverse();
    let reversed = parse(&serde_json::to_vec(&services)?)?;
    assert_eq!(
        first.iter().map(|c| c.id).collect::<Vec<_>>(),
        reversed.iter().map(|c| c.id).collect::<Vec<_>>()
    );
    Ok(())
}

#[test]
fn equal_numbers_use_main_label_order_and_keep_distinct_services() -> TestResult {
    let entries = parse(br#"[
        {"id":9,"serviceId":101,"name":"Same","type":1,"remoteControlKeyId":1,"channel":{"type":"GR"}},
        {"id":8,"serviceId":100,"name":"Same","type":1,"remoteControlKeyId":1,"channel":{"type":"GR"}},
        {"id":7,"serviceId":102,"name":"Alpha","type":1,"remoteControlKeyId":1,"channel":{"type":"GR"}}
    ]"#)?;
    // Simulcast filtering needs EPG signatures and is deliberately a separate step.
    assert_eq!(entries.iter().map(|c| c.id).collect::<Vec<_>>(), [7, 8, 9]);
    Ok(())
}

#[test]
fn rejects_invalid_schema_and_empty_catalog() {
    for bytes in [
        b"broken".as_slice(),
        b"{}",
        br#"[{"id":1,"type":1}]"#,
        br#"[{"id":1,"name":"TV","type":1,"serviceId":65536}]"#,
    ] {
        assert!(matches!(parse(bytes), Err(Error::Json(_))));
    }
    for bytes in [b"[]".as_slice(), br#"[{"id":1,"name":"Radio","type":2}]"#] {
        assert!(matches!(parse(bytes), Err(Error::NoTvChannels)));
    }
}
