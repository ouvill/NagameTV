//! Real RPC-to-Player checks under QCoreApplication; never create playback output.
use super::ffi;
use crate::remote::{
    Control,
    settings::{Preferences, Settings},
};
use cxx_qt::CxxQtType;
use std::{
    thread,
    time::{Duration, Instant},
};
use tonic::{Code, Request};
use viewer_remote::proto::{self, player_service_client::PlayerServiceClient};

pub(super) fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut player = ffi::new_player();
    assert!(player.rust().media.playback().is_none());
    let address = unused_address()?;
    player.pin_mut().rust_mut().remote = Control::new(Preferences::transient(Settings::default()));
    assert!(
        player
            .pin_mut()
            .configure_remote(true, "127.0.0.1".into(), address.port().into())
    );
    assert_eq!(player.remote_status().to_string(), "listening");
    player.pin_mut().rust_mut().autoplay_pending = true;
    let keys = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let emitted_keys = keys.clone();
    let _keys = player
        .pin_mut()
        .on_remote_key_requested(move |_, key| emitted_keys.lock().unwrap().push(key));
    let kinds = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let emitted_kinds = kinds.clone();
    let _kinds = player
        .pin_mut()
        .on_remote_command_executed(move |_, kind| emitted_kinds.lock().unwrap().push(kind));

    let client = thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let mut client = PlayerServiceClient::connect(format!("http://{address}"))
                .await
                .unwrap();
            client
                .set_muted(Request::new(proto::SetMutedRequest { muted: Some(true) }))
                .await
                .unwrap();
            let state = client
                .get_state(Request::new(proto::GetStateRequest {}))
                .await
                .unwrap()
                .into_inner()
                .state
                .unwrap();
            assert!(state.muted);
            client
                .set_volume(Request::new(proto::SetVolumeRequest {
                    fraction: Some(0.23),
                }))
                .await
                .unwrap();
            let state = client
                .get_state(Request::new(proto::GetStateRequest {}))
                .await
                .unwrap()
                .into_inner()
                .state
                .unwrap();
            assert!(!state.muted, "match the desktop slider's unmute behavior");
            assert!((state.volume_fraction - 0.23).abs() < 1e-9);
            assert_eq!(
                client
                    .play(Request::new(proto::PlayRequest {}))
                    .await
                    .unwrap_err()
                    .code(),
                Code::FailedPrecondition
            );
            assert_eq!(
                client
                    .select_channel(Request::new(proto::SelectChannelRequest {
                        channel_id: Some(999)
                    }))
                    .await
                    .unwrap_err()
                    .code(),
                Code::NotFound
            );
            assert_eq!(
                client
                    .set_subtitles(Request::new(proto::SetSubtitlesRequest {
                        visible: Some(true)
                    }))
                    .await
                    .unwrap_err()
                    .code(),
                Code::FailedPrecondition
            );
            macro_rules! unavailable {
                ($method:ident, $request:expr) => {
                    assert_eq!(
                        client.$method($request).await.unwrap_err().code(),
                        Code::FailedPrecondition
                    );
                };
            }
            unavailable!(set_paused, proto::SetPausedRequest { paused: Some(true) });
            unavailable!(
                seek_relative,
                proto::SeekRelativeRequest {
                    delta_ms: Some(-10_000)
                }
            );
            unavailable!(
                seek_to,
                proto::SeekToRequest {
                    position_ms: Some(0)
                }
            );
            unavailable!(return_to_live, proto::ReturnToLiveRequest {});
            unavailable!(
                set_playback_rate,
                proto::SetPlaybackRateRequest { tenths: Some(15) }
            );
            unavailable!(step_channel, proto::StepChannelRequest { offset: Some(1) });
            unavailable!(previous_channel, proto::PreviousChannelRequest {});
            unavailable!(
                select_remote_key,
                proto::SelectRemoteKeyRequest {
                    number: Some(1),
                    band: 0
                }
            );
            for key in 1..=9 {
                client
                    .send_key(proto::SendKeyRequest { key })
                    .await
                    .unwrap();
            }
            let state = client
                .get_state(proto::GetStateRequest {})
                .await
                .unwrap()
                .into_inner()
                .state
                .unwrap();
            assert!(
                !state.can_pause
                    && !state.can_seek
                    && !state.can_return_to_live
                    && !state.can_set_rate
            );
            client
                .stop(Request::new(proto::StopRequest {}))
                .await
                .unwrap();
        });
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    while !client.is_finished() {
        assert!(Instant::now() < deadline, "remote Player check timed out");
        player.pin_mut().poll();
        thread::sleep(Duration::from_millis(1));
    }
    client.join().map_err(|_| "remote client check panicked")?;
    assert_eq!(*keys.lock().unwrap(), (1..=9).collect::<Vec<_>>());
    let mut expected = vec![
        proto::RemoteCommandKind::SetMuted as i32,
        proto::RemoteCommandKind::SetVolume as i32,
    ];
    expected.extend(std::iter::repeat_n(
        proto::RemoteCommandKind::SendKey as i32,
        9,
    ));
    expected.push(proto::RemoteCommandKind::Stop as i32);
    assert_eq!(*kinds.lock().unwrap(), expected);
    assert!(!player.rust().autoplay_pending);
    assert!(!player.audio_muted());
    assert!((*player.volume_level() - 0.23).abs() < 1e-9);
    assert!(player.rust().media.playback().is_none());
    assert!(player.pin_mut().shutdown());
    let deadline = Instant::now() + Duration::from_secs(5);
    while player.rust().remote.status() != "disabled" {
        assert!(Instant::now() < deadline, "remote shutdown did not finish");
        player.pin_mut().rust_mut().remote.poll();
        thread::sleep(Duration::from_millis(1));
    }
    std::net::TcpListener::bind(address)?;
    check_reconfiguration()?;
    check_web_settings_and_qr()?;
    eprintln!("Remote RPC/Player integration checks passed (no hardware)");
    Ok(())
}

