//! An unedited recording uses one linear broadcast clock. Container PTS offsets
//! are already removed by the demuxer's TIME segment; never add them again.
use crate::{
    channels::BroadcastService,
    transport::programs::catalog::{ClockReading, View},
};
use viewer_comments::cache::{Interval, Recording};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Broadcast {
    service: BroadcastService,
    start_ms: i64,
}

impl Broadcast {
    pub(super) fn with_start(self, start_ms: i64) -> Self {
        Self::new(self.service, start_ms).unwrap_or(self)
    }
    pub fn new(service: BroadcastService, start_ms: i64) -> Option<Self> {
        (start_ms > 0 && start_ms.checked_mul(1_000_000).is_some())
            .then_some(Self { service, start_ms })
    }

    pub(crate) fn view(&self, duration: Option<gstreamer::ClockTime>) -> View {
        View {
            service: Some(self.service),
            clock: duration
                .and_then(|duration| ClockReading::recording(self.start_ms, duration.nseconds())),
            ..View::default()
        }
    }

    pub(crate) fn comments(&self, duration: Option<gstreamer::ClockTime>) -> Recording {
        let range = duration
            .filter(|duration| *duration > gstreamer::ClockTime::ZERO)
            .and_then(|duration| {
                // Round outward to API seconds, including the last fractional frame.
                let milliseconds = gstreamer::ClockTime::SECOND.mseconds();
                let start_ms = u64::try_from(self.start_ms).ok()?;
                let duration_ms = duration
                    .nseconds()
                    .div_ceil(gstreamer::ClockTime::MSECOND.nseconds());
                let end = start_ms.checked_add(duration_ms)?.div_ceil(milliseconds);
                Interval::new(
                    self.start_ms / milliseconds as i64,
                    i64::try_from(end).ok()?,
                )
            });
        range
            .map(Recording::Whole)
            .unwrap_or(Recording::Discovering)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    const START: i64 = 1_700_000_000_000;
    fn broadcast(start_ms: i64) -> Broadcast {
        Broadcast::new(
            BroadcastService {
                network_id: 4,
                service_id: 101,
            },
            start_ms,
        )
        .unwrap()
    }
    #[test]
    fn whole_video_waits_for_a_nonzero_duration_and_includes_the_last_fractional_second() {
        let broadcast = broadcast(START);
        assert_eq!(broadcast.comments(None), Recording::Discovering);
        assert_eq!(
            broadcast.comments(Some(gstreamer::ClockTime::ZERO)),
            Recording::Discovering
        );
        let seconds = gstreamer::ClockTime::SECOND.nseconds();
        let duration = gstreamer::ClockTime::from_nseconds(seconds + 1);
        assert_eq!(
            broadcast.comments(Some(duration)),
            Recording::Whole(Interval::new(START / 1000, START / 1000 + 2).unwrap())
        );
    }
    proptest! {
        #[test]
        fn whole_video_range_covers_exactly_the_touched_api_seconds(start_offset_ms in 0_u64..1000, duration_ns in 1_u64..86_400_000_000_000) {
            let start_ms = START + start_offset_ms as i64;
            let Recording::Whole(range) = broadcast(start_ms).comments(Some(gstreamer::ClockTime::from_nseconds(duration_ns))) else {
                panic!("known video duration");
            };
            let start_ns = start_ms as u64 * gstreamer::ClockTime::MSECOND.nseconds();
            let end_ns = start_ns + duration_ns;
            let second = gstreamer::ClockTime::SECOND.nseconds();
            prop_assert_eq!(range.start() as u64, start_ns / second);
            prop_assert_eq!((range.end() - 1) as u64, (end_ns - 1) / second);
        }
        #[test]
        fn linear_clock_is_invertible_and_never_extends_past_the_video(duration_ms in 1_u64..86_400_000, position_ms in 0_u64..86_400_000) {
            let position_ms = position_ms % duration_ms;
            let clock = ClockReading::recording(START, duration_ms * 1_000_000).unwrap();
            prop_assert_eq!(clock.utc(position_ms * 1_000_000), Some(START + position_ms as i64));
            prop_assert_eq!(clock.media(START + position_ms as i64), Some(position_ms * 1_000_000));
            prop_assert_eq!(clock.utc(duration_ms * 1_000_000), None);
            prop_assert_eq!(clock.media(START - 1), None);
        }
    }
}
