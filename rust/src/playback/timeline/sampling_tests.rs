//! Controlled native TIME queries; no display, GPU, or audio devices.
use super::*;

#[derive(Clone, Copy, Default)]
struct Replies {
    position: Option<gst::ClockTime>,
    duration: Option<gst::ClockTime>,
    seeking: Option<(bool, gst::ClockTime, Option<gst::ClockTime>)>,
}

struct Playback {
    element: gst::Element,
    controller: Controller,
    replies: Arc<Mutex<Replies>>,
    upstream: gst::Pad,
}

impl Playback {
    fn new() -> Self {
        gst::init().unwrap();
        let replies = Arc::new(Mutex::new(Replies::default()));
        let source = replies.clone();
        let upstream = gst::Pad::builder(gst::PadDirection::Src)
            .query_function(move |_, _, query| {
                let replies = *source.lock().unwrap();
                match query.view_mut() {
                    gst::QueryViewMut::Position(query) => {
                        if let Some(position) = replies.position {
                            query.set(position);
                            return true;
                        }
                    }
                    gst::QueryViewMut::Duration(query) => {
                        if let Some(duration) = replies.duration {
                            query.set(duration);
                            return true;
                        }
                    }
                    gst::QueryViewMut::Seeking(query) => {
                        if let Some((seekable, start, end)) = replies.seeking {
                            query.set(seekable, start, end);
                            return true;
                        }
                    }
                    _ => {}
                }
                false
            })
            .build();
        let element = gst::ElementFactory::make("identity").build().unwrap();
        upstream.set_active(true).unwrap();
        upstream.link(&element.static_pad("sink").unwrap()).unwrap();
        element.set_state(gst::State::Paused).unwrap();
        let controller = Controller::new(&element, StartPosition::Beginning).unwrap();
        Self {
            element,
            controller,
            replies,
            upstream,
        }
    }

    fn publish(&mut self, replies: Replies) {
        *self.replies.lock().unwrap() = replies;
        self.controller.next_sample = Instant::now();
        self.controller.poll(&self.element).unwrap();
    }
}

impl Drop for Playback {
    fn drop(&mut self) {
        self.element.set_state(gst::State::Null).unwrap();
        self.upstream.set_active(false).unwrap();
    }
}

fn available() -> Replies {
    let duration = gst::ClockTime::from_seconds(60);
    Replies {
        position: Some(gst::ClockTime::from_seconds(25)),
        duration: Some(duration),
        seeking: Some((true, gst::ClockTime::ZERO, Some(duration))),
    }
}

#[test]
fn missing_time_queries_preserve_display_but_cannot_authorize_a_seek() {
    let mut playback = Playback::new();
    playback.publish(available());
    let before = playback.controller.snapshot();
    assert_eq!(before.position, available().position);
    assert_eq!(before.duration, available().duration);
    assert!(before.range.is_some());

    // A decoder can temporarily have no TIME answers during preroll/buffering.
    playback.publish(Replies::default());
    assert_eq!(playback.controller.snapshot(), before);
    assert!(matches!(
        playback.controller.prepare(&playback.element),
        Err(Error::Unavailable)
    ));
    assert_eq!(playback.controller.snapshot(), before);

    // The live receive poll must not erase the same confirmed playhead either.
    playback
        .controller
        .retained(
            &playback.element,
            LiveWindow::History(before.range.unwrap()),
            false,
            super::super::input::ReadProgress::idle(),
        )
        .unwrap();
    assert_eq!(playback.controller.snapshot(), before);

    // Explicitly withdrawing seeking is different from a missing query answer.
    playback.publish(Replies {
        seeking: Some((false, gst::ClockTime::ZERO, None)),
        ..available()
    });
    assert_eq!(playback.controller.snapshot().range, None);
    assert!(matches!(
        playback.controller.prepare(&playback.element),
        Err(Error::Unavailable)
    ));
    playback.publish(available());
    assert_eq!(playback.controller.snapshot(), before);

    // A new playback session cannot inherit the previous session's values.
    playback.controller = Controller::new(&playback.element, StartPosition::Beginning).unwrap();
    playback.publish(Replies::default());
    assert_eq!(playback.controller.snapshot(), Snapshot::default());
}

#[test]
fn seeking_keeps_confirmed_position_and_range_until_output_arrives() {
    let mut playback = Playback::new();
    playback.publish(available());
    let before = playback.controller.snapshot();
    let sequence = gst::Seqnum::next();
    playback.controller.state = State::Seeking(Seek {
        sequence,
        target: gst::ClockTime::ZERO,
        resume: Resume::Paused,
        next: None,
        rate: Rate::NORMAL,
        completion: Completion::Position,
        deadline: Instant::now() + SEEK_TIMEOUT,
    });
    // A flushing pipeline may report zero before any frame of the new segment.
    // Missing bounds must not shrink the displayed axis to its initial range.
    for replies in [
        Replies::default(),
        Replies {
            position: Some(gst::ClockTime::ZERO),
            duration: Some(gst::ClockTime::ZERO),
            seeking: Some((true, gst::ClockTime::ZERO, None)),
        },
    ] {
        playback.publish(replies);
        assert_eq!(playback.controller.snapshot().position, before.position);
        assert_eq!(playback.controller.snapshot().duration, before.duration);
        assert_eq!(playback.controller.snapshot().range, before.range);
        assert_eq!(
            playback.controller.snapshot().seek_target,
            Some(gst::ClockTime::ZERO)
        );
        assert!(matches!(
            playback.controller.prepare(&playback.element),
            Err(Error::Unavailable)
        ));
    }
    {
        let mut output = playback.controller.output.lock().unwrap();
        output.buffered = Some(sequence);
        output.rate = Some(Rate::NORMAL.multiplier());
    }
    // A confirmed seek to the beginning must still publish a genuine zero.
    playback.publish(Replies {
        position: Some(gst::ClockTime::ZERO),
        ..available()
    });
    assert_eq!(playback.controller.phase(), Phase::Paused);
    assert_eq!(playback.controller.snapshot().seek_target, None);
    assert_eq!(
        playback.controller.snapshot().position,
        Some(gst::ClockTime::ZERO)
    );
    assert_eq!(playback.controller.snapshot().range, before.range);
}
