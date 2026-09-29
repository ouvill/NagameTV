//! Translate application errors at the presentation boundary, retaining external causes verbatim.
pub(super) use crate::qt::text::Text;

pub(super) trait PresentError {
    fn present(&self) -> Text;
}

fn detail(source: &'static str, value: impl std::fmt::Display) -> Text {
    Text::message(source, [Text::literal(value)])
}

impl PresentError for crate::playback::Error {
    fn present(&self) -> Text {
        use crate::playback::{AudioSinkError, ClockError, Error::*, deinterlace};
        match self {
            Cleanup { primary, cleanup } => Text::message(
                "%1 (stopping also failed: %2)",
                [primary.present(), cleanup.present()],
            ),
            Deinterlace(error) => match error {
                deinterlace::Error::Invalid(value) => detail(
                    "Invalid NAGAMETV_DEINTERLACE=%1; expected yadif, linear, off, gl or va",
                    value,
                ),
                deinterlace::Error::NonUnicode => {
                    Text::source("NAGAMETV_DEINTERLACE is not valid Unicode")
                }
            },
            AudioSink(error) => match error {
                AudioSinkError::Invalid(value) => detail(
                    "Invalid NAGAMETV_AUDIO_SINK=%1; expected pulsesink or fakesink",
                    value,
                ),
                AudioSinkError::NonUnicode => {
                    Text::source("NAGAMETV_AUDIO_SINK is not valid Unicode")
                }
            },
            Clock(error) => match error {
                ClockError::Invalid(value) => detail(
                    "Invalid NAGAMETV_PLAYBACK_CLOCK=%1; expected auto or system",
                    value,
                ),
                ClockError::NonUnicode => {
                    Text::source("NAGAMETV_PLAYBACK_CLOCK is not valid Unicode")
                }
                ClockError::InvalidRefreshRate(value) => {
                    detail("Invalid display refresh rate: %1 Hz", value)
                }
            },
            MediaSubtitle(error) => error.present(),
            VideoOutput(error) => detail("Video output: %1", error),
            Initialization(error) => detail("GStreamer initialization failed: %1", error),
            Operation(error) => detail("GStreamer operation failed: %1", error),
            StateChange(error) => detail("GStreamer state change failed: %1", error),
            AlreadyInitialized => Text::source("Playback already initialized"),
            Unavailable => Text::source("Playback unavailable"),
            MissingSinkPad => Text::source("Missing video sink pad"),
            MissingVideoItem => Text::source("Missing Qt video item"),
            InvalidVideoItem => {
                Text::source("Video output requires a GStreamer video item on the GUI thread")
            }
            OutputAlreadyAttached => Text::source("Video output is already attached"),
            OutputShutDown => Text::source("Video output has been shut down"),
            OutputNotReady => Text::source("Video output is not ready"),
            MissingBus => Text::source("Missing GStreamer bus"),
            MissingDecoder(error) => detail("Missing video or audio decoder: %1", error),
            EndOfStream => Text::source("The stream has ended"),
            Recording(error) => error.present(),
            Input(error) => error.present(),
            Transport(error) => error.present(),
            // GStreamer/HTTP response diagnostics are retained verbatim.
            LiveResumeRejected { .. } | Stream { .. } => Text::literal(self),
        }
    }
}

impl PresentError for crate::media_subtitles::Error {
    fn present(&self) -> Text {
        match self {
            Self::Read(error) => detail("Could not read subtitles: %1", error),
            Self::Format => Text::source("Select a valid SRT or ASS subtitle file"),
            Self::Encoding => Text::source("Save the subtitle file as UTF-8"),
            Self::Capacity => Text::source("Subtitle data exceeds the supported limit"),
            Self::Renderer(error) => detail("Could not render subtitles: %1", error),
            Self::Sink(error) => detail("Could not receive subtitles: %1", error),
            Self::Unavailable => Text::source("Subtitle track is no longer available"),
            Self::Rejected => Text::source("Subtitle selection was rejected"),
            Self::Worker => Text::source("Subtitle worker stopped unexpectedly"),
        }
    }
}

