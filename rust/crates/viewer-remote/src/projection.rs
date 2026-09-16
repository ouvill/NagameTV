use crate::{model, proto};

pub(crate) fn state(state: &model::State, revision: u64) -> proto::PlayerState {
    use proto::player_state::Playback;
    let active = |channel_id| proto::ActivePlayback { channel_id };
    let playback = match &state.playback {
        model::Playback::Stopped => Playback::Stopped(proto::StoppedPlayback {}),
        model::Playback::Connecting(id) => Playback::Connecting(active(*id)),
        model::Playback::Playing(id) => Playback::Playing(active(*id)),
        model::Playback::StopFailed(id) => Playback::StopFailed(active(*id)),
        model::Playback::FileConnecting(name) => {
            Playback::FileConnecting(proto::FilePlayback { name: name.clone() })
        }
        model::Playback::FilePlaying(name) => {
            Playback::FilePlaying(proto::FilePlayback { name: name.clone() })
        }
        model::Playback::FilePaused(name) => {
            Playback::FilePaused(proto::FilePlayback { name: name.clone() })
        }
        model::Playback::FileSeeking(name) => {
            Playback::FileSeeking(proto::FilePlayback { name: name.clone() })
        }
        model::Playback::FileSeekingPaused(name) => {
            Playback::FileSeekingPaused(proto::FilePlayback { name: name.clone() })
        }
        model::Playback::FileEnded(name) => {
            Playback::FileEnded(proto::FilePlayback { name: name.clone() })
        }
        model::Playback::FileStopFailed(name) => {
            Playback::FileStopFailed(proto::FilePlayback { name: name.clone() })
        }
    };
    proto::PlayerState {
        revision,
        selected_channel_id: state.selected_channel_id,
        playback: Some(playback),
        volume_fraction: state.volume.fraction(),
        muted: state.muted,
        subtitles: match state.subtitles {
            model::SubtitleDisplay::Disabled => proto::SubtitleDisplay::Disabled,
            model::SubtitleDisplay::Hidden => proto::SubtitleDisplay::Hidden,
            model::SubtitleDisplay::Visible => proto::SubtitleDisplay::Visible,
        } as i32,
        playback_error: state.playback_error.clone(),
        settings_error: state.settings_error.clone(),
        current_program: state.current_program.as_ref().map(|p| proto::Program {
            id: p.id,
            name: p.name.clone(),
            description: p.description.clone(),
            start_at_ms: p.start_at_ms,
            duration_ms: p.duration_ms,
        }),
    }
}

pub(crate) fn channel(channel: &model::Channel) -> proto::Channel {
    proto::Channel {
        id: channel.id,
        name: channel.name.clone(),
        label: channel.label.clone(),
        band: match channel.band {
            model::Band::Terrestrial => proto::BroadcastBand::Terrestrial,
            model::Band::Bs => proto::BroadcastBand::Bs,
            model::Band::Cs => proto::BroadcastBand::Cs,
            model::Band::Sky => proto::BroadcastBand::Sky,
            model::Band::Other => proto::BroadcastBand::Other,
        } as i32,
    }
}
