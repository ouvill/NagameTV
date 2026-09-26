//! One owner for native playback and its subtitle generation. Only a successful
//! stop lends the capability to install a replacement subtitle session/stream.
use super::{Error, Playback, Result};
use crate::{channels::BroadcastService, features::subtitles};

pub struct Session {
    playback: Option<Playback>,
    subtitles: SubtitleSession,
    input: Input,
    live_buffer: crate::settings::LiveBuffer,
}

enum SubtitleSession {
    Disabled,
    Broadcast(subtitles::Session),
    Media(Box<crate::media_subtitles::Session>),
}

enum Input {
    Idle,
    Media {
        source: super::media::Input,
        identity: u64,
        broadcast: Option<super::recording::Broadcast>,
        controller: Box<super::timeline::Controller>,
    },
    Active {
        source: super::input::Input,
        controller: Box<super::timeline::Controller>,
        projection: Projection,
    },
}

impl Input {
    fn suspend(&self, suspended: bool) {
        match self {
            Self::Idle => {}
            Self::Active { source, .. } => source.suspend(suspended),
            Self::Media { source, .. } => source.suspend(suspended),
        }
    }
}

enum Projection {
    Recording,
    Live(Box<super::live_timeline::Presenter>),
}

/// Exclusive access to this session's playback cursor; reception has a separate owner.
pub struct TransportControl<'a> {
    playback: &'a Playback,
    controller: &'a mut super::timeline::Controller,
    source: Option<&'a mut super::input::Input>,
}

impl TransportControl<'_> {
    pub fn set_rate(
        self,
        rate: super::speed::Rate,
    ) -> std::result::Result<(), super::timeline::Error> {
        if self.playback.tempo.is_none() {
            return Err(super::timeline::Error::RateUnavailable);
        }
        self.controller
            .prepare(self.playback.element())?
            .set_rate(rate)
    }

    pub fn seek(self, milliseconds: f64) -> std::result::Result<(), super::timeline::Error> {
        self.controller
            .prepare(self.playback.element())?
            .seek(milliseconds)
    }
    pub fn skip(self, milliseconds: f64) -> std::result::Result<(), super::timeline::Error> {
        let target = self.controller.relative_target(milliseconds)?;
        self.seek(target)
    }
    pub fn resume(
        mut self,
        resume: super::timeline::Resume,
    ) -> std::result::Result<(), super::timeline::Error> {
        use gstreamer::prelude::*;
        let started = if resume == super::timeline::Resume::Paused {
            if let Some(source) = &mut self.source {
                let position = self
                    .playback
                    .element()
                    .query_position::<gstreamer::ClockTime>()
                    .ok_or(super::timeline::Error::Unavailable)?;
                source.begin_pause(position.nseconds())?
            } else {
                false
            }
        } else {
            false
        };
        if let Err(error) = self.controller.pause(self.playback.element(), resume) {
            if started
                && let Some(source) = self.source
                && let Err(cleanup) = source.finish_pause()
            {
                tracing::error!(
                    error = &cleanup as &dyn std::error::Error,
                    "Could not release history after rejected pause"
                );
            }
            return Err(error);
        }
        Ok(())
    }
}

/// Exclusive, non-cloneable evidence that this owner's previous stream stopped.
pub struct Stopped<'a>(&'a mut Session);

pub enum SubtitleStart {
    Disabled,
    Parsing,
    Failed(subtitles::Error),
}

// A media file cannot accidentally start a transport-stream subtitle parser.
enum SubtitleInput {
    Disabled,
    Live(Option<BroadcastService>),
    Recording(u16),
}
impl SubtitleInput {
    fn start(
        self,
        element: &gstreamer::Element,
    ) -> Option<std::result::Result<subtitles::Session, subtitles::Error>> {
        match self {
            Self::Disabled => None,
            Self::Live(broadcast) => Some(subtitles::Session::start(element, broadcast)),
            Self::Recording(service) => {
                Some(subtitles::Session::start_recording(element, service, true))
            }
        }
    }
}

