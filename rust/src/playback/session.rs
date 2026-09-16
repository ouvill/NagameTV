//! One owner for native playback and its subtitle generation. Only a successful
//! stop lends the capability to install a replacement subtitle session/stream.
use super::{Error, Playback, Result};
use crate::{channels::BroadcastService, features::subtitles};

pub struct Session {
    playback: Option<Playback>,
    subtitles: Option<subtitles::Session>,
    input: Input,
}

enum Input {
    Idle,
    Live,
    Recording(super::timeline::Controller),
}

/// Exclusive access to a recording's own pipeline, never a live source.
pub struct RecordingControl<'a> {
    playback: &'a Playback,
    controller: &'a mut super::timeline::Controller,
}

impl RecordingControl<'_> {
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
        self,
        resume: super::timeline::Resume,
    ) -> std::result::Result<(), super::timeline::Error> {
        self.controller.pause(self.playback.element(), resume)
    }
}

/// Exclusive, non-cloneable evidence that this owner's previous stream stopped.
pub struct Stopped<'a>(&'a mut Session);

pub enum SubtitleStart {
    Disabled,
    Parsing,
    Failed(subtitles::Error),
}

impl Session {
    pub fn new(playback: Option<Playback>) -> Self {
        Self {
            playback,
            subtitles: None,
            input: Input::Idle,
        }
    }
    // Native stream-control methods are private to the playback module. This
    // borrow permits statistics/audio operations, never start/stop/attachment.
    pub fn playback(&self) -> Option<&Playback> {
        self.playback.as_ref()
    }
    pub fn subtitles(&self) -> Option<&subtitles::Session> {
        self.subtitles.as_ref()
    }

    pub fn recording_control(
        &mut self,
    ) -> std::result::Result<RecordingControl<'_>, super::timeline::Error> {
        match (&self.playback, &mut self.input) {
            (Some(playback), Input::Recording(controller)) => Ok(RecordingControl {
                playback,
                controller,
            }),
            _ => Err(super::timeline::Error::Unavailable),
        }
    }

    pub fn timeline(&self) -> Option<(super::timeline::Phase, super::timeline::Snapshot)> {
        match &self.input {
            Input::Recording(controller) => Some((controller.phase(), controller.snapshot())),
            Input::Idle | Input::Live => None,
        }
    }

    pub fn poll(&mut self) -> Result<super::Event> {
        let Some(playback) = &self.playback else {
            return Ok(super::Event::Idle);
        };
        let mut event = playback.poll()?;
        if let Input::Recording(controller) = &mut self.input {
            if let super::Event::Ended(sequence) = event
                && !controller.ended(sequence)?
            {
                event = super::Event::Idle;
            }
            controller.poll(playback.element())?;
        }
        Ok(event)
    }

    /// # Safety
    /// Same live-item, GUI-thread and shutdown-before-item-destruction contract
    /// as Playback::attach. Ownership of the Qt item does not transfer.
    pub unsafe fn attach(&mut self, item: *mut crate::player::ffi::QQuickItem) -> Result<()> {
        let playback = self.playback.as_mut().ok_or(Error::Unavailable)?;
        // SAFETY: The caller supplies the item lifetime/thread guarantees above.
        unsafe { playback.attach(item) }
    }

    pub fn stop(&mut self) -> Result<Stopped<'_>> {
        self.stop_with(Playback::stop)
    }
    fn stop_with(&mut self, stop: impl FnOnce(&Playback) -> Result<()>) -> Result<Stopped<'_>> {
        if let Some(playback) = &self.playback {
            stop(playback)?;
        }
        // A failed native transition cannot reach either resource release or
        // construction of Stopped. The caller retains this owner for retry.
        self.subtitles = None;
        self.input = Input::Idle;
        Ok(Stopped(self))
    }
    pub fn shutdown(&mut self) -> Result<()> {
        if let Some(playback) = &mut self.playback {
            playback.shutdown()?;
        }
        self.subtitles = None;
        self.input = Input::Idle;
        Ok(())
    }
    pub fn shutdown_before_drop(&mut self) {
        if let Some(playback) = &mut self.playback {
            playback.shutdown_before_drop();
        }
        self.subtitles = None;
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
    ) -> Result<SubtitleStart> {
        let uri = format!("{server}/api/services/{service}/stream");
        self.start_uri(
            &uri,
            broadcast.map(|s| s.service_id),
            |element| subtitles::Session::start(element, broadcast),
            subtitles_enabled,
            Input::Live,
        )
    }

    pub fn start_file(
        self,
        file: &super::recording::Recording,
        subtitles_enabled: bool,
        programs_enabled: bool,
    ) -> Result<SubtitleStart> {
        let playback = self.0.playback.as_ref().ok_or(Error::Unavailable)?;
        let input = Input::Recording(super::timeline::Controller::new(&playback.sink)?);
        self.start_uri(
            file.uri(),
            Some(file.service()),
            |element| {
                subtitles::Session::start_recording(
                    element,
                    file.service(),
                    subtitles_enabled,
                    programs_enabled,
                )
            },
            subtitles_enabled || programs_enabled,
            input,
        )
    }

    fn start_uri(
        self,
        uri: &str,
        service: Option<u16>,
        start_subtitles: impl FnOnce(
            &gstreamer::Element,
        )
            -> std::result::Result<subtitles::Session, subtitles::Error>,
        subtitles_enabled: bool,
        input: Input,
    ) -> Result<SubtitleStart> {
        let playback = self.0.playback.as_ref().ok_or(Error::Unavailable)?;
        let subtitles = if subtitles_enabled {
            match start_subtitles(playback.element()) {
                Ok(session) => {
                    self.0.subtitles = Some(session);
                    SubtitleStart::Parsing
                }
                Err(error) => SubtitleStart::Failed(error),
            }
        } else {
            SubtitleStart::Disabled
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
        subtitles: Some(subtitles),
        input: Input::Live,
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
