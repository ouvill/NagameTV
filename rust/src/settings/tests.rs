use super::*;

#[test]
fn explicit_commit_persists_latest_changes_without_waiting_for_shutdown()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("settings.toml");
    let mut session = Session::open(path.clone())?;
    assert_eq!(session.flush()?, SaveStatus::Unchanged);
    for volume in [10.0, 20.0, 30.0] {
        session.preferences_mut().volume = Volume::from(volume);
    }
    assert!(!path.exists(), "editing alone must not perform IO");
    assert_eq!(session.flush()?, SaveStatus::Saved);
    let reader = Session::open(path.clone())?;
    assert_eq!(reader.preferences().volume.fraction(), 0.3);
    // Even an inaccessible destination needs no IO when the snapshot is unchanged.
    fs::remove_file(&path)?;
    fs::create_dir(&path)?;
    assert_eq!(session.flush()?, SaveStatus::Unchanged);
    session.preferences_mut().service_id = "123".into();
    assert!(matches!(session.flush(), Err(Error::Io { .. })));
    fs::remove_dir(&path)?;
    assert_eq!(session.flush()?, SaveStatus::Saved);
    assert_eq!(Session::open(path)?.preferences().service_id, "123");
    let mut transient = Session::transient(Preferences::default());
    transient.preferences_mut().volume = Volume::from(50.0);
    assert_eq!(transient.flush()?, SaveStatus::Transient);
    Ok(())
}

// Tests return Result for filesystem/setup failures; assertions describe contracts.
#[test]
fn compatible_main_settings_preserve_unported_fields() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("settings.toml");
    fs::write(
        &path,
        r#"
server = "http://example.test:40772"
service_id = "3209641984"
volume = 42.5
language = "ja"
danmaku_enabled = true
comment_font_size = 28.0
comment_opacity = 0.7
comment_speed = 1.25
subtitles_enabled = true
"#,
    )?;
    let mut session = Session::open(path.clone())?;
    assert_eq!(session.preferences().volume.fraction(), 0.425);
    assert!(session.preferences().subtitles_enabled);
    assert!(session.preferences().epg_enabled);
    assert!(!session.preferences().comments_enabled);
    session.preferences_mut().comments_enabled = true;
    session.preferences_mut().volume = Volume::from(20.0);
    session.flush()?;
    let loaded = load(&path)?;
    assert_eq!(loaded.volume.fraction(), 0.2);
    assert!(loaded.comments_enabled);
    assert_eq!(loaded.language, Language::Japanese);
    assert!(loaded.danmaku_enabled);
    assert_eq!(f64::from(loaded.comment_speed), 1.25);
    assert_eq!(f64::from(loaded.comment_font_size), 28.0);
    assert_eq!(f64::from(loaded.comment_opacity), 0.7);
    assert_eq!(fs::read_dir(dir.path())?.count(), 1);
    Ok(())
}

#[test]
fn missing_corrupt_and_oversized_files_are_distinct() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("settings.toml");
    assert_eq!(load(&path)?, Preferences::default());
    let mut session = Session::open(path.clone())?;
    session.flush()?;
    assert!(!path.exists(), "unchanged defaults need no write");
    fs::write(&path, "volume = [")?;
    assert!(matches!(
        Session::open(path.clone()),
        Err(Error::Parse { .. })
    ));
    assert_eq!(fs::read_to_string(&path)?, "volume = [");
    fs::write(&path, vec![b' '; MAX_SETTINGS_BYTES as usize + 1])?;
    assert!(matches!(load(&path), Err(Error::TooLarge)));
    Ok(())
}

#[test]
fn failed_replace_keeps_destination_and_cleans_temporary_file()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("settings.toml");
    fs::create_dir(&path)?;
    fs::write(path.join("keep"), b"existing data")?;
    assert!(matches!(
        save(&path, &Preferences::default()),
        Err(Error::Io { .. })
    ));
    assert_eq!(fs::read(path.join("keep"))?, b"existing data");
    assert_eq!(fs::read_dir(dir.path())?.count(), 1);
    Ok(())
}

#[test]
fn overrides_normalization_and_selection_are_independent_of_io()
-> Result<(), Box<dyn std::error::Error>> {
    let mut prefs: Preferences = toml::from_str("volume = nan\nservice_id = '12'\n")?;
    assert_eq!(prefs.volume, Volume::default());
    assert_eq!(Volume::from(-1.0).fraction(), 0.0);
    assert_eq!(Volume::from(120.0).fraction(), 1.0);
    assert!(Volume::from_fraction(f64::INFINITY).is_none());
    assert_eq!(prefs.selected_index([10, 12, 14].into_iter()), Some(1));
    assert_eq!(prefs.selected_index([10, 14].into_iter()), Some(0));
    assert_eq!(prefs.selected_index([].into_iter()), None);
    prefs.apply_overrides(Some("http://other:40772".into()), None);
    assert!(
        prefs.service_id.is_empty(),
        "saved channel belongs to old server"
    );
    prefs.apply_overrides(None, Some("14".into()));
    assert_eq!(prefs.selected_index([10, 12, 14].into_iter()), Some(2));
    let mut session = Session::transient(prefs);
    session.preferences_mut().volume = Volume::from(30.0);
    session.flush()?;
    assert!(matches!(session.persistence, Persistence::Transient));
    Ok(())
}

#[test]
fn comment_presentation_bounds_survive_invalid_persisted_values()
-> Result<(), Box<dyn std::error::Error>> {
    let prefs: Preferences =
        toml::from_str("comment_font_size = 500.0\ncomment_opacity = nan\ncomment_speed = -1.0")?;
    assert_eq!(f64::from(prefs.comment_font_size), 48.0);
    assert_eq!(f64::from(prefs.comment_opacity), 1.0);
    assert_eq!(f64::from(prefs.comment_speed), 0.5);
    assert!(CommentFontSize::checked(f64::NAN).is_none());
    assert!(CommentOpacity::checked(f64::INFINITY).is_none());
    assert!(CommentSpeed::checked(f64::NEG_INFINITY).is_none());
    let saved = toml::to_string(&prefs)?;
    assert_eq!(toml::from_str::<Preferences>(&saved)?, prefs);
    Ok(())
}

#[test]
fn language_codes_follow_main_and_unknown_ui_requests_are_rejected()
-> Result<(), Box<dyn std::error::Error>> {
    let prefs: Preferences = toml::from_str("")?;
    assert_eq!(prefs.language, Language::System);
    for (code, expected) in [
        ("ja", Language::Japanese),
        ("en", Language::English),
        ("system", Language::System),
        ("unsupported", Language::English),
    ] {
        let prefs: Preferences = toml::from_str(&format!("language = {code:?}"))?;
        assert_eq!(prefs.language, expected);
        let saved = toml::to_string(&prefs)?;
        assert_eq!(toml::from_str::<Preferences>(&saved)?.language, expected);
    }
    assert!(Language::parse("unsupported").is_none());
    Ok(())
}
