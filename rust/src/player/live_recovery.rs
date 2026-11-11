//! Project the Rust policy and execute its one-use same-channel restart permission.
use super::{ffi, status::tr};
use crate::playback::{
    self,
    live_recovery::{Action, Status},
};
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::{pin::Pin, time::Instant};

impl ffi::Player {
    pub fn live_recovery_state(&self) -> QString {
        QString::from(self.rust().live_recovery.status().as_str())
    }
    pub fn live_recovery_message(&self) -> QString {
        match self.rust().live_recovery.status() {
            Status::Idle => QString::default(),
            Status::Recovering => tr("Video or audio stopped. Reconnecting to the same channel…"),
            Status::Failed => tr("Could not restore video or audio. Please retry."),
        }
    }
    pub(super) fn reset_live_recovery(mut self: Pin<&mut Self>) {
        let before = self.rust().live_recovery.status();
        self.as_mut().rust_mut().live_recovery.reset();
        self.as_mut().rust_mut().live_recovery_target = None;
        if before != Status::Idle {
            self.live_recovery_changed();
        }
    }
    #[cfg(feature = "native_tests")]
    pub(super) fn fail_live_recovery(mut self: Pin<&mut Self>) {
        let before = self.rust().live_recovery.status();
        self.as_mut().rust_mut().live_recovery.fail();
        if before != Status::Failed {
            self.live_recovery_changed();
        }
    }
    pub fn retry_live_recovery(mut self: Pin<&mut Self>) {
        if self.rust().live_recovery.status() != Status::Failed {
            return;
        }
        if let Some(target) = self.as_mut().rust_mut().live_recovery_target.take() {
            self.as_mut().clear_playback_failure();
            self.start_stream(target.manual_retry());
        }
    }
    pub(super) fn poll_live_recovery(mut self: Pin<&mut Self>) {
        let observation = match self.rust().media.live_recovery_observation() {
            Ok(observation) => observation,
            Err(error) => {
                let error = match self.as_mut().release_stream() {
                    Ok(()) => error,
                    Err(cleanup) => playback::Error::Cleanup {
                        primary: Box::new(error),
                        cleanup: Box::new(cleanup),
                    },
                };
                self.playback_failed(error);
                return;
            }
        };
        self.apply_live_recovery(observation, Instant::now());
    }
    pub(super) fn apply_live_recovery(
        mut self: Pin<&mut Self>,
        observation: playback::live_recovery::Observation,
        now: Instant,
    ) {
        let before = self.rust().live_recovery.status();
        let action = self
            .as_mut()
            .rust_mut()
            .live_recovery
            .observe(now, observation);
        let after = self.rust().live_recovery.status();
        if before == Status::Idle && after != Status::Idle {
            let target = self.rust().stream_state.recovery_target();
            self.as_mut().rust_mut().live_recovery_target = target;
            match self.rust().media.live_recovery_diagnostic() {
                Ok(diagnostic) => {
                    tracing::warn!(operation = "detect live output stall",
                        input_output_time_ns = ?observation.times(), ?observation, %diagnostic, "Live input is advancing without output")
                }
                Err(error) => tracing::error!(
                    operation = "collect live stall diagnostic",
                    error = &error as &dyn std::error::Error,
                    "Could not collect live input diagnostic"
                ),
            }
        }
        if before != after {
            self.as_mut().live_recovery_changed();
        }
        // A direct Qt notification can accept stop/selection/retry synchronously.
        // Never apply an action from the cancelled observation to that new request.
        if self.rust().live_recovery.status() != after {
            return;
        }
        match action {
            Action::None => {}
            Action::Restart(permit) => {
                tracing::warn!(
                    operation = "restart stalled live playback",
                    attempt = permit.attempt(),
                    ?observation,
                    "Reopening the same live channel"
                );
                let Some(target) = self.rust().live_recovery_target.clone() else {
                    self.playback_failed(playback::Error::LiveRecoveryFailed);
                    return;
                };
                // stop() on Session must finish before a replacement input is installed.
                // release_stream removes the requested identity so continuation cannot no-op.
                if let Err(error) = self.as_mut().release_stream() {
                    self.playback_failed(error);
                } else if self.rust().live_recovery.status() == Status::Recovering
                    && self.rust().media.source_identity().is_none()
                {
                    self.continue_stream(target.restart(permit));
                }
            }
            Action::Failed => {
                tracing::error!(
                    operation = "recover live output",
                    ?observation,
                    "Live recovery deadline or retry limit reached"
                );
                let error = match self.as_mut().release_stream() {
                    Ok(()) => playback::Error::LiveRecoveryFailed,
                    Err(cleanup) => playback::Error::Cleanup {
                        primary: Box::new(playback::Error::LiveRecoveryFailed),
                        cleanup: Box::new(cleanup),
                    },
                };
                self.playback_failed(error);
            }
        }
    }
}
