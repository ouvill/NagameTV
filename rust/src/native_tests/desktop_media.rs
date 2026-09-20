//! Hardware-free MPRIS wire contract on a private session bus. The fixture is
//! synthetic media metadata; actual transport is exercised by the startup suite.
use crate::player::ffi;
use cxx_qt_lib::{QCoreApplication, QString};
use serde_json::json;
use std::{
    process::Command,
    thread,
    time::{Duration, Instant},
};

fn checks() -> Result<(), Box<dyn std::error::Error>> {
    let app = QCoreApplication::new();
    assert!(!app.is_null());
    let mut session = ffi::connect_desktop_media()?;
    let mut snapshot = json!({
        "track": "/org/mpris/MediaPlayer2/track/42", "title": "字幕のある番組", "artist": "試験放送",
        "status": "Playing", "can_play": true, "can_pause": true, "can_seek": true,
        "rate": 1.0, "minimum_rate": 0.5, "maximum_rate": 2.0,
        "seeking": false, "volume": 0.5, "position": 2_000_000, "length": 60_000_000,
    });
    session
        .pin_mut()
        .publish(&QString::from(snapshot.to_string()));
    let mut client = Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../tests/desktop-media.py"
        ))
        .arg(std::process::id().to_string())
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut received = Vec::new();
    loop {
        app.process_events();
        let command = session.pin_mut().take_command().to_string();
        if !command.is_empty() {
            let command: serde_json::Value = serde_json::from_str(&command)?;
            received.push(command.clone());
            match command["kind"].as_str().ok_or("command kind")? {
                "Rate" => snapshot["rate"] = command["value"].clone(),
                "Volume" => snapshot["volume"] = command["value"].clone(),
                "Pause" => snapshot["status"] = json!("Paused"),
                "Play" | "PlayPause" => snapshot["status"] = json!("Playing"),
                "SetPosition" => {
                    snapshot["seeking"] = json!(true);
                    session
                        .pin_mut()
                        .publish(&QString::from(snapshot.to_string()));
                    snapshot["position"] = command["value"].clone();
                    snapshot["seeking"] = json!(false);
                }
                "Stop" => {
                    snapshot["status"] = json!("Stopped");
                    snapshot["track"] = json!("");
                }
                "Raise" => (),
                unexpected => return Err(format!("Unexpected command: {unexpected}").into()),
            }
            session
                .pin_mut()
                .publish(&QString::from(snapshot.to_string()));
        }
        if let Some(status) = client.try_wait()? {
            assert!(status.success(), "MPRIS D-Bus client failed");
            break;
        }
        if Instant::now() >= deadline {
            client.kill()?;
            client.wait()?;
            return Err("MPRIS client timed out".into());
        }
        thread::sleep(Duration::from_millis(5));
    }
    let kinds: Vec<_> = received
        .iter()
        .map(|c| c["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        [
            "Volume",
            "Rate",
            "Pause",
            "Play",
            "SetPosition",
            "Raise",
            "Stop"
        ]
    );
    assert_eq!(received[0]["value"], 0.7);
    assert_eq!(received[4]["track"], "/org/mpris/MediaPlayer2/track/42");
    drop(session);
    let status = Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../tests/desktop-media.py"
        ))
        .arg(std::process::id().to_string())
        .arg("removed")
        .status()?;
    assert!(
        status.success(),
        "MPRIS registration survived owner destruction"
    );
    Ok(())
}

pub fn run() -> i32 {
    match checks() {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("Desktop media checks failed: {error}");
            1
        }
    }
}

// Called only by the validated GPU/audio startup suite, with real recording playback.
pub(super) fn check_playback(
    app: &cxx_qt_lib::QGuiApplication,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut client = Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../tests/desktop-media-playback.py"
        ))
        .arg(std::process::id().to_string())
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        app.process_events();
        if let Some(status) = client.try_wait()? {
            if !status.success() {
                return Err("Production MPRIS playback checks failed".into());
            }
            return Ok(());
        }
        if Instant::now() >= deadline {
            client.kill()?;
            client.wait()?;
            return Err("Production MPRIS checks timed out".into());
        }
        thread::sleep(Duration::from_millis(5));
    }
}