impl Session {
    pub fn new(playback: Option<Playback>, live_buffer: crate::settings::LiveBuffer) -> Self {
        Self {
            playback,
            subtitles: SubtitleSession::Disabled,
            input: Input::Idle,
            live_buffer,
        }
    }
    pub fn configure_live_buffer(&mut self, buffer: crate::settings::LiveBuffer) {
        self.live_buffer = buffer;
        if let Input::Active {
            controller,
            projection: Projection::Live(_),
            ..
        } = &mut self.input
        {
            controller.configure_live_buffer(buffer);
        }
    }
    // Native stream-control methods are private to the playback module. This
    // borrow permits statistics/audio operations, never start/stop/attachment.
    pub fn playback(&self) -> Option<&Playback> {
        self.playback.as_ref()
    }
    pub fn media_subtitles(&self) -> Option<&crate::media_subtitles::Session> {
        match &self.subtitles {
            SubtitleSession::Media(session) => Some(session),
            SubtitleSession::Disabled | SubtitleSession::Broadcast(_) => None,
        }
    }
    pub fn media_subtitles_mut(&mut self) -> Option<&mut crate::media_subtitles::Session> {
        match &mut self.subtitles {
            SubtitleSession::Media(session) => Some(session),
            SubtitleSession::Disabled | SubtitleSession::Broadcast(_) => None,
        }
    }
    pub fn subtitles(&self) -> Option<&subtitles::Session> {
        match &self.subtitles {
            SubtitleSession::Broadcast(session) => Some(session),
            SubtitleSession::Disabled | SubtitleSession::Media(_) => None,
        }
    }