fn unused_address() -> Result<std::net::SocketAddr, std::io::Error> {
    std::net::TcpListener::bind("127.0.0.1:0")?.local_addr()
}

fn wait_for(player: &mut cxx::UniquePtr<ffi::Player>, status: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while player.remote_status().to_string() != status {
        assert!(
            Instant::now() < deadline,
            "remote did not reach {status}: {}",
            player.remote_error()
        );
        player.pin_mut().poll();
        thread::sleep(Duration::from_millis(1));
    }
}

fn check_reconfiguration() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let path = temporary.path().join("remote-control.toml");
    let mut first = ffi::new_player();
    first.pin_mut().rust_mut().remote = Control::new(Preferences::open(path.clone())?);
    assert!(!first.remote_enabled());
    assert_eq!(first.remote_address().to_string(), "0.0.0.0");
    assert_eq!(first.remote_port(), 50051);
    let address = unused_address()?;
    let _signal = first.pin_mut().on_remote_enabled_changed(|player| {
        assert_eq!(
            player.remote_enabled(),
            player.rust().remote.settings().enabled()
        );
        assert_eq!(
            player.remote_port(),
            i32::from(player.rust().remote.settings().endpoint().port())
        );
        if player.remote_status().to_string() == "listening" {
            assert!(!player.remote_endpoints().is_empty());
        }
    });
    assert!(
        first
            .pin_mut()
            .configure_remote(true, "127.0.0.1".into(), address.port().into())
    );
    wait_for(&mut first, "listening");
    assert_eq!(first.remote_endpoints().to_string(), address.to_string());
    assert!(Preferences::open(path.clone())?.current().enabled());

    let mut second = ffi::new_player();
    second.pin_mut().rust_mut().remote = Control::new(Preferences::open(path.clone())?);
    second.pin_mut().rust_mut().start_remote_if_requested();
    assert_eq!(second.remote_status().to_string(), "failed");
    assert!(
        second.remote_enabled(),
        "binding failure must preserve the user's choice"
    );
    assert!(second.remote_error().to_string().contains("already in use"));
    assert_eq!(first.remote_status().to_string(), "listening");

    let other = unused_address()?;
    assert!(
        second
            .pin_mut()
            .configure_remote(true, "127.0.0.1".into(), other.port().into())
    );
    wait_for(&mut second, "listening");
    first.pin_mut().save_settings();
    assert_eq!(Preferences::open(path.clone())?.current().endpoint(), other);
    assert!(
        !first
            .pin_mut()
            .configure_remote(true, "invalid".into(), 50051)
    );
    assert!(
        !first
            .pin_mut()
            .configure_remote(true, "127.0.0.1".into(), 0)
    );
    assert_eq!(first.remote_port(), i32::from(address.port()));

    // An immediate second edit while stopping supersedes the pending destination.
    let abandoned = unused_address()?;
    let replacement = unused_address()?;
    first
        .pin_mut()
        .configure_remote(true, "127.0.0.1".into(), abandoned.port().into());
    first
        .pin_mut()
        .configure_remote(true, "127.0.0.1".into(), replacement.port().into());
    wait_for(&mut first, "listening");
    assert_eq!(
        first.remote_endpoints().to_string(),
        replacement.to_string()
    );
    std::net::TcpListener::bind(address)?;
    std::net::TcpListener::bind(abandoned)?;
    first
        .pin_mut()
        .configure_remote(false, "127.0.0.1".into(), replacement.port().into());
    wait_for(&mut first, "disabled");
    std::net::TcpListener::bind(replacement)?;
    let restored = Preferences::open(path.clone())?.current();
    assert!(!restored.enabled());
    assert_eq!(restored.endpoint(), replacement);

    // A failed save remains visible and an unchanged Apply retries it.
    std::fs::remove_file(&path)?;
    std::fs::create_dir(&path)?;
    first
        .pin_mut()
        .configure_remote(true, "127.0.0.1".into(), replacement.port().into());
    assert!(!first.remote_save_error().is_empty());
    std::fs::remove_dir(&path)?;
    first
        .pin_mut()
        .configure_remote(true, "127.0.0.1".into(), replacement.port().into());
    assert!(first.remote_save_error().is_empty());
    assert!(Preferences::open(path)?.current().enabled());
    assert!(first.pin_mut().shutdown());
    assert!(second.pin_mut().shutdown());
    wait_for(&mut first, "disabled");
    wait_for(&mut second, "disabled");
    std::net::TcpListener::bind(replacement)?;
    std::net::TcpListener::bind(other)?;
    Ok(())
}

