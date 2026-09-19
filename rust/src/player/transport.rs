//! Publish transport controls as one coherent Qt snapshot.
use super::{ffi, status::tr};
use crate::playback::{
    self,
    timeline::{Error, Notice, Resume},
};
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::{
    pin::Pin,
    time::{Duration, Instant},
};

// Match the six-second reading time used by screenshot notices.
pub(super) const NOTICE_DURATION: Duration = Duration::from_secs(6);

#[derive(PartialEq, Eq)]
pub(super) struct TimedNotice {
    kind: Notice,
    deadline: Instant,
}

#[derive(PartialEq, Eq)]
pub(super) enum Message {
    None,
    Notice(TimedNotice),
    Failure(QString),
}

impl Message {
    pub(super) fn notice(kind: Notice, now: Instant) -> Self {
        Self::Notice(TimedNotice {
            kind,
            deadline: now + NOTICE_DURATION,
        })
    }

    fn expire(&mut self, now: Instant) -> bool {
        match self {
            Self::Notice(notice) if now >= notice.deadline => {
                *self = Self::None;
                true
            }
            Self::None | Self::Notice(_) | Self::Failure(_) => false,
        }
    }

    fn render(&self) -> QString {
        match self {
            Self::None => QString::default(),
            Self::Notice(notice) => tr(match notice.kind {
                Notice::Expired => {
                    "The playback position expired and was moved into the retained range."
                }
                Notice::SettingsClamped => {
                    "Timeshift settings changed. The playback position was moved into the retained range."
                }
                Notice::SettingsReturnedToLive => {
                    "Timeshift settings changed. Playback returned to the live edge."
                }
            }),
            // Diagnostics are literal data, never translation keys.
            Self::Failure(detail) => detail.clone(),
        }
    }
}

impl ffi::Player {
    pub fn transport_error(&self) -> QString {
        self.rust().transport_message.render()
    }

    pub(super) fn expire_transport_notice(mut self: Pin<&mut Self>, now: Instant) {
        if self.as_mut().rust_mut().transport_message.expire(now) {
            self.as_mut().transport_error_changed();
        }
    }

    pub fn timeshift(&self) -> bool {
        self.rust().stream_state.timeshift()
    }
    pub fn window_start_ms(&self) -> f64 {
        self.rust()
            .timeline
            .range
            .map_or(0.0, |range| range.start().mseconds() as f64)
    }
    pub fn window_end_ms(&self) -> f64 {
        self.rust()
            .timeline
            .range
            .map_or(-1.0, |range| range.end().mseconds() as f64)
    }
    pub fn live_delay_ms(&self) -> f64 {
        if self.timeshift() {
            (self.duration_ms() - self.position_ms()).max(0.0)
        } else {
            0.0
        }
    }
    pub fn return_to_live(self: Pin<&mut Self>) -> bool {
        self.control_transport(playback::Session::return_to_live)
    }

    pub fn media_active(&self) -> bool {
        self.rust().stream_state.active()
    }
    pub fn paused(&self) -> bool {
        self.rust().stream_state.paused()
    }
    pub fn seeking(&self) -> bool {
        self.rust().stream_state.seeking()
    }
    pub fn ended(&self) -> bool {
        self.rust().stream_state.ended()
    }
    pub fn seekable(&self) -> bool {
        (self.recording() || self.timeshift()) && self.rust().timeline.range.is_some()
    }
    pub fn position_ms(&self) -> f64 {
        self.rust()
            .timeline
            .position
            .map_or(-1.0, |value| value.mseconds() as f64)
    }
    pub fn duration_estimated(&self) -> bool {
        self.rust().timeline.estimated
    }
    pub fn duration_ms(&self) -> f64 {
        self.rust()
            .timeline
            .duration
            .map_or(-1.0, |value| value.mseconds() as f64)
    }
    pub fn pause(self: Pin<&mut Self>) -> bool {
        self.control_transport(|media| media.transport_control()?.resume(Resume::Paused))
    }
    pub(super) fn resume_transport(self: Pin<&mut Self>) -> bool {
        self.control_transport(|media| media.transport_control()?.resume(Resume::Playing))
    }
    pub fn seek_to(self: Pin<&mut Self>, milliseconds: f64) -> bool {
        self.control_transport(|media| media.transport_control()?.seek(milliseconds))
    }
    pub fn seek_timeline(self: Pin<&mut Self>, session: QString, milliseconds: f64) -> bool {
        self.control_transport(|media| media.seek_timeline(&session.to_string(), milliseconds))
    }
    pub fn timeline_preview(&self, session: QString, milliseconds: f64) -> QString {
        QString::from(
            self.rust()
                .media
                .timeline_preview(&session.to_string(), milliseconds),
        )
    }
    pub fn skip(self: Pin<&mut Self>, milliseconds: f64) -> bool {
        self.control_transport(|media| media.transport_control()?.skip(milliseconds))
    }
    fn control_transport(
        mut self: Pin<&mut Self>,
        operation: impl FnOnce(&mut playback::Session) -> Result<(), Error>,
    ) -> bool {
        let result = if self.rust().stream_state.active() && (self.recording() || self.timeshift())
        {
            operation(&mut self.as_mut().rust_mut().media)
        } else {
            Err(Error::Unavailable)
        };
        let accepted = result.is_ok();
        let message = result.err().map_or(Message::None, |error| {
            Message::Failure(QString::from(error.to_string()))
        });
        let changed = self.rust().transport_message != message;
        self.as_mut().rust_mut().transport_message = message;
        self.as_mut().change_stream_state(|state| state);
        if changed {
            self.as_mut().transport_error_changed();
        }
        if self.seeking() {
            self.as_mut().rust_mut().subtitle_cells = 0;
            self.as_mut().set_subtitle_data(QString::default());
            if self.recording() {
                self.as_mut()
                    .set_current_program_data(QString::from("null"));
                self.as_mut().set_program_progress(0.0);
            }
        }
        accepted
    }
    pub(super) fn set_transport_message(mut self: Pin<&mut Self>, message: Message) {
        if self.rust().transport_message != message {
            self.as_mut().rust_mut().transport_message = message;
            self.as_mut().transport_error_changed();
        }
    }
}
