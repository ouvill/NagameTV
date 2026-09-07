//! Retain the latest subtitle outcome for presentation without restarting its session.
use super::{ffi, status::tr};
use crate::features::subtitles::Error;
use cxx_qt::CxxQtType;
use std::pin::Pin;

#[derive(Debug, Default)]
pub(super) enum Status {
    #[default]
    Stopped,
    Parsing,
    Failed(Error),
    PresentationFailed(serde_json::Error),
}

impl Status {
    fn source(&self) -> &'static str {
        match self {
            Self::Stopped => "Stopped",
            Self::Parsing => "Parsing subtitles",
            Self::PresentationFailed(_) => {
                "Could not prepare subtitles for display. Stop playback and play again."
            }
            Self::Failed(error) => match error {
                Error::PlaybackUnavailable => "Playback is unavailable for subtitles",
                Error::MissingBin => "The playback bin is unavailable for subtitles",
                Error::MissingBus => "The playback bus is unavailable for subtitles",
                Error::MissingService => {
                    "Cannot start subtitles without broadcast service information"
                }
                Error::DecoderUnavailable => "Could not initialize the subtitle decoder",
                Error::ParserPoisoned => {
                    "Subtitle parser state is invalid. Stop playback and play again."
                }
                Error::ClockPoisoned => {
                    "Subtitle timing state is invalid. Stop playback and play again."
                }
            },
        }
    }
}

impl ffi::Player {
    pub(super) fn update_subtitle_status(mut self: Pin<&mut Self>, status: Status) {
        match &status {
            Status::Failed(error) => eprintln!("Subtitle processing failed: {error}"),
            Status::PresentationFailed(error) => eprintln!("Subtitle presentation failed: {error}"),
            _ => {}
        }
        self.as_mut().rust_mut().subtitle_phase = status;
        self.refresh_subtitle_status();
    }

    pub(super) fn refresh_subtitle_status(mut self: Pin<&mut Self>) {
        let text = tr(self.rust().subtitle_phase.source());
        self.as_mut().set_subtitle_status(text);
    }
}