fn check_web_settings_and_qr() -> Result<(), Box<dyn std::error::Error>> {
    use base64::Engine;
    let temporary = tempfile::tempdir()?;
    let path = temporary.path().join("remote-control.toml");
    let mut player = ffi::new_player();
    player.pin_mut().rust_mut().remote = Control::new(Preferences::open(path.clone())?);
    assert!(player.remote_web_allowed());
    assert!(!player.remote_web_enabled());
    assert_eq!(player.remote_web_urls().to_string(), "[]");
    let address = unused_address()?;
    assert!(
        player
            .pin_mut()
            .configure_remote(true, "127.0.0.1".into(), address.port().into())
    );
    assert_eq!(remote_http_status(address)?, 404);
    assert!(player.pin_mut().set_remote_web_enabled(true));
    wait_for(&mut player, "listening");
    assert_eq!(remote_http_status(address)?, 200);
    let expected = format!("http://{address}/");
    let urls: Vec<String> = serde_json::from_str(&player.remote_web_urls().to_string())?;
    assert_eq!(urls, vec![expected.clone()]);
    let uri = player
        .remote_qr_data_uri(expected.clone().into())
        .to_string();
    let svg = String::from_utf8(
        base64::engine::general_purpose::STANDARD.decode(
            uri.strip_prefix("data:image/svg+xml;base64,")
                .expect("SVG data URI"),
        )?,
    )?;
    assert!(svg.contains("<svg") && svg.contains("<path"));
    assert!(
        player
            .remote_qr_data_uri("http://evil.example/".into())
            .is_empty()
    );
    assert!(Preferences::open(path.clone())?.current().web_enabled());
    assert!(player.pin_mut().set_remote_web_enabled(false));
    wait_for(&mut player, "listening");
    assert_eq!(remote_http_status(address)?, 404);
    assert!(player.pin_mut().set_remote_web_enabled(true));
    wait_for(&mut player, "listening");
    assert_eq!(remote_http_status(address)?, 200);
    assert!(
        player
            .pin_mut()
            .configure_remote(false, "127.0.0.1".into(), address.port().into())
    );
    wait_for(&mut player, "disabled");
    assert!(
        player.remote_web_enabled(),
        "listener edits preserve the web preference"
    );
    assert_eq!(player.remote_web_urls().to_string(), "[]");
    assert_eq!(player.remote_watchers(), 0);
    assert!(player.pin_mut().set_remote_web_enabled(false));
    assert!(!Preferences::open(path)?.current().web_enabled());
    assert!(player.pin_mut().shutdown());
    Ok(())
}

fn remote_http_status(address: std::net::SocketAddr) -> Result<u16, Box<dyn std::error::Error>> {
    use std::io::{BufRead, Write};
    let mut stream = std::net::TcpStream::connect(address)?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    write!(
        stream,
        "GET / HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
    )?;
    let mut status = String::new();
    std::io::BufReader::new(stream).read_line(&mut status)?;
    Ok(status
        .split_whitespace()
        .nth(1)
        .ok_or("missing HTTP status")?
        .parse()?)
}
