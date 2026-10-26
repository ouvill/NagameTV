use crate::{
    COMMAND_TIMEOUT, Pending, Published,
    model::{self, Command, CommandError, Volume},
    proto,
};
use std::{pin::Pin, sync::Arc, time::Instant};
use tokio::sync::{Semaphore, mpsc, oneshot, watch};
use tokio_util::sync::CancellationToken;
use tonic::{Request, Response, Status};

pub(crate) struct Rpc {
    commands: mpsc::Sender<Pending>,
    snapshots: watch::Receiver<Arc<Published>>,
    stop: CancellationToken,
    subscribers: Arc<Semaphore>,
}

impl Rpc {
    pub(crate) fn new(
        commands: mpsc::Sender<Pending>,
        snapshots: watch::Receiver<Arc<Published>>,
        stop: CancellationToken,
        subscribers: Arc<Semaphore>,
    ) -> Self {
        Self {
            commands,
            snapshots,
            stop,
            subscribers,
        }
    }

    async fn execute(&self, command: Command) -> Result<(), Status> {
        if self.stop.is_cancelled() {
            return Err(Status::unavailable("viewer is stopping"));
        }
        let (reply, result) = oneshot::channel();
        self.commands
            .try_send(Pending {
                command,
                reply,
                deadline: Instant::now() + COMMAND_TIMEOUT,
                stop: self.stop.clone(),
            })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => {
                    Status::resource_exhausted("command queue is full")
                }
                mpsc::error::TrySendError::Closed(_) => Status::unavailable("viewer is stopping"),
            })?;
        tokio::select! {
            biased;
            () = self.stop.cancelled() => Err(Status::unavailable("viewer is stopping")),
            result = tokio::time::timeout(COMMAND_TIMEOUT, result) => {
                result.map_err(|_| Status::deadline_exceeded("command outcome unknown; read state before retrying"))?
                    .map_err(|_| Status::unavailable("command was not executed"))?
                    .map_err(|error| match error {
                        CommandError::Expired => Status::deadline_exceeded(error.to_string()),
                        CommandError::ChannelNotFound => Status::not_found(error.to_string()),
                        CommandError::NotReady(message) => Status::failed_precondition(message),
                        CommandError::Playback(message) => Status::internal(message),
                    })
            }
        }
    }
}

fn required<T>(field: Option<T>, name: &str) -> Result<T, Status> {
    field.ok_or_else(|| Status::invalid_argument(format!("{name} is required")))
}

#[tonic::async_trait]
impl proto::player_service_server::PlayerService for Rpc {
    async fn list_channels(
        &self,
        _: Request<proto::ListChannelsRequest>,
    ) -> Result<Response<proto::ListChannelsResponse>, Status> {
        Ok(Response::new(proto::ListChannelsResponse {
            channels: self.snapshots.borrow().channels.clone(),
        }))
    }

    async fn get_state(
        &self,
        _: Request<proto::GetStateRequest>,
    ) -> Result<Response<proto::GetStateResponse>, Status> {
        Ok(Response::new(proto::GetStateResponse {
            state: Some(self.snapshots.borrow().state.clone()),
        }))
    }

    type WatchStateStream =
        Pin<Box<dyn tokio_stream::Stream<Item = Result<proto::WatchStateResponse, Status>> + Send>>;

    async fn watch_state(
        &self,
        _: Request<proto::WatchStateRequest>,
    ) -> Result<Response<Self::WatchStateStream>, Status> {
        let permit = self
            .subscribers
            .clone()
            .try_acquire_owned()
            .map_err(|_| Status::resource_exhausted("too many state subscriptions"))?;
        let mut snapshots = self.snapshots.clone();
        let stop = self.stop.clone();
        let stream = async_stream::try_stream! {
            let _permit = permit;
            loop {
                if stop.is_cancelled() { break; }
                let state = snapshots.borrow_and_update().state.clone();
                yield proto::WatchStateResponse { state: Some(state) };
                tokio::select! {
                    biased;
                    () = stop.cancelled() => break,
                    changed = snapshots.changed() => if changed.is_err() { break; },
                }
            }
        };
        Ok(Response::new(Box::pin(stream)))
    }

    async fn select_channel(
        &self,
        request: Request<proto::SelectChannelRequest>,
    ) -> Result<Response<proto::SelectChannelResponse>, Status> {
        self.execute(Command::SelectChannel(required(
            request.into_inner().channel_id,
            "channel_id",
        )?))
        .await?;
        Ok(Response::new(proto::SelectChannelResponse {}))
    }
    async fn play(
        &self,
        _: Request<proto::PlayRequest>,
    ) -> Result<Response<proto::PlayResponse>, Status> {
        self.execute(Command::Play).await?;
        Ok(Response::new(proto::PlayResponse {}))
    }
    async fn stop(
        &self,
        _: Request<proto::StopRequest>,
    ) -> Result<Response<proto::StopResponse>, Status> {
        self.execute(Command::Stop).await?;
        Ok(Response::new(proto::StopResponse {}))
    }
    async fn set_volume(
        &self,
        request: Request<proto::SetVolumeRequest>,
    ) -> Result<Response<proto::SetVolumeResponse>, Status> {
        let value = Volume::new(required(request.into_inner().fraction, "fraction")?)
            .map_err(|error| Status::invalid_argument(error.to_string()))?;
        self.execute(Command::SetVolume(value)).await?;
        Ok(Response::new(proto::SetVolumeResponse {}))
    }
    async fn set_muted(
        &self,
        request: Request<proto::SetMutedRequest>,
    ) -> Result<Response<proto::SetMutedResponse>, Status> {
        self.execute(Command::SetMuted(required(
            request.into_inner().muted,
            "muted",
        )?))
        .await?;
        Ok(Response::new(proto::SetMutedResponse {}))
    }
    async fn set_subtitles(
        &self,
        request: Request<proto::SetSubtitlesRequest>,
    ) -> Result<Response<proto::SetSubtitlesResponse>, Status> {
        self.execute(Command::SetSubtitles(required(
            request.into_inner().visible,
            "visible",
        )?))
        .await?;
        Ok(Response::new(proto::SetSubtitlesResponse {}))
    }

