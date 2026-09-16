use super::*;
use model::{Band, Channel, Playback, SubtitleDisplay, Volume};
use prost::Message;
use proto::player_service_client::PlayerServiceClient;
use tonic::{Code, Request, transport::Channel as Transport};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn initial() -> State {
    State {
        selected_channel_id: Some(42),
        playback: Playback::Stopped,
        volume: Volume::new(0.5).unwrap(),
        muted: false,
        subtitles: SubtitleDisplay::Hidden,
        playback_error: String::new(),
        settings_error: String::new(),
        current_program: None,
    }
}

fn config() -> config::Config {
    config::Config::new("127.0.0.1:0".parse().unwrap())
}

fn session() -> Session {
    Bound::bind(config())
        .unwrap()
        .start(
            &Handle::current(),
            initial(),
            vec![Channel {
                id: 42,
                name: "Test TV".into(),
                label: "01".into(),
                band: Band::Terrestrial,
            }],
        )
        .unwrap()
}

async fn client(session: &Session) -> PlayerServiceClient<Transport> {
    PlayerServiceClient::connect(format!("http://{}", session.address()))
        .await
        .unwrap()
}

async fn next(session: &mut Session) -> Pending {
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if let Some(request) = session.next_request() {
                return request;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("command arrived")
}

#[test]
fn configuration_preserves_the_selected_endpoint_and_volume_is_validated() {
    for address in ["127.0.0.1:50051", "192.168.1.10:50051", "[::1]:50051"] {
        let config = config::Config::new(address.parse().unwrap());
        assert_eq!(config.address, address.parse::<SocketAddr>().unwrap());
    }
    for fraction in [-1.0, 1.01, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(Volume::new(fraction).is_err());
    }
}

#[tokio::test]
async fn unauthenticated_clients_can_read_and_invalid_commands_are_not_queued() {
    let mut session = session();
    let mut client = client(&session).await;
    let state = client
        .get_state(proto::GetStateRequest {})
        .await
        .unwrap()
        .into_inner()
        .state
        .unwrap();
    assert_eq!(state.selected_channel_id, Some(42));
    assert!(session.next_request().is_none());
    for fraction in [
        None,
        Some(-0.1),
        Some(1.1),
        Some(f64::NAN),
        Some(f64::INFINITY),
    ] {
        assert_eq!(
            client
                .set_volume(Request::new(proto::SetVolumeRequest { fraction }))
                .await
                .unwrap_err()
                .code(),
            Code::InvalidArgument
        );
    }
    assert_eq!(
        client
            .set_muted(Request::new(proto::SetMutedRequest { muted: None }))
            .await
            .unwrap_err()
            .code(),
        Code::InvalidArgument
    );
    assert_eq!(
        client
            .select_channel(Request::new(proto::SelectChannelRequest {
                channel_id: None
            }))
            .await
            .unwrap_err()
            .code(),
        Code::InvalidArgument
    );
    assert_eq!(
        client
            .set_subtitles(Request::new(proto::SetSubtitlesRequest { visible: None }))
            .await
            .unwrap_err()
            .code(),
        Code::InvalidArgument
    );
    assert!(session.next_request().is_none());
    let channels = client
        .list_channels(Request::new(proto::ListChannelsRequest {}))
        .await
        .unwrap()
        .into_inner()
        .channels;
    assert_eq!(channels.len(), 1);
    assert_eq!(channels[0].id, 42);
    session.stop().wait().await.unwrap();
}

#[tokio::test]
async fn unauthenticated_commands_acknowledge_execution_and_preserve_errors() {
    let mut session = session();
    let mut client = client(&session).await;
    let mut requester = client.clone();
    let call = tokio::spawn(async move {
        requester
            .set_muted(Request::new(proto::SetMutedRequest { muted: Some(false) }))
            .await
    });
    let request = next(&mut session).await.claim().unwrap();
    assert_eq!(request.command(), Command::SetMuted(false));
    assert!(
        !call.is_finished(),
        "enqueueing alone must not acknowledge execution"
    );
    request.complete(Ok(()));
    call.await.unwrap().unwrap();

    let call = tokio::spawn(async move {
        client
            .select_channel(Request::new(proto::SelectChannelRequest {
                channel_id: Some(999),
            }))
            .await
    });
    let request = next(&mut session).await.claim().unwrap();
    assert_eq!(request.command(), Command::SelectChannel(999));
    request.complete(Err(CommandError::ChannelNotFound));
    assert_eq!(call.await.unwrap().unwrap_err().code(), Code::NotFound);
    session.stop().wait().await.unwrap();
}

#[tokio::test]
async fn state_stream_starts_with_snapshot_and_recovers_latest_on_reconnect() {
    let mut session = session();
    let mut client = client(&session).await;
    let mut stream = client
        .watch_state(Request::new(proto::WatchStateRequest {}))
        .await
        .unwrap()
        .into_inner();
    let first = stream.message().await.unwrap().unwrap().state.unwrap();
    assert_eq!(first.revision, 1);
    assert!(matches!(
        first.playback,
        Some(proto::player_state::Playback::Stopped(_))
    ));
    session.publish(initial(), None);
    assert!(
        tokio::time::timeout(Duration::from_millis(40), stream.message())
            .await
            .is_err(),
        "unchanged snapshots must not be republished"
    );
    let mut state = initial();
    state.playback = Playback::StopFailed(42);
    state.muted = true;
    session.publish(state.clone(), None);
    let changed = stream.message().await.unwrap().unwrap().state.unwrap();
    assert_eq!(changed.revision, 2);
    assert!(changed.muted);
    assert!(matches!(
        changed.playback,
        Some(proto::player_state::Playback::StopFailed(
            proto::ActivePlayback { channel_id: 42 }
        ))
    ));
    drop(stream);
    state.playback = Playback::FilePlaying("録画.ts".into());
    session.publish(state, Some(vec![]));
    let mut stream = client
        .watch_state(Request::new(proto::WatchStateRequest {}))
        .await
        .unwrap()
        .into_inner();
    let resumed = stream.message().await.unwrap().unwrap().state.unwrap();
    assert_eq!(resumed.revision, 3);
    assert!(
        matches!(resumed.playback, Some(proto::player_state::Playback::FilePlaying(
        proto::FilePlayback { ref name }
    )) if name == "録画.ts")
    );
    assert!(
        client
            .list_channels(Request::new(proto::ListChannelsRequest {}))
            .await
            .unwrap()
            .into_inner()
            .channels
            .is_empty()
    );
    session.stop().wait().await.unwrap();
    assert!(stream.message().await.unwrap().is_none());
}

#[tokio::test]
async fn timeout_and_client_cancellation_discard_unclaimed_commands() {
    let mut session = session();
    let mut client = client(&session).await;
    let mut requester = client.clone();
    let call =
        tokio::spawn(async move { requester.stop(Request::new(proto::StopRequest {})).await });
    let pending = next(&mut session).await;
    assert_eq!(
        call.await.unwrap().unwrap_err().code(),
        Code::DeadlineExceeded
    );
    assert!(pending.claim().is_none());

    let call = tokio::spawn(async move { client.stop(Request::new(proto::StopRequest {})).await });
    let pending = next(&mut session).await;
    call.abort();
    let _ = call.await;
    tokio::time::timeout(Duration::from_secs(1), async {
        while !pending.reply.is_closed() {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    assert!(pending.claim().is_none());
    session.stop().wait().await.unwrap();
}

#[tokio::test]
async fn queue_and_subscriber_limits_recover_after_cancellation() {
    // Use RPC directly for deterministic queue saturation, independent of HTTP/2 limits.
    use proto::player_service_server::PlayerService;
    let (sender, mut receiver) = mpsc::channel(1);
    let (_published, snapshots) = watch::channel(Arc::new(Published {
        state: projection::state(&initial(), 1),
        channels: vec![],
    }));
    let rpc = Arc::new(rpc::Rpc::new(sender, snapshots, CancellationToken::new()));
    let first = rpc.clone();
    let first = tokio::spawn(async move { first.stop(Request::new(proto::StopRequest {})).await });
    tokio::time::timeout(Duration::from_secs(1), async {
        while receiver.is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        rpc.play(Request::new(proto::PlayRequest {}))
            .await
            .unwrap_err()
            .code(),
        Code::ResourceExhausted
    );
    receiver
        .recv()
        .await
        .unwrap()
        .claim()
        .unwrap()
        .complete(Ok(()));
    first.await.unwrap().unwrap();
    let mut streams = Vec::new();
    for _ in 0..16 {
        streams.push(
            rpc.watch_state(Request::new(proto::WatchStateRequest {}))
                .await
                .unwrap(),
        );
    }
    let error = rpc
        .watch_state(Request::new(proto::WatchStateRequest {}))
        .await
        .err()
        .unwrap();
    assert_eq!(error.code(), Code::ResourceExhausted);
    streams.clear();
    assert!(
        rpc.watch_state(Request::new(proto::WatchStateRequest {}))
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn shutdown_closes_subscriptions_discards_pending_and_releases_socket() {
    let mut session = session();
    let address = session.address();
    let mut client = client(&session).await;
    let mut stream = client
        .watch_state(Request::new(proto::WatchStateRequest {}))
        .await
        .unwrap()
        .into_inner();
    stream.message().await.unwrap().unwrap();
    let call = tokio::spawn(async move { client.stop(Request::new(proto::StopRequest {})).await });
    let pending = next(&mut session).await;
    let stopping = session.stop();
    assert!(
        pending.claim().is_none(),
        "stop invalidates execution rights immediately"
    );
    let error = call.await.unwrap().unwrap_err();
    assert_eq!(error.code(), Code::Unavailable);
    assert!(stream.message().await.unwrap().is_none());
    stopping.wait().await.unwrap();
    std::net::TcpListener::bind(address).unwrap();
}

fn frame(message: &impl Message) -> Vec<u8> {
    let body = message.encode_to_vec();
    let mut frame = vec![0];
    frame.extend_from_slice(&(body.len() as u32).to_be_bytes());
    frame.extend(body);
    frame
}

fn unframe(bytes: &[u8]) -> (&[u8], &[u8]) {
    assert_eq!(bytes[0], 0);
    let length = u32::from_be_bytes(bytes[1..5].try_into().unwrap()) as usize;
    (&bytes[5..5 + length], &bytes[5 + length..])
}

#[tokio::test]
async fn unauthenticated_grpc_web_unary_and_server_streaming_work_over_http1() -> Result {
    let mut session = session();
    let http = reqwest::Client::builder().http1_only().build()?;
    let url = |method| {
        format!(
            "http://{}/viewer.v1.PlayerService/{method}",
            session.address()
        )
    };
    let response = http
        .post(url("GetState"))
        .header("content-type", "application/grpc-web+proto")
        .header("x-grpc-web", "1")
        .body(frame(&proto::GetStateRequest {}))
        .send()
        .await?;
    assert_eq!(response.status(), 200);
    let bytes = response.bytes().await?;
    let (message, trailers) = unframe(&bytes);
    assert_eq!(
        proto::GetStateResponse::decode(message)?
            .state
            .unwrap()
            .revision,
        1
    );
    assert_eq!(trailers[0], 0x80);
    assert!(std::str::from_utf8(&trailers[5..])?.contains("grpc-status:0"));

    let mut response = http
        .post(url("WatchState"))
        .header("content-type", "application/grpc-web+proto")
        .header("x-grpc-web", "1")
        .body(frame(&proto::WatchStateRequest {}))
        .send()
        .await?;
    async fn receive(
        response: &mut reqwest::Response,
        buffered: &mut Vec<u8>,
    ) -> Result<proto::WatchStateResponse> {
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if buffered.len() >= 5 {
                    let length = u32::from_be_bytes(buffered[1..5].try_into()?) as usize;
                    if buffered.len() >= length + 5 {
                        let (message, _) = unframe(buffered);
                        let state = proto::WatchStateResponse::decode(message)?;
                        buffered.drain(..5 + length);
                        return Ok(state);
                    }
                }
                buffered.extend(
                    response
                        .chunk()
                        .await?
                        .ok_or("stream ended before message")?,
                );
            }
        })
        .await?
    }
    let mut buffered = vec![];
    assert_eq!(
        receive(&mut response, &mut buffered)
            .await?
            .state
            .unwrap()
            .revision,
        1
    );
    let command = http
        .post(url("SetMuted"))
        .header("content-type", "application/grpc-web+proto")
        .header("x-grpc-web", "1")
        .body(frame(&proto::SetMutedRequest { muted: Some(true) }));
    let call = tokio::spawn(async move { command.send().await });
    let request = next(&mut session).await.claim().unwrap();
    assert_eq!(request.command(), Command::SetMuted(true));
    let mut state = initial();
    state.muted = true;
    session.publish(state, None);
    request.complete(Ok(()));
    let reply = call.await??;
    assert_eq!(reply.status(), 200);
    let bytes = reply.bytes().await?;
    let (message, trailers) = unframe(&bytes);
    proto::SetMutedResponse::decode(message)?;
    assert_eq!(trailers[0], 0x80);
    assert!(std::str::from_utf8(&trailers[5..])?.contains("grpc-status:0"));
    assert!(
        receive(&mut response, &mut buffered)
            .await?
            .state
            .unwrap()
            .muted
    );
    drop(response);
    session.stop().wait().await?;
    Ok(())
}

#[test]
fn live_and_recording_transport_variants_roundtrip_without_exposing_a_path() {
    use proto::player_state::Playback as Wire;
    for (phase, expected) in [
        (
            Playback::Paused(42),
            Wire::Paused(proto::ActivePlayback { channel_id: 42 }),
        ),
        (
            Playback::Seeking(42),
            Wire::Seeking(proto::ActivePlayback { channel_id: 42 }),
        ),
        (
            Playback::SeekingPaused(42),
            Wire::SeekingPaused(proto::ActivePlayback { channel_id: 42 }),
        ),
        (
            Playback::Ended(42),
            Wire::Ended(proto::ActivePlayback { channel_id: 42 }),
        ),
        (
            Playback::FilePaused("sample.ts".into()),
            Wire::FilePaused(proto::FilePlayback {
                name: "sample.ts".into(),
            }),
        ),
        (
            Playback::FileSeeking("sample.ts".into()),
            Wire::FileSeeking(proto::FilePlayback {
                name: "sample.ts".into(),
            }),
        ),
        (
            Playback::FileSeekingPaused("sample.ts".into()),
            Wire::FileSeekingPaused(proto::FilePlayback {
                name: "sample.ts".into(),
            }),
        ),
        (
            Playback::FileEnded("sample.ts".into()),
            Wire::FileEnded(proto::FilePlayback {
                name: "sample.ts".into(),
            }),
        ),
    ] {
        let mut state = initial();
        state.playback = phase;
        let wire = projection::state(&state, 42);
        let decoded = proto::PlayerState::decode(wire.encode_to_vec().as_slice()).unwrap();
        assert_eq!(decoded.playback, Some(expected));
        assert_eq!(decoded.selected_channel_id, Some(42));
    }
}