    pub fn transport_control(
        &mut self,
    ) -> std::result::Result<TransportControl<'_>, super::timeline::Error> {
        match (&self.playback, &mut self.input) {
            (
                Some(playback),
                Input::Active {
                    controller, source, ..
                },
            ) => Ok(TransportControl {
                playback,
                controller,
                source: Some(source),
            }),
            (Some(playback), Input::Media { controller, .. }) => Ok(TransportControl {
                playback,
                controller,
                source: None,
            }),
            _ => Err(super::timeline::Error::Unavailable),
        }
    }

    pub fn take_notice(&mut self) -> Option<super::timeline::Notice> {
        match &mut self.input {
            Input::Active { controller, .. } => controller.take_notice(),
            Input::Idle | Input::Media { .. } => None,
        }
    }
    pub fn metadata(
        &self,
        position: gstreamer::ClockTime,
    ) -> crate::transport::programs::catalog::View {
        match &self.input {
            Input::Active { source, .. } => source.metadata(position.nseconds()),
            Input::Media {
                broadcast,
                controller,
                ..
            } => broadcast
                .as_ref()
                .map(|b| b.view(controller.snapshot().duration))
                .unwrap_or_default(),
            Input::Idle => Default::default(),
        }
    }
    pub fn source_identity(&self) -> Option<u64> {
        match &self.input {
            Input::Active { source, .. } => Some(source.identity()),
            Input::Media {
                identity,
                broadcast: Some(_),
                ..
            } => Some(*identity),
            Input::Idle
            | Input::Media {
                broadcast: None, ..
            } => None,
        }
    }
    pub fn comment_source(
        &self,
        fallback: Option<BroadcastService>,
        channel: impl Fn(BroadcastService) -> Option<u16>,
        reception: viewer_comments::cache::Reception,
    ) -> Option<viewer_comments::cache::Source> {
        match &self.input {
            Input::Active {
                source, controller, ..
            } => source.comment_source(
                fallback,
                channel,
                controller.snapshot().position.map(|p| p.nseconds()),
                reception,
            ),
            Input::Media {
                broadcast: Some(broadcast),
                controller,
                ..
            } => Some(viewer_comments::cache::Source::Recording(
                broadcast.comments(controller.snapshot().duration),
            )),
            Input::Idle
            | Input::Media {
                broadcast: None, ..
            } => None,
        }
    }
    /// Audio follows the same sampled output position as transport, including pause.
    /// A pending seek has no confirmed destination to which routing can be applied.
    pub(crate) fn audio_metadata(&self) -> Option<crate::audio::Metadata> {
        match &self.input {
            Input::Active {
                source, controller, ..
            } => match controller.phase() {
                super::timeline::Phase::Seeking(_) => None,
                super::timeline::Phase::Playing
                | super::timeline::Phase::Paused
                | super::timeline::Phase::Ended => {
                    let position = controller.snapshot().position?;
                    let view = source.metadata(position.nseconds());
                    Some(crate::audio::Metadata::from_ts(
                        source.identity(),
                        view.program?,
                    ))
                }
            },
            Input::Idle | Input::Media { .. } => None,
        }
    }
    pub fn speed(&self) -> super::speed::Snapshot {
        match &self.input {
            Input::Idle => super::speed::Snapshot::default(),
            Input::Active { controller, .. } | Input::Media { controller, .. } => {
                let mut snapshot = controller.speed();
                if self
                    .playback
                    .as_ref()
                    .is_none_or(|playback| playback.tempo.is_none())
                {
                    snapshot.availability = super::speed::Availability::MissingTempo;
                }
                snapshot
            }
        }
    }

    pub fn timeline(&self) -> Option<(super::timeline::Phase, super::timeline::Snapshot)> {
        match &self.input {
            Input::Active { controller, .. } | Input::Media { controller, .. } => {
                Some((controller.phase(), controller.snapshot()))
            }
            Input::Idle => None,
        }
    }
    pub fn timeshift_bytes_per_second(&self) -> Option<f64> {
        match &self.input {
            Input::Active { source, .. } => source.bytes_per_second(),
            Input::Idle | Input::Media { .. } => None,
        }
    }
    pub fn configure_timeshift(&mut self, policy: super::input::Policy) -> Result<()> {
        match &mut self.input {
            Input::Active {
                source,
                controller,
                projection: Projection::Live(_),
            } => {
                if let Some(change) = source.reconfigure(policy)? {
                    controller.retention_changed(change);
                }
            }
            Input::Idle
            | Input::Media { .. }
            | Input::Active {
                projection: Projection::Recording,
                ..
            } => {}
        }
        Ok(())
    }
    pub fn live_timeline(&mut self) -> Option<super::live_timeline::Snapshot> {
        match &mut self.input {
            Input::Active {
                source,
                controller,
                projection: Projection::Live(presenter),
            } => source.live_timeline(
                presenter,
                controller.phase(),
                controller.snapshot().position,
                controller.seek_target(),
            ),
            Input::Active {
                projection: Projection::Recording,
                ..
            }
            | Input::Idle
            | Input::Media { .. } => None,
        }
    }
    pub fn timeline_preview(&self, session: &str, milliseconds: f64) -> String {
        match &self.input {
            Input::Active {
                projection: Projection::Live(presenter),
                ..
            } => presenter.preview(session, milliseconds),
            Input::Active {
                projection: Projection::Recording,
                ..
            }
            | Input::Idle
            | Input::Media { .. } => "null".into(),
        }
    }
    pub fn seek_timeline(
        &mut self,
        session: &str,
        milliseconds: f64,
    ) -> std::result::Result<(), super::timeline::Error> {
        let playback = self
            .playback
            .as_ref()
            .ok_or(super::timeline::Error::Unavailable)?;
        match &mut self.input {
            Input::Active {
                source,
                controller,
                projection: Projection::Live(presenter),
            } => {
                if !presenter.owns(session) {
                    return Err(super::timeline::Error::Unavailable);
                }
                let target = source.live_seek_target(milliseconds)?;
                target.seek(controller.prepare(playback.element())?)
            }
            Input::Idle
            | Input::Media { .. }
            | Input::Active {
                projection: Projection::Recording,
                ..
            } => Err(super::timeline::Error::Unavailable),
        }
    }

    pub fn return_to_live(&mut self) -> std::result::Result<(), super::timeline::Error> {
        match (&self.playback, &mut self.input) {
            (
                Some(playback),
                Input::Active {
                    source,
                    controller,
                    projection: Projection::Live(_),
                },
            ) if matches!(
                source.live_window(),
                Some(super::timeline::LiveWindow::History(_))
            ) =>
            {
                controller.prepare(playback.element())?.return_to_live()
            }
            _ => Err(super::timeline::Error::Unavailable),
        }
    }

    pub fn poll(&mut self) -> Result<super::Event> {
        let Some(playback) = &self.playback else {
            return Ok(super::Event::Idle);
        };
        if let Input::Active { source, .. } = &self.input {
            source.check()?;
        }
        let mut event = playback.poll()?;
        let controller = match &mut self.input {
            Input::Idle => None,
            Input::Media { controller, .. } => Some(controller),
            Input::Active {
                controller, source, ..
            } => {
                controller.set_estimated(source.duration_estimated());
                Some(controller)
            }
        };
        let mut arrival = super::timeline::Arrival::None;
        if let Some(controller) = controller {
            if let super::Event::Ended(sequence) = event
                && !controller.ended(sequence)?
            {
                event = super::Event::Idle;
            }
            arrival = controller.poll(playback.element())?;
        }
        if arrival == super::timeline::Arrival::Live
            && let Input::Active { source, .. } = &mut self.input
        {
            source.finish_pause()?;
        }
        if let Input::Active {
            controller, source, ..
        } = &mut self.input
            && let Some(window) = source.live_window()
        {
            controller.retained(
                playback.element(),
                window,
                source.take_expired(),
                source.read_progress()?,
            )?;
        }
        Ok(event)
    }

    /// # Safety
    /// Same live-item, GUI-thread and shutdown-before-item-destruction contract
    /// as Playback::attach. Ownership of the Qt item does not transfer.
    pub unsafe fn attach(&mut self, item: *mut crate::qt::ffi::QQuickItem) -> Result<()> {
        let playback = self.playback.as_mut().ok_or(Error::Unavailable)?;
        // SAFETY: The caller supplies the item lifetime/thread guarantees above.
        unsafe { playback.attach(item) }
    }

    pub fn stop(&mut self) -> Result<Stopped<'_>> {
        self.stop_with(Playback::stop)
    }
    fn stop_with(&mut self, stop: impl FnOnce(&Playback) -> Result<()>) -> Result<Stopped<'_>> {
        self.input.suspend(true);
        if let Some(playback) = &self.playback
            && let Err(error) = stop(playback)
        {
            self.input.suspend(false);
            return Err(error);
        }
        // A failed native transition cannot reach either resource release or
        // construction of Stopped. The caller retains this owner for retry.
        self.subtitles = SubtitleSession::Disabled;
        self.input = Input::Idle;
        Ok(Stopped(self))
    }
    pub fn shutdown(&mut self) -> Result<()> {
        self.input.suspend(true);
        if let Some(playback) = &mut self.playback {
            playback.shutdown()?;
        }
        self.subtitles = SubtitleSession::Disabled;
        self.input = Input::Idle;
        Ok(())
    }
    pub fn shutdown_before_drop(&mut self) {
        self.input.suspend(true);
        if let Some(playback) = &mut self.playback {
            playback.shutdown_before_drop();
        }
        self.subtitles = SubtitleSession::Disabled;
        self.input = Input::Idle;
    }
}

