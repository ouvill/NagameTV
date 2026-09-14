use super::*;

#[test]
fn screenshot_directory_defaults_and_custom_paths_survive_restart()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let path = temporary.path().join("settings.toml");
    let pictures = temporary.path().join("Pictures");
    let mut session = open(path.clone())?;
    assert_eq!(
        session
            .preferences()
            .screenshot_directory
            .resolve(&pictures),
        Some(pictures.join("mirakurun-viewer"))
    );
    assert!(
        session
            .preferences()
            .screenshot_directory
            .resolve(Path::new(""))
            .is_none()
    );
    let custom = temporary.path().join("スクリーンショット #100%");
    session.change(Change::ScreenshotDirectory(ScreenshotDirectory::try_from(
        custom.to_string_lossy().into_owned(),
    )?));
    session.flush()?;
    assert_eq!(
        open(path.clone())?
            .preferences()
            .screenshot_directory
            .resolve(&pictures),
        Some(custom)
    );
    session.change(Change::ScreenshotDirectory(ScreenshotDirectory::Pictures));
    session.flush()?;
    assert_eq!(
        open(path)?.preferences().screenshot_directory,
        ScreenshotDirectory::Pictures
    );
    assert!(ScreenshotDirectory::try_from("relative/path".to_owned()).is_err());
    assert!(toml::from_str::<Preferences>("screenshot_directory = '../relative'").is_err());
    Ok(())
}

#[test]
fn unconfigured_settings_stay_unconfigured_when_other_preferences_are_saved()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("settings.toml");
    let mut session = open(path.clone())?;
    assert!(session.preferences().server.is_empty());
    session.change(Change::Language(Language::Japanese));
    session.flush()?;
    assert!(open(path.clone())?.preferences().server.is_empty());
    fs::write(&path, "server = 'http://127.0.0.1:40772'\n")?;
    assert_eq!(open(path)?.preferences().server, "http://127.0.0.1:40772");
    Ok(())
}

#[test]
fn autoplay_preserves_main_environment_convention() {
    for saved in [false, true] {
        assert_eq!(autoplay_requested(saved, None), saved);
        assert!(!autoplay_requested(saved, Some("0")));
        for value in ["1", "true", "yes", "", "false", " 0 "] {
            assert!(
                autoplay_requested(saved, Some(value)),
                "main enables {value:?}"
            );
        }
    }
}

#[test]
fn autoplay_defaults_off_and_survives_restart() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("settings.toml");
    fs::write(&path, "service_id = '123'\nfuture_setting = 'keep'\n")?;
    assert!(!open(path.clone())?.preferences().autoplay);
    for enabled in [true, false] {
        let mut session = open(path.clone())?;
        session.change(Change::Autoplay(enabled));
        assert_eq!(session.flush()?, SaveStatus::Saved);
        let restored = open(path.clone())?;
        assert_eq!(restored.preferences().autoplay, enabled);
        assert_eq!(restored.preferences().service_id, "123");
        assert_eq!(
            restored.preferences().extra["future_setting"].as_str(),
            Some("keep")
        );
    }
    Ok(())
}

