//! One owner for native playback and its subtitle generation. Only a successful
//! stop lends the capability to install a replacement subtitle session/stream.
use super::{Error, Playback, Result};
use crate::{channels::BroadcastService, features::subtitles};

pub struct Session {
    playback: Option<Playback>,
    subtitles: Option<subtitles::Session>,
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
        Ok(Stopped(self))
    }
    pub fn shutdown(&mut self) -> Result<()> {
        if let Some(playback) = &mut self.playback {
            playback.shutdown()?;
        }
        self.subtitles = None;
        Ok(())
    }
    pub fn shutdown_before_drop(&mut self) {
        if let Some(playback) = &mut self.playback {
            playback.shutdown_before_drop();
        }
        self.subtitles = None;
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
        )
    }

    pub fn start_file(
        self,
        file: &super::recording::Recording,
        subtitles_enabled: bool,
    ) -> Result<SubtitleStart> {
        self.start_uri(
            file.uri(),
            Some(file.service()),
            |element| subtitles::Session::start_recording(element, file.service()),
            subtitles_enabled,
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