impl PresentError for crate::playback::timeline::Error {
    fn present(&self) -> Text {
        use crate::playback::timeline::Error::*;
        match self {
            Retention(error) => {
                Text::message("Could not start retaining playback: %1", [error.present()])
            }
            Unavailable => Text::source("Seeking is not available yet"),
            RateUnavailable => Text::source("Playback speed cannot be changed"),
            InvalidRate => Text::source("Choose a playback speed from 0.5 to 2.0 in steps of 0.1"),
            InvalidPosition => Text::source("The playback position is invalid"),
            Rejected => Text::source("The seek request was rejected"),
            RateTimedOut => Text::source(
                "Playback was paused because the speed change could not be confirmed. Resuming will restore normal speed.",
            ),
            TimedOut => Text::source("The seek could not be confirmed"),
            Observer => Text::source("Could not monitor the playback position"),
            State(error) => detail("Could not change the playback state: %1", error),
        }
    }
}

impl PresentError for crate::playback::input::Error {
    fn present(&self) -> Text {
        use crate::playback::input::Error::*;
        match self {
            Io(error) => detail("TS input: %1", error),
            Filter(error) => detail("TS normalization: %1", error),
            Poisoned => Text::source("The TS input state is invalid"),
            NoReadProgress => Text::source("The TS input read position did not advance"),
            Expired => Text::source("The seek target is outside retained history"),
            Unindexed => {
                Text::source("Program information for the seek target is not available yet")
            }
            Cancelled => Text::source("TS exploration was cancelled"),
            ExplorationTimedOut => Text::source("The seek search reached its time limit"),
        }
    }
}

impl PresentError for crate::playback::recording::Error {
    fn present(&self) -> Text {
        use crate::playback::recording::Error::*;
        match self {
            NotLocal => Text::source("Select a local video file."),
            InvalidUrl => Text::source(
                "Enter a local file URL or a recording URL starting with http:// or https://.",
            ),
            ParseUrl(error) => detail("Invalid recording URL: %1", error),
            // HTTP response diagnostics belong to the remote server/transport.
            Http(error) => Text::literal(error),
            Portal(error) => detail(
                "Could not receive the dropped file: %1. Use Open video file to select it.",
                error,
            ),
            Read(error) => detail("Could not read the video file: %1", error),
            MissingProgram => {
                Text::source("No supported TS, MP4 or Matroska video was found in this file.")
            }
            TimedOut => {
                Text::source("Video inspection reached its time limit. Try opening the file again.")
            }
            Cancelled => Text::source("Video inspection was cancelled."),
            WorkerStopped => Text::source("The video inspection worker stopped unexpectedly."),
            Initialization(error) => detail("Could not initialize media inspection: %1", error),
            TooLarge => Text::source("The video file is too large"),
        }
    }
}

impl PresentError for crate::services::Error {
    fn present(&self) -> Text {
        match self {
            Self::Url(error) => detail("Could not parse the server URL: %1", error),
            Self::InvalidServerUrl => {
                Text::source("Enter a server URL starting with http:// or https://")
            }
        }
    }
}

impl PresentError for crate::services::NetworkError {
    fn present(&self) -> Text {
        match self {
            Self::Runtime(error) => detail("Could not initialize networking: %1", error),
            Self::Http(_) => Text::literal(self),
            Self::ResponseTooLarge { limit } => detail("The response exceeds %1 bytes", limit),
            Self::WorkerStopped => Text::source("The channel worker stopped unexpectedly"),
        }
    }
}

impl<E: PresentError> PresentError for crate::services::FetchError<E> {
    fn present(&self) -> Text {
        match self {
            Self::Network(error) => error.present(),
            Self::Parse(error) => error.present(),
        }
    }
}

impl PresentError for crate::channels::Error {
    fn present(&self) -> Text {
        // The remote response's parse diagnostics remain literal.
        Text::literal(self)
    }
}

impl PresentError for crate::epgstation::Error {
    fn present(&self) -> Text {
        match self {
            Self::Address(error) => error.present(),
            Self::Credentials => {
                Text::source("Enter an HTTP or HTTPS server URL without credentials")
            }
            Self::KeywordTooLong => detail(
                "The search keyword must be at most %1 characters",
                crate::epgstation::MAX_KEYWORD_CHARS,
            ),
            Self::MissingCredentials => Text::source("Enter both a username and password"),
            // These describe external responses, not application input validation.
            Self::Json(_)
            | Self::InvalidCatalogue(_)
            | Self::MissingToken
            | Self::UnexpectedStatus(_) => Text::literal(self),
        }
    }
}