#[test]
fn explicit_commit_persists_latest_changes_without_waiting_for_shutdown()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("settings.toml");
    let mut session = open(path.clone())?;
    assert_eq!(session.flush()?, SaveStatus::Unchanged);
    for volume in [10.0, 20.0, 30.0] {
        session.change(Change::Volume(Volume::from(volume)));
    }
    assert!(!path.exists(), "editing alone must not perform IO");
    assert_eq!(session.flush()?, SaveStatus::Saved);
    let reader = open(path.clone())?;
    assert_eq!(reader.preferences().volume.fraction(), 0.3);
    // Even an inaccessible destination needs no IO when the snapshot is unchanged.
    fs::remove_file(&path)?;
    fs::create_dir(&path)?;
    assert_eq!(session.flush()?, SaveStatus::Unchanged);
    session.change(Change::Service("123".into()));
    assert!(matches!(session.flush(), Err(Error::Io { .. })));
    fs::remove_dir(&path)?;
    assert_eq!(session.flush()?, SaveStatus::Saved);
    assert_eq!(open(path)?.preferences().service_id, "123");
    let mut transient = Loaded::transient(Preferences::default()).activate(None, None);
    transient.change(Change::Volume(Volume::from(50.0)));
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
    let mut session = open(path.clone())?;
    assert_eq!(session.preferences().volume.fraction(), 0.425);
    assert!(session.preferences().show_subtitles);
    assert!(session.preferences().comments_enabled);
    session.change(Change::Comments(true));
    session.change(Change::Volume(Volume::from(20.0)));
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
fn subtitle_visibility_uses_the_existing_key_and_survives_restart()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("settings.toml");
    assert!(!Preferences::default().show_subtitles);
    for visible in [false, true] {
        fs::write(
            &path,
            format!(
                "subtitles_enabled = {visible}\nepg_enabled = false\nfuture_setting = 'keep'\n"
            ),
        )?;
        let mut session = open(path.clone())?;
        assert_eq!(session.preferences().show_subtitles, visible);
        // Legacy feature-disable flags cannot disable the normal launch's workers.
        let plan = crate::features::LaunchPlan::parse([])?;
        assert!(plan.subtitles && plan.epg);
        session.change(Change::SubtitleDisplay(!visible));
        assert_eq!(session.flush()?, SaveStatus::Saved);
        let restored = open(path.clone())?;
        assert_eq!(restored.preferences().show_subtitles, !visible);
        assert_eq!(
            restored.preferences().extra["future_setting"].as_str(),
            Some("keep")
        );
        let document: toml::Table = toml::from_str(&fs::read_to_string(&path)?)?;
        assert_eq!(document["subtitles_enabled"].as_bool(), Some(!visible));
    }
    Ok(())
}

#[test]
fn commentary_defaults_match_main_and_explicit_disable_survives_roundtrip()
-> Result<(), Box<dyn std::error::Error>> {
    let defaults: Preferences = toml::from_str("")?;
    assert_eq!(defaults, Preferences::default());
    assert!(defaults.comments_enabled);
    assert!(!defaults.danmaku_enabled);
    // Legacy main files only store the overlay choice. Both choices still
    // receive history; new files may explicitly disable the entire feature.
    for overlay in [false, true] {
        let legacy: Preferences = toml::from_str(&format!("danmaku_enabled = {overlay}"))?;
        assert!(legacy.comments_enabled);
        assert_eq!(legacy.danmaku_enabled, overlay);
        let disabled: Preferences = toml::from_str(&format!(
            "comments_enabled = false\ndanmaku_enabled = {overlay}"
        ))?;
        assert!(!disabled.comments_enabled);
        assert_eq!(disabled.danmaku_enabled, overlay);
        assert_eq!(
            toml::from_str::<Preferences>(&toml::to_string(&disabled)?)?,
            disabled
        );
    }
    Ok(())
}

#[test]
fn missing_corrupt_and_oversized_files_are_distinct() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("settings.toml");
    assert_eq!(load(&path)?, Preferences::default());
    let mut session = open(path.clone())?;
    session.flush()?;
    assert!(!path.exists(), "unchanged defaults need no write");
    fs::write(&path, "volume = [")?;
    assert!(matches!(open(path.clone()), Err(Error::Parse { .. })));
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
    let mut session = Loaded::transient(prefs).activate(None, None);
    session.change(Change::Volume(Volume::from(30.0)));
    session.flush()?;
    assert!(matches!(session.persistence, Persistence::Transient));
    Ok(())
}

