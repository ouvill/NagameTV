//! Translate Qt requests into owned inspections; commit only current results.
use super::{ffi, stream_state::Attempt};
use crate::playback::recording::{Error, Purpose, Request};
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QUrl};
use std::{path::PathBuf, pin::Pin};

impl ffi::Player {
    pub fn recording_loading(&self) -> bool {
        self.rust().recording_loader.loading()
    }

    /// True means accepted for inspection. recordingOpened reports completion.
    pub fn open_recording(mut self: Pin<&mut Self>, url: QUrl) -> bool {
        let Some(path) = url.to_local_file().filter(|path| !path.is_empty()) else {
            self.set_file_error(super::status::with_detail(
                "Could not open the TS file: %1",
                Error::NotLocal,
            ));
            return false;
        };
        self.as_mut().rust_mut().autoplay_pending = false;
        self.as_mut().set_file_error(QString::default());
        self.begin_recording(Request::open(PathBuf::from(path.to_string())));
        true
    }
    pub(super) fn begin_recording(mut self: Pin<&mut Self>, request: Request) {
        let before = self.recording_loading();
        self.as_mut().rust_mut().recording_loader.begin(request);
        if before != self.recording_loading() {
            self.recording_loading_changed();
        }
    }
    pub fn cancel_recording_open(mut self: Pin<&mut Self>) {
        let before = self.recording_loading();
        self.as_mut().rust_mut().recording_loader.cancel();
        if before != self.recording_loading() {
            self.recording_loading_changed();
        }
    }
    pub(super) fn poll_recording(mut self: Pin<&mut Self>) {
        let before = self.recording_loading();
        let outcome = self.as_mut().rust_mut().recording_loader.poll();
        // Process the result before notifying completion. A signal handler can
        // request a new file; it must never be cancelled by this older result.
        let completed = outcome.map(|(purpose, result)| {
            let success = match result {
                Ok(file) => {
                    self.as_mut().clear_playback_failure();
                    self.as_mut().start_stream(Attempt::File(file))
                }
                Err(error) => {
                    match purpose {
                        Purpose::Open => self.as_mut().set_file_error(super::status::with_detail(
                            "Could not open the TS file: %1",
                            error,
                        )),
                        Purpose::Replay => self.as_mut().playback_failed(error.into()),
                    }
                    false
                }
            };
            (purpose, success)
        });
        if before != self.recording_loading() {
            self.as_mut().recording_loading_changed();
        }
        if let Some((Purpose::Open, success)) = completed {
            self.recording_opened(success);
        }
    }
}
