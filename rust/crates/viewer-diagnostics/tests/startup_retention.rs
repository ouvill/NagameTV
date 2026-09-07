//! Exercise startup pruning with real process lifetimes and flushed recorder files.
#![cfg(target_os = "linux")]

use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime},
};
use viewer_diagnostics::{
    Snapshot,
    recorder::{Enqueue, Event, Recorder},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;
const CHILD_DIRECTORY: &str = "VIEWER_RETENTION_TEST_DIRECTORY";

#[test]
#[ignore = "helper invoked in a separate process by startup_keeps_completed_and_live_logs"]
fn recorder_child() -> TestResult {
    let directory = std::env::var_os(CHILD_DIRECTORY).ok_or("missing child directory")?;
    let recorder = Recorder::start_directory(directory.into(), "retention-test")?;
    assert_eq!(
        recorder.record(Event::Sample, Snapshot::default()),
        Enqueue::Accepted
    );
    recorder.stop().join()?;
    Ok(())
}

fn run_recorder(directory: &Path) -> Result<u32, Box<dyn std::error::Error>> {
    let mut child = Command::new(std::env::current_exe()?)
        .args(["--ignored", "--exact", "recorder_child", "--nocapture"])
        .env(CHILD_DIRECTORY, directory)
        .stdin(Stdio::null())
        .spawn()?;
    let pid = child.id();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return Err(format!("recorder child {pid} failed: {status}").into());
                }
                return Ok(pid);
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            result => {
                // Own and reap the helper on timeout or a polling failure; never
                // leave it writing into a temporary directory after this test.
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("recorder child {pid} did not finish: {result:?}").into());
            }
        }
    }
}

#[test]
fn startup_keeps_completed_and_live_logs() -> TestResult {
    let directory = tempfile::tempdir()?;
    // This real live PID must be excluded even when its file is the oldest.
    let live = Recorder::start_directory(directory.path().to_owned(), "live-test")?;
    let live_path = directory
        .path()
        .join(format!("usage-{}.jsonl", std::process::id()));
    fs::File::open(&live_path)?.set_modified(SystemTime::UNIX_EPOCH)?;
    fs::write(directory.path().join("notes.jsonl"), "unrelated")?;
    let mut completed = Vec::new();
    for sequence in 0..9 {
        let pid = run_recorder(directory.path())?;
        assert!(!completed.contains(&pid), "child PID reused during test");
        assert!(!Path::new(&format!("/proc/{pid}")).exists());
        let path = directory.path().join(format!("usage-{pid}.jsonl"));
        let text = fs::read_to_string(&path)?;
        let lines: Vec<_> = text.lines().collect();
        assert_eq!(lines.len(), 1, "stop must flush the accepted sample");
        let entry: serde_json::Value = serde_json::from_str(lines[0])?;
        assert_eq!(entry["record"]["pid"], pid);
        assert_eq!(entry["record"]["event"], "sample");
        // Deterministic ordering without sleeps or assumptions about filesystem
        // timestamp resolution. Liveness detection still uses actual /proc.
        fs::File::open(&path)?
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(sequence + 1))?;
        completed.push(pid);
        assert!(live_path.exists());
        assert_eq!(
            fs::read_to_string(directory.path().join("notes.jsonl"))?,
            "unrelated"
        );
        // Startup retains six completed segments, then creates its own. Once
        // that process exits there may be seven until the next startup/prune.
        for (index, previous) in completed.iter().enumerate() {
            assert_eq!(
                directory
                    .path()
                    .join(format!("usage-{previous}.jsonl"))
                    .exists(),
                index >= completed.len().saturating_sub(7),
            );
        }
    }
    assert_eq!(viewer_diagnostics::retention::prune(directory.path())?, 1);
    assert!(live_path.exists());
    assert_eq!(
        live.record(Event::Sample, Snapshot::default()),
        Enqueue::Accepted
    );
    live.stop().join()?;
    assert!(!fs::read_to_string(live_path)?.is_empty());
    Ok(())
}
