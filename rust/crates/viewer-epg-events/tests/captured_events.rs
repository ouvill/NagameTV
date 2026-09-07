//! Manual replay of captured server events; no network or hardware access.
use std::{
    io::Read,
    time::{Duration, Instant},
};
use viewer_epg_events::{Decoder, RefreshGate};

#[test]
#[ignore = "requires MIRAKURUN_EVENT_FIXTURE containing a complete captured JSON array"]
fn captured_program_events_request_one_refresh() -> Result<(), Box<dyn std::error::Error>> {
    const LIMIT: u64 = 1024 * 1024;
    let path = std::env::var_os("MIRAKURUN_EVENT_FIXTURE").ok_or("fixture path required")?;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > LIMIT {
        return Err("capture exceeds 1 MiB".into());
    }
    let events: Vec<serde_json::Value> = serde_json::from_slice(&bytes)?;
    let expected = events
        .iter()
        .filter(|event| {
            event["resource"] == "program"
                && matches!(event["type"].as_str(), Some("create" | "update" | "remove"))
        })
        .count();
    assert!(expected > 0, "capture must contain real program changes");
    for chunk_size in [1, 7, 4096] {
        let now = Instant::now();
        let mut decoder = Decoder::default();
        let mut gate = RefreshGate::new(now);
        let mut changed_chunks = 0;
        for chunk in bytes.chunks(chunk_size) {
            if decoder.push(chunk)? {
                changed_chunks += 1;
                gate.changed();
            }
        }
        // At one byte per call, each completed event must be observed separately.
        if chunk_size == 1 {
            assert_eq!(changed_chunks, expected);
        }
        assert!(changed_chunks > 0);
        assert!(!gate.take_due(now + Duration::from_secs(59)));
        assert!(gate.take_due(now + Duration::from_secs(60)));
        assert!(!gate.take_due(now + Duration::from_secs(120)));
    }
    eprintln!("CAPTURED_EPG events={expected} chunk_sizes=1,7,4096 refreshes_per_replay=1");
    Ok(())
}
