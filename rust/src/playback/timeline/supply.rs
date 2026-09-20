//! Detect fast playback limited by incoming data, even when other streams' PTS
//! keep the receive index ahead of the playable output.
use super::*;
use crate::playback::input::ReadProgress;

const MIN_OBSERVATION: Duration = Duration::from_millis(500);
// At 1.1x, observe long enough to distinguish the expected gain from frame
// quantization. This changes observation time, never the configured live target.
const MIN_EXPECTED_GAIN: Duration = Duration::from_millis(200);

struct Sample {
    at: Instant,
    position: gst::ClockTime,
    rate: Rate,
    reader: ReadProgress,
}

#[derive(Default)]
pub(super) struct Supply {
    sample: Option<Sample>,
}
impl Supply {
    pub fn reset(&mut self) {
        self.sample = None;
    }
    pub fn limited(
        &mut self,
        rate: Rate,
        position: gst::ClockTime,
        reader: ReadProgress,
        now: Instant,
    ) -> bool {
        if !rate.faster() {
            self.reset();
            return false;
        }
        let current = Sample {
            at: now,
            position,
            rate,
            reader,
        };
        let Some(previous) = &self.sample else {
            self.sample = Some(current);
            return false;
        };
        let waited = reader.waited_since(previous.reader);
        if waited.is_none() || previous.rate != rate || position < previous.position {
            self.sample = Some(current);
            return false;
        }
        let elapsed = now.duration_since(previous.at);
        let interval = MIN_OBSERVATION.max(MIN_EXPECTED_GAIN.div_f64(rate.multiplier() - 1.));
        if elapsed < interval {
            return false;
        }
        let advanced = (position - previous.position).nseconds() as f64
            / gst::ClockTime::SECOND.nseconds() as f64;
        // A reader can reach the receive edge while several seconds remain in
        // decoder queues. Wait until output also loses at least half its gain.
        let limited = waited == Some(true)
            && advanced < elapsed.as_secs_f64() * (1. + rate.multiplier()) / 2.;
        self.sample = Some(current);
        limited
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn position(seconds: f64) -> gst::ClockTime {
        gst::ClockTime::from_nseconds((seconds * gst::ClockTime::SECOND.nseconds() as f64) as u64)
    }

    #[test]
    fn source_waits_only_limit_speed_after_queued_video_is_consumed() {
        for tenths in [11, 12, 15, 20] {
            let rate = Rate::checked(tenths).unwrap();
            let mut supply = Supply::default();
            let start = Instant::now();
            let interval = MIN_OBSERVATION.max(MIN_EXPECTED_GAIN.div_f64(rate.multiplier() - 1.));
            let seconds = interval.as_secs_f64();
            assert!(!supply.limited(rate, position(0.), ReadProgress::idle(), start));
            let queued_end = seconds * rate.multiplier();
            assert!(
                !supply.limited(
                    rate,
                    position(queued_end),
                    ReadProgress::for_test(0, 1),
                    start + interval
                ),
                "{tenths}: decoder read-ahead alone must not end fast playback"
            );
            assert!(
                supply.limited(
                    rate,
                    position(queued_end + seconds),
                    ReadProgress::for_test(0, 2),
                    start + interval * 2
                ),
                "{tenths}: output has slowed to incoming data speed"
            );
        }
    }

    #[test]
    fn slow_decoder_without_receive_wait_does_not_claim_catch_up() {
        let mut supply = Supply::default();
        let start = Instant::now();
        let fast = Rate::checked(20).unwrap();
        assert!(!supply.limited(fast, position(0.), ReadProgress::idle(), start));
        assert!(!supply.limited(
            fast,
            position(0.),
            ReadProgress::idle(),
            start + MIN_OBSERVATION
        ));
    }

    #[test]
    fn seeks_pause_and_rate_changes_discard_previous_measurements() {
        let fast = Rate::checked(20).unwrap();
        let changed = Rate::checked(15).unwrap();
        let start = Instant::now();
        let initial = ReadProgress::for_test(0, 1);
        let waiting = ReadProgress::for_test(0, 2);
        let after_seek = ReadProgress::for_test(1, 1);
        let mut supply = Supply::default();
        assert!(!supply.limited(fast, position(0.), initial, start));
        assert!(!supply.limited(fast, position(0.), after_seek, start + MIN_OBSERVATION));
        supply.reset();
        assert!(!supply.limited(fast, position(0.), initial, start));
        supply.reset(); // Pause and resume must not count paused wall time.
        assert!(!supply.limited(fast, position(0.), waiting, start + MIN_OBSERVATION));
        assert!(!supply.limited(
            changed,
            position(0.),
            ReadProgress::for_test(0, 3),
            start + MIN_OBSERVATION * 2
        ));
    }

    #[test]
    fn short_delivery_gaps_and_slow_playback_do_not_trigger() {
        let start = Instant::now();
        for tenths in [5, 10, 11, 20] {
            let mut supply = Supply::default();
            let rate = Rate::checked(tenths).unwrap();
            assert!(!supply.limited(rate, position(0.), ReadProgress::idle(), start));
            assert!(!supply.limited(
                rate,
                position(0.),
                ReadProgress::for_test(0, 1),
                start + MIN_OBSERVATION / 2
            ));
        }
    }
}