    async fn set_paused(
        &self,
        request: Request<proto::SetPausedRequest>,
    ) -> Result<Response<proto::SetPausedResponse>, Status> {
        self.execute(Command::SetPaused(required(
            request.into_inner().paused,
            "paused",
        )?))
        .await?;
        Ok(Response::new(proto::SetPausedResponse {}))
    }
    async fn seek_relative(
        &self,
        request: Request<proto::SeekRelativeRequest>,
    ) -> Result<Response<proto::SeekRelativeResponse>, Status> {
        self.execute(Command::SeekRelative(valid(model::SeekDelta::new(
            required(request.into_inner().delta_ms, "delta_ms")?,
        ))?))
        .await?;
        Ok(Response::new(proto::SeekRelativeResponse {}))
    }
    async fn seek_to(
        &self,
        request: Request<proto::SeekToRequest>,
    ) -> Result<Response<proto::SeekToResponse>, Status> {
        self.execute(Command::SeekTo(valid(model::SeekPosition::new(required(
            request.into_inner().position_ms,
            "position_ms",
        )?))?))
        .await?;
        Ok(Response::new(proto::SeekToResponse {}))
    }
    async fn return_to_live(
        &self,
        _: Request<proto::ReturnToLiveRequest>,
    ) -> Result<Response<proto::ReturnToLiveResponse>, Status> {
        self.execute(Command::ReturnToLive).await?;
        Ok(Response::new(proto::ReturnToLiveResponse {}))
    }
    async fn set_playback_rate(
        &self,
        request: Request<proto::SetPlaybackRateRequest>,
    ) -> Result<Response<proto::SetPlaybackRateResponse>, Status> {
        self.execute(Command::SetPlaybackRate(valid(model::PlaybackRate::new(
            required(request.into_inner().tenths, "tenths")?,
        ))?))
        .await?;
        Ok(Response::new(proto::SetPlaybackRateResponse {}))
    }
    async fn step_channel(
        &self,
        request: Request<proto::StepChannelRequest>,
    ) -> Result<Response<proto::StepChannelResponse>, Status> {
        self.execute(Command::StepChannel(valid(model::Step::new(required(
            request.into_inner().offset,
            "offset",
        )?))?))
        .await?;
        Ok(Response::new(proto::StepChannelResponse {}))
    }
    async fn previous_channel(
        &self,
        _: Request<proto::PreviousChannelRequest>,
    ) -> Result<Response<proto::PreviousChannelResponse>, Status> {
        self.execute(Command::PreviousChannel).await?;
        Ok(Response::new(proto::PreviousChannelResponse {}))
    }
    async fn send_key(
        &self,
        request: Request<proto::SendKeyRequest>,
    ) -> Result<Response<proto::SendKeyResponse>, Status> {
        self.execute(Command::SendKey(valid(model::RemoteKey::new(
            request.into_inner().key,
        ))?))
        .await?;
        Ok(Response::new(proto::SendKeyResponse {}))
    }
    async fn select_remote_key(
        &self,
        request: Request<proto::SelectRemoteKeyRequest>,
    ) -> Result<Response<proto::SelectRemoteKeyResponse>, Status> {
        let request = request.into_inner();
        let number = valid(model::RemoteNumber::new(required(
            request.number,
            "number",
        )?))?;
        let band = match proto::BroadcastBand::try_from(request.band) {
            Ok(proto::BroadcastBand::Unspecified) => None,
            Ok(proto::BroadcastBand::Terrestrial) => Some(model::Band::Terrestrial),
            Ok(proto::BroadcastBand::Bs) => Some(model::Band::Bs),
            Ok(proto::BroadcastBand::Cs) => Some(model::Band::Cs),
            Ok(proto::BroadcastBand::Sky) => Some(model::Band::Sky),
            Ok(proto::BroadcastBand::Other) => Some(model::Band::Other),
            Err(_) => return Err(Status::invalid_argument("band is unknown")),
        };
        self.execute(Command::SelectRemoteKey(number, band)).await?;
        Ok(Response::new(proto::SelectRemoteKeyResponse {}))
    }
}

fn valid<T>(value: Result<T, model::InvalidCommand>) -> Result<T, Status> {
    value.map_err(|error| Status::invalid_argument(error.to_string()))
}
