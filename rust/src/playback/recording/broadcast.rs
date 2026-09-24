//! An unedited recording uses one linear broadcast clock. Container PTS offsets
//! are already removed by the demuxer's TIME segment; never add them again.
use crate::{
    channels::BroadcastService,
    transport::programs::catalog::{ClockReading, View},
};

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    const START: i64 = 1_700_000_000_000;
    proptest! {
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