#[test]
fn comment_presentation_bounds_survive_invalid_persisted_values()
-> Result<(), Box<dyn std::error::Error>> {
    let prefs: Preferences =
        toml::from_str("comment_font_size = 500.0\ncomment_opacity = nan\ncomment_speed = -1.0")?;
    assert_eq!(f64::from(prefs.comment_font_size), 72.0);
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

/// Manual filesystem measurement; no display, audio, or user settings are used.
#[test]
#[ignore = "manual latency measurement; requires SETTINGS_BENCH_DIR"]
fn measure_explicit_save_latency() -> Result<(), Box<dyn std::error::Error>> {
    use std::time::Instant;
    let base = std::env::var_os("SETTINGS_BENCH_DIR")
        .ok_or("set SETTINGS_BENCH_DIR to a scratch directory")?;
    let directory = tempfile::tempdir_in(base)?;
    for extra_bytes in [0, 60 * 1024] {
        let path = directory
            .path()
            .join(format!("settings-{extra_bytes}.toml"));
        let mut session = open(path.clone())?;
        if extra_bytes > 0 {
            session.preferences.extra.insert(
                "benchmark_payload".into(),
                toml::Value::String("x".repeat(extra_bytes)),
            );
        }
        let mut saves = Vec::with_capacity(100);
        let mut unchanged = Vec::with_capacity(100);
        for index in 0..100 {
            session.change(Change::Service(index.to_string()));
            let start = Instant::now();
            assert_eq!(session.flush()?, SaveStatus::Saved);
            saves.push(start.elapsed());
            let start = Instant::now();
            assert_eq!(session.flush()?, SaveStatus::Unchanged);
            unchanged.push(start.elapsed());
        }
        assert_eq!(open(path.clone())?.preferences().service_id, "99");
        saves.sort_unstable();
        unchanged.sort_unstable();
        println!(
            "settings_bytes={} samples=100 save_us_p50={} save_us_p95={} save_us_max={} unchanged_us_p95={} unchanged_us_max={}",
            fs::metadata(path)?.len(),
            saves[49].as_micros(),
            saves[94].as_micros(),
            saves[99].as_micros(),
            unchanged[94].as_micros(),
            unchanged[99].as_micros()
        );
    }
    Ok(())
}

#[test]
fn comment_send_shortcut_defaults_and_persists_without_saving_drafts()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("settings.toml");
    fs::write(&path, "comments_enabled = true\n")?;
    let mut session = open(path.clone())?;
    assert!(!session.preferences().comment_send_on_enter);
    for enabled in [true, false] {
        session.change(Change::CommentSendOnEnter(enabled));
        assert_eq!(session.flush()?, SaveStatus::Saved);
        assert_eq!(
            open(path.clone())?.preferences().comment_send_on_enter,
            enabled
        );
    }
    assert!(!fs::read_to_string(path)?.contains("draft"));
    Ok(())
}

#[test]
fn comment_shadow_and_large_font_survive_settings_reload() -> Result<(), Box<dyn std::error::Error>>
{
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("settings.toml");
    // Existing settings acquire the default shadow without changing their size.
    fs::write(&path, "comment_font_size = 36.0\n")?;
    let mut session = open(path.clone())?;
    assert!(session.preferences().comment_shadow_enabled);
    assert_eq!(f64::from(session.preferences().comment_font_size), 36.0);
    for (shadow, size) in [(false, 72.0), (true, 60.0), (false, 14.0)] {
        session.change(Change::CommentShadow(shadow));
        let preferences = session.preferences();
        session.change(Change::Danmaku {
            enabled: preferences.danmaku_enabled,
            size: CommentFontSize::checked(size).unwrap(),
            opacity: preferences.comment_opacity,
            speed: preferences.comment_speed,
        });
        assert_eq!(session.flush()?, SaveStatus::Saved);
        session = open(path.clone())?;
        assert_eq!(session.preferences().comment_shadow_enabled, shadow);
        assert_eq!(f64::from(session.preferences().comment_font_size), size);
    }
    Ok(())
}

fn open(path: std::path::PathBuf) -> Result<Session, Error> {
    Loaded::open(path).map(|loaded| loaded.activate(None, None))
}