impl Stopped<'_> {
    pub fn start(
        self,
        server: &str,
        service: u64,
        broadcast: Option<BroadcastService>,
        subtitles_enabled: bool,
        programs_enabled: bool,
        retention: super::input::Policy,
    ) -> Result<SubtitleStart> {
        let uri = format!("{server}/api/services/{service}/stream");
        let playback = self.0.playback.as_ref().ok_or(Error::Unavailable)?;
        let service = broadcast.map_or(0, |service| service.service_id);
        let input = Input::Active {
            source: super::input::Input::live(
                playback.element(),
                &uri,
                service,
                retention,
                programs_enabled,
            )?,
            controller: Box::new(super::timeline::Controller::new(
                &playback.sink,
                super::timeline::StartPosition::LiveEdge(self.0.live_buffer),
            )?),
            projection: Projection::Live(Box::new(super::live_timeline::Presenter::new())),
        };
        self.start_uri(
            "appsrc://",
            (service != 0).then_some(service),
            if subtitles_enabled {
                SubtitleInput::Live(broadcast)
            } else {
                SubtitleInput::Disabled
            },
            input,
        )
    }

    pub fn start_file(
        self,
        file: &super::recording::Recording,
        subtitles_enabled: bool,
        programs_enabled: bool,
    ) -> Result<SubtitleStart> {
        match file {
            super::recording::Recording::Transport(file) => {
                self.start_transport(file, subtitles_enabled, programs_enabled)
            }
            super::recording::Recording::Media(file) => {
                let playback = self.0.playback.as_ref().ok_or(Error::Unavailable)?;
                let input = Input::Media {
                    source: super::media::Input::new(playback.element(), file),
                    identity: super::next_source_identity(),
                    broadcast: file.broadcast().cloned(),
                    controller: Box::new(super::timeline::Controller::new(
                        &playback.sink,
                        super::timeline::StartPosition::Beginning,
                    )?),
                };
                if subtitles_enabled {
                    self.0.subtitles =
                        SubtitleSession::Media(Box::new(crate::media_subtitles::Session::start(
                            playback.element(),
                            file.external_subtitle().cloned(),
                        )?));
                }
                self.start_uri("appsrc://", None, SubtitleInput::Disabled, input)
            }
        }
    }

    fn start_transport(
        self,
        file: &super::recording::TransportStream,
        subtitles_enabled: bool,
        programs_enabled: bool,
    ) -> Result<SubtitleStart> {
        let playback = self.0.playback.as_ref().ok_or(Error::Unavailable)?;
        let input = Input::Active {
            source: super::input::Input::recording(playback.element(), file, programs_enabled)?,
            controller: Box::new(super::timeline::Controller::new(
                &playback.sink,
                super::timeline::StartPosition::Beginning,
            )?),
            projection: Projection::Recording,
        };
        self.start_uri(
            "appsrc://",
            Some(file.service()),
            if subtitles_enabled {
                SubtitleInput::Recording(file.service())
            } else {
                SubtitleInput::Disabled
            },
            input,
        )
    }

    fn start_uri(
        self,
        uri: &str,
        service: Option<u16>,
        subtitle_input: SubtitleInput,
        input: Input,
    ) -> Result<SubtitleStart> {
        let playback = self.0.playback.as_ref().ok_or(Error::Unavailable)?;
        use gstreamer::prelude::*;
        if self.0.media_subtitles().is_some() {
            playback.element().set_property_from_str(
                "flags",
                "video+audio+text+soft-volume+buffering+native-video",
            );
        } else {
            playback
                .element()
                .set_property_from_str("flags", "video+audio+soft-volume+buffering+native-video");
        }
        let subtitles = match subtitle_input.start(playback.element()) {
            None => SubtitleStart::Disabled,
            Some(Ok(session)) => {
                self.0.subtitles = SubtitleSession::Broadcast(session);
                SubtitleStart::Parsing
            }
            Some(Err(error)) => SubtitleStart::Failed(error),
        };
        self.0.input = input;
        if let Err(error) = playback.play(uri, service) {
            let failure = match self.0.stop() {
                Ok(_) => error,
                Err(cleanup) => Error::Cleanup {
                    primary: Box::new(error),
                    cleanup: Box::new(cleanup),
                },
            };
            return Err(failure);
        }
        Ok(subtitles)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // Preserve the existing final-destruction policy: shutdown must complete
        // before subtitle Drop disconnects callbacks or the native graph drops.
        self.shutdown_before_drop();
    }
}

#[cfg(feature = "video_item_tests")]
pub(super) fn check_stop_ownership() {
    // Install a real subtitle generation while the graph is still in NULL,
    // as production does before PLAYING. No stream or audio device is opened.
    let playback = Playback::new().expect("native playback");
    let subtitles = subtitles::Session::start(
        playback.element(),
        Some(BroadcastService {
            network_id: 1,
            service_id: 42,
        }),
    )
    .expect("subtitle generation");
    let mut session = Session {
        playback: Some(playback),
        subtitles: SubtitleSession::Broadcast(subtitles),
        input: Input::Idle,
        live_buffer: Default::default(),
    };
    let before = session.subtitles().expect("subtitle generation").counters();
    assert!(before.0 > 0);
    // Inject a native stop failure without a broken device. This is the same
    // boundary used by production stop; no resource is replaced by a test stub.
    assert!(session.stop_with(|_| Err(Error::OutputNotReady)).is_err());
    assert_eq!(
        session
            .subtitles()
            .expect("retained after failure")
            .counters()
            .0,
        before.0
    );
    session.stop().expect("retry stop");
    assert!(session.subtitles().is_none());
    session.shutdown().expect("final shutdown");
}
