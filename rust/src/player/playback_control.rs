use super::ffi;
use crate::playback::{Playback, PlaybackError, PlaybackEvent};
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::{
    ffi::c_void,
    pin::Pin,
    time::{SystemTime, UNIX_EPOCH},
};
use thiserror::Error;

impl ffi::Player {
    /// # Safety
    /// `item` must be a live QQuickItem owned by the GUI thread. The QML caller
    /// retains the video item for the lifetime of the playback sink.
    pub unsafe fn attach_video_item(mut self: Pin<&mut Self>, item: *mut ffi::QQuickItem) -> bool {
        // SAFETY: the caller guarantees the item lifetime and GUI-thread access.
        let address = unsafe { ffi::q_quick_item_address(item) };
        let result: Result<(), PlayerError> = {
            let mut rust = self.as_mut().rust_mut();
            rust.playback
                .as_mut()
                .ok_or(PlayerError::PlaybackUnavailable)
                .and_then(|playback| {
                    playback
                        .attach_video_item(address as *mut c_void)
                        .map_err(Into::into)
                })
        };
        match result {
            Ok(()) => true,
            Err(error) => {
                self.as_mut().report_playback_error(&error);
                false
            }
        }
    }

    pub fn play(mut self: Pin<&mut Self>) {
        self.as_ref().record_diagnostics("play_requested");
        self.as_mut().rust_mut().subtitle_cue = None;
        self.as_mut().set_subtitle_text(QString::default());
        self.as_mut().set_subtitle_data(QString::default());
        self.as_mut().set_playback_error(QString::default());
        self.as_mut().set_playback_error_details(QString::default());
        let server = self.as_ref().server().to_string();
        let service_id = self.as_ref().service_id().to_string().parse::<u64>();
        let result: Result<(), PlayerError> =
            match (self.as_ref().rust().playback.as_ref(), service_id) {
                (Some(playback), Ok(id)) if id > 0 => {
                    let program = self.as_ref().rust().network.as_ref().and_then(|network| {
                        let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?;
                        let now = u64::try_from(now.as_millis()).ok()?;
                        let snapshot = network.epg().snapshot();
                        let program = snapshot.current_program(id, now)?;
                        Some(crate::audio::AudioProgram {
                            service_id: program.service_id,
                            start_at: program.start_at,
                            audios: program.audios.clone(),
                        })
                    });
                    playback
                        .play_service(&server, id, program)
                        .map_err(Into::into)
                }
                (None, _) => Err(PlayerError::PlaybackUnavailable),
                _ => Err(PlayerError::InvalidServiceId),
            };
        match result {
            Ok(()) => {
                self.as_mut().set_playing(true);
                self.as_mut().set_status(QString::from("Connecting..."));
            }
            Err(error) => self.as_mut().report_playback_error(&error),
        }
    }

    pub fn stop(mut self: Pin<&mut Self>) {
        self.as_ref().record_diagnostics("stop_requested");
        let result = self
            .as_ref()
            .rust()
            .playback
            .as_ref()
            .ok_or(PlayerError::PlaybackUnavailable)
            .and_then(|playback| playback.stop().map_err(Into::into));
        match result {
            Ok(()) => {
                self.as_mut().set_playing(false);
                self.as_mut().set_status(QString::from("Stopped"));
                self.as_mut().rust_mut().subtitle_cue = None;
                self.as_mut().set_subtitle_text(QString::default());
                self.as_mut().set_subtitle_data(QString::default());
                self.as_mut().set_playback_error(QString::default());
                self.as_mut().set_playback_error_details(QString::default());
            }
            Err(error) => self.as_mut().report_playback_error(&error),
        }
    }

    pub(super) fn refresh_audio_state(mut self: Pin<&mut Self>) {
        if let Some((tracks, error)) = self
            .as_ref()
            .rust()
            .playback
            .as_ref()
            .map(Playback::audio_state)
        {
            self.as_mut().set_audio_tracks(QString::from(tracks));
            self.as_mut().set_audio_error(QString::from(error));
        }
    }

    pub fn select_audio_track(mut self: Pin<&mut Self>, key: QString) {
        if let Some(playback) = self.as_ref().rust().playback.as_ref() {
            playback.select_audio_option(&key.to_string());
        }
        self.as_mut().refresh_audio_state();
    }

    pub fn poll_events(mut self: Pin<&mut Self>) {
        // Failed or stopped streams must not overwrite the original diagnostic.
        if *self.as_ref().playing() {
            let event = self
                .as_ref()
                .rust()
                .playback
                .as_ref()
                .ok_or(PlayerError::PlaybackUnavailable)
                .and_then(|playback| playback.drain_events().map_err(Into::into));
            match event {
                Ok(PlaybackEvent::None) => {}
                Ok(PlaybackEvent::Playing) => self.as_mut().set_status(QString::from("Playing")),
                Ok(PlaybackEvent::Ended) => self
                    .as_mut()
                    .report_playback_error(&PlaybackError::StreamEnded.into()),
                Err(error) => self.as_mut().report_playback_error(&error),
            }
        }
        self.as_mut().refresh_audio_state();
        let volume = if *self.as_ref().audio_muted() {
            0.0
        } else {
            *self.as_ref().volume()
        };
        if (volume - self.as_ref().rust().applied_volume).abs() > f64::EPSILON {
            if let Some(playback) = self.as_ref().rust().playback.as_ref() {
                playback.set_volume(volume);
            }
            self.as_mut().rust_mut().applied_volume = volume;
        }
        let (start, duration) = {
            let player = self.as_ref();
            let rust = player.rust();
            (rust.current_program_start, rust.current_program_duration)
        };
        let progress = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .filter(|_| start > 0 && duration > 0)
            .map(|now| now.as_millis().saturating_sub(u128::from(start)) as f64 / duration as f64)
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        if (progress - *self.as_ref().program_progress()).abs() > 0.000_01 {
            self.as_mut().set_program_progress(progress);
        }
        self.as_mut().poll_comments();
        self.as_mut().poll_epg_events();
    }
}

#[derive(Debug, Error)]
pub(super) enum PlayerError {
    #[error(transparent)]
    Playback(#[from] PlaybackError),
    #[error("Could not initialize the player")]
    PlaybackUnavailable,
    #[error("Enter a valid Mirakurun service ID")]
    InvalidServiceId,
}