impl PresentError for crate::settings::Error {
    fn present(&self) -> Text {
        match self {
            Self::MissingDirectory => Text::source("The settings directory could not be found"),
            Self::Io { path, source } => Text::message(
                "Settings file operation failed (%1): %2",
                [Text::literal(path.display()), Text::literal(source)],
            ),
            Self::TooLarge => Text::source("The settings file exceeds the 64 KiB limit"),
            Self::Parse { path, source } => Text::message(
                "Could not parse the settings file (%1): %2",
                [Text::literal(path.display()), Text::literal(source)],
            ),
            Self::Serialize(error) => detail("Could not serialize settings: %1", error),
        }
    }
}

impl PresentError for crate::error_log::Error {
    fn present(&self) -> Text {
        match self {
            Self::InvalidDirectory => Text::source("Could not determine an absolute log directory"),
            Self::Io { path, source } => Text::message(
                "Log file operation failed (%1): %2",
                [Text::literal(path.display()), Text::literal(source)],
            ),
        }
    }
}

impl PresentError for crate::diagnostics::Error {
    fn present(&self) -> Text {
        match self {
            Self::AlreadyInitialized => Text::source("Diagnostics are already initialized"),
            Self::NoOwner => Text::source("Diagnostics have no owner"),
            Self::Poisoned => Text::source("The diagnostics lock is poisoned"),
            Self::Recorder(error) => error.present(),
        }
    }
}

impl PresentError for viewer_diagnostics::recorder::Error {
    fn present(&self) -> Text {
        match self {
            Self::Spawn(error) => detail("Could not start the diagnostics worker: %1", error),
            Self::Storage(error) => {
                Text::message("Diagnostic log saving stopped: %1", [error.present()])
            }
            Self::Panicked => Text::source("The diagnostics worker panicked"),
        }
    }
}

impl PresentError for viewer_diagnostics::storage::Error {
    fn present(&self) -> Text {
        match self {
            Self::Io(error) => detail("Diagnostic log operation failed: %1", error),
            Self::Json(error) => detail("Could not serialize the diagnostic record: %1", error),
            Self::RecordTooLarge => Text::source("The diagnostic record exceeds the size limit"),
            Self::ExistingFileTooLarge => {
                Text::source("The existing diagnostic log exceeds the size limit")
            }
        }
    }
}

impl PresentError for crate::remote::Error {
    fn present(&self) -> Text {
        match self {
            Self::Settings(error) => error.present(),
            Self::Directory(error) => error.present(),
            Self::NetworkUnavailable => Text::source("Network runtime is unavailable"),
            Self::AddressInUse { address, .. } => detail("%1: port is already in use", address),
            Self::Stopped => Text::source("Remote control stopped unexpectedly"),
            Self::Io(error) => Text::literal(error),
            Self::Session(error) => match error {
                viewer_remote::ServerError::Transport(error) => {
                    detail("Remote API transport failed: %1", error)
                }
                viewer_remote::ServerError::Task(error) => {
                    detail("Remote API task failed: %1", error)
                }
                viewer_remote::ServerError::ShutdownTimeout => {
                    Text::source("Remote API shutdown timed out")
                }
            },
        }
    }
}

impl PresentError for crate::remote::settings::Error {
    fn present(&self) -> Text {
        match self {
            Self::Port => Text::source("The remote port must be between 1 and 65535"),
            Self::Address => Text::source("The remote address must be an IP address"),
            Self::EnvironmentUnicode(name) => detail("%1 must be Unicode", name),
            Self::CombinedAddress => {
                Text::source("Use an IP-only NAGAMETV_REMOTE_ADDR with NAGAMETV_REMOTE_PORT")
            }
            Self::EnvironmentAddress => Text::source("NAGAMETV_REMOTE_ADDR must be an IP address"),
            Self::EnvironmentPort => {
                Text::source("NAGAMETV_REMOTE_PORT must be between 1 and 65535")
            }
            Self::EnvironmentEnabled => {
                Text::source("NAGAMETV_REMOTE_ENABLED must be 0, 1, false or true")
            }
            Self::TooLarge => Text::source("Remote settings exceed 4 KiB"),
            Self::MissingDirectory => Text::source("The remote settings directory is missing"),
            Self::Parse(error) => detail("Could not parse remote settings: %1", error),
            Self::Io(error) => Text::literal(error),
            Self::Encoding(error) => Text::literal(error),
            Self::Serialize(error) => Text::literal(error),
            Self::Persist(error) => Text::literal(error),
        }
    }
}
