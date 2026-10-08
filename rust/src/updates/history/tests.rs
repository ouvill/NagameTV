use super::*;

const START: u64 = 1_700_000_000;
fn at(seconds: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(seconds)
}

#[test]
fn state_paths_follow_xdg_and_ignore_empty_or_relative_roots() {
    assert_eq!(
        resolve_path(Some("/custom/state".into()), Some("/home/user".into())).unwrap(),
        PathBuf::from("/custom/state/nagametv/updates.toml")
    );
    for state in [None, Some("".into()), Some("relative".into())] {
        assert_eq!(
            resolve_path(state, Some("/home/user".into())).unwrap(),
            PathBuf::from("/home/user/.local/state/nagametv/updates.toml")
        );
    }
    assert!(resolve_path(None, None).is_err());
    assert!(resolve_path(None, Some("relative".into())).is_err());
}

#[test]
fn attempt_and_success_survive_restart_and_the_daily_boundary()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("nagametv/updates.toml");
    let mut history = History::open(Ok(path.clone()));
    assert!(history.due(at(START)));
    assert!(!path.exists(), "opening history does not create an attempt");
    history.attempt(at(START));
    assert!(history.error().is_none());
    assert_eq!(read(&path)?.last_attempt_at, Some(START));
    assert_eq!(read(&path)?.last_success_at, None);
    history.succeeded(at(START + 2));
    assert!(history.error().is_none());
    let restored = History::open(Ok(path.clone()));
    assert_eq!(restored.last_success(), Some(START + 2));
    assert!(!restored.due(at(START + CHECK_INTERVAL.as_secs() - 1)));
    assert!(restored.due(at(START + CHECK_INTERVAL.as_secs())));
    let mut failed = restored;
    failed.attempt(at(START + CHECK_INTERVAL.as_secs()));
    assert_eq!(
        History::open(Ok(path)).last_success(),
        Some(START + 2),
        "an attempt alone must not claim success"
    );
    Ok(())
}

#[test]
fn clock_corrections_do_not_postpone_checks_indefinitely() -> Result<(), Box<dyn std::error::Error>>
{
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("updates.toml");
    let mut history = History::open(Ok(path.clone()));
    history.attempt(at(START));
    history.succeeded(at(START + 2));
    assert!(history.due(at(START - 60)));
    history.attempt(at(START - 60));
    assert_eq!(history.last_success(), None);
    assert!(!history.due(at(START - 60)));
    history.succeeded(at(START - 120));
    assert_eq!(read(&path)?.last_attempt_at, Some(START - 120));
    assert_eq!(history.last_success(), Some(START - 120));
    let invalid_clock = UNIX_EPOCH - Duration::from_secs(1);
    assert!(history.due(invalid_clock));
    history.attempt(invalid_clock);
    assert!(history.error().is_some());
    assert!(!history.due(invalid_clock));
    assert_eq!(read(&path)?.last_attempt_at, Some(START - 120));
    Ok(())
}

#[test]
fn corrupt_or_oversized_history_is_preserved_and_automatic_checks_are_blocked()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("updates.toml");
    for text in [
        "broken = [".into(),
        "last_attempt_at = -1".into(),
        "last_success_at = 20".into(),
        format!("last_attempt_at = {}", MAX_UNIX_SECONDS + 1),
        " ".repeat(MAX_HISTORY_BYTES as usize + 1),
    ] {
        fs::write(&path, &text)?;
        let mut history = History::open(Ok(path.clone()));
        assert!(history.error().is_some());
        assert!(!history.due(at(START)));
        history.attempt(at(START));
        history.succeeded(at(START));
        assert_eq!(fs::read_to_string(&path)?, text);
    }
    Ok(())
}

#[test]
fn save_failures_stop_automatic_checks_and_retain_the_attempt_in_memory()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("state/updates.toml");
    let mut history = History::open(Ok(path));
    fs::write(directory.path().join("state"), "not a directory")?;
    history.attempt(at(START));
    assert!(history.error().is_some());
    assert_eq!(history.record.last_attempt_at, Some(START));
    assert!(!history.due(at(START + CHECK_INTERVAL.as_secs())));
    assert_eq!(
        fs::read_to_string(directory.path().join("state"))?,
        "not a directory"
    );
    Ok(())
}
