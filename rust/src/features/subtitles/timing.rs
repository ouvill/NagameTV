use super::SubtitleCue;
use std::collections::VecDeque;

const PTS_WRAP: i128 = 1 << 33;
const MAX_PENDING: usize = 128;

/// One correspondence between the transport's 90 kHz PTS and video stream time.
#[derive(Clone, Copy)]
pub(super) struct Anchor {
    pub pts: u64,
    pub stream_ns: u64,
}

impl Anchor {
    fn map_ticks(self, pts: i128) -> i128 {
        let delta = (pts - i128::from(self.pts) + PTS_WRAP / 2).rem_euclid(PTS_WRAP) - PTS_WRAP / 2;
        i128::from(self.stream_ns) + delta * 1_000_000_000 / 90_000
    }

    fn map_ms(self, pts_ms: i64) -> i128 {
        self.map_ticks(i128::from(pts_ms) * 90)
    }
}

#[derive(Debug)]
pub(crate) enum SubtitleUpdate {
    Unchanged,
    Clear,
    Show(SubtitleCue),
}

#[derive(Default)]
pub(super) struct Timeline {
    anchor: Option<Anchor>,
    // Only timestamped cues can enter the scheduling queue.
    pending: VecDeque<(i64, SubtitleCue)>,
    expires_pts_ms: Option<i64>,
    clear_pending: bool,
}

impl Timeline {
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    pub fn reset(&mut self) {
        *self = Self {
            clear_pending: true,
            ..Self::default()
        };
    }

    pub fn anchor(&mut self, anchor: Anchor) {
        if self.anchor.is_some_and(|old| {
            (old.map_ticks(i128::from(anchor.pts)) - i128::from(anchor.stream_ns)).abs()
                > 500_000_000
        }) {
            // A new timestamp epoch must not display captions from the old one.
            self.reset();
        }
        self.anchor = Some(anchor);
    }

    pub fn map_ticks(&self, pts: u64) -> Option<i128> {
        self.anchor.map(|anchor| anchor.map_ticks(i128::from(pts)))
    }

    pub fn push(&mut self, cue: SubtitleCue) {
        if cue.cells.len() > 2048
            || cue.text.len() > 128 * 1024
            || cue.cells.iter().map(|c| c.text.len()).sum::<usize>() > 128 * 1024
        {
            tracing::warn!("Discarding oversized caption screen");
            return;
        }
        let Some(pts_ms) = cue.pts_ms else {
            tracing::warn!("ARIB caption has no presentation timestamp; cannot synchronize it");
            return;
        };
        if self.pending.len() >= MAX_PENDING {
            tracing::warn!("Subtitle timeline is full; dropping the incoming caption");
            return;
        }
        self.pending.push_back((pts_ms, cue));
    }

    /// Query against the video sink's stream position, never wall time or receipt time.
    pub fn poll(&mut self, position_ns: Option<u64>) -> SubtitleUpdate {
        let mut update = if std::mem::take(&mut self.clear_pending) {
            SubtitleUpdate::Clear
        } else {
            SubtitleUpdate::Unchanged
        };
        let (Some(anchor), Some(position_ns)) = (self.anchor, position_ns) else {
            return update;
        };
        let now = i128::from(position_ns);
        let mut latest: Option<(i128, SubtitleCue)> = None;
        for _ in 0..self.pending.len() {
            // The loop visits the initial queue length; each iteration removes
            // exactly one entry and may requeue it. No other code can mutate the
            // queue while this method holds &mut self, so it cannot be empty here.
            let (pts_ms, cue) = self.pending.pop_front().expect("queue length checked");
            let start = anchor.map_ms(pts_ms);
            if start <= now {
                // Only the final due screen is observable. Equal timestamps use
                // arrival order, matching the former stable sort without a scratch Vec.
                if latest.as_ref().is_none_or(|(time, _)| start >= *time) {
                    latest = Some((start, cue));
                }
            } else {
                self.pending.push_back((pts_ms, cue));
            }
        }
        // A GUI stall can make several changes due at once; render the final state.
        if let Some((_, cue)) = latest {
            self.expires_pts_ms = cue
                .duration_ms
                .and_then(|duration| cue.pts_ms?.checked_add(i64::try_from(duration).ok()?));
            update = if cue.is_clear_only() {
                self.expires_pts_ms = None;
                SubtitleUpdate::Clear
            } else {
                SubtitleUpdate::Show(cue)
            };
        }
        if self
            .expires_pts_ms
            .is_some_and(|pts| anchor.map_ms(pts) <= now)
        {
            self.expires_pts_ms = None;
            update = SubtitleUpdate::Clear;
        }
        update
    }
}

#[cfg(test)]
mod tests {
    // In tests, unwrap/expect assert successful setup or an expected result.
    // Failures intentionally fail the test; they are not assumed impossible IO.
    use super::*;

    fn caption(pts_ms: i64, duration_ms: Option<u64>) -> SubtitleCue {
        SubtitleCue {
            text: "字幕".into(),
            clear_screen: false,
            duration_ms,
            ..SubtitleCue::clear(pts_ms)
        }
    }

    fn timeline() -> Timeline {
        let mut timeline = Timeline::default();
        timeline.anchor(Anchor {
            pts: 900_000,
            stream_ns: 2_000_000_000,
        });
        timeline
    }

    #[test]
    fn bounds_pending_screens_and_rejects_oversized_text() {
        let mut timeline = Timeline::default();
        let mut too_large = SubtitleCue::clear(0);
        too_large.text = "x".repeat(128 * 1024 + 1);
        timeline.push(too_large);
        assert_eq!(timeline.pending_count(), 0);
        for n in 0..1000 {
            timeline.push(SubtitleCue::clear(n));
        }
        assert_eq!(timeline.pending_count(), MAX_PENDING);
        timeline.reset();
        assert_eq!(timeline.pending_count(), 0);
    }

    #[test]
    fn untimestamped_captions_never_enter_the_queue() {
        let mut timeline = timeline();
        let mut cue = caption(10_000, None);
        cue.pts_ms = None;
        timeline.push(cue);
        assert!(timeline.pending.is_empty());
        assert!(matches!(
            timeline.poll(Some(2_000_000_000)),
            SubtitleUpdate::Unchanged
        ));
        timeline.push(caption(10_000, None));
        assert!(matches!(
            timeline.poll(Some(2_000_000_000)),
            SubtitleUpdate::Show(_)
        ));
    }

    #[test]
    fn buffers_until_video_reaches_pts_and_expires_on_video_time() {
        let mut timeline = timeline();
        timeline.push(caption(11_000, Some(2_000)));
        assert!(matches!(timeline.poll(None), SubtitleUpdate::Unchanged));
        assert!(matches!(
            timeline.poll(Some(2_999_999_999)),
            SubtitleUpdate::Unchanged
        ));
        assert!(matches!(
            timeline.poll(Some(3_000_000_000)),
            SubtitleUpdate::Show(_)
        ));
        // A paused/buffering video position cannot expire the subtitle.
        for _ in 0..1000 {
            assert!(matches!(
                timeline.poll(Some(3_000_000_000)),
                SubtitleUpdate::Unchanged
            ));
        }
        assert!(matches!(
            timeline.poll(Some(4_999_999_999)),
            SubtitleUpdate::Unchanged
        ));
        assert!(matches!(
            timeline.poll(Some(5_000_000_000)),
            SubtitleUpdate::Clear
        ));
    }

    #[test]
    fn indefinite_caption_survives_seven_seconds_until_a_timed_clear() {
        let mut timeline = timeline();
        timeline.push(caption(10_000, None));
        timeline.push(SubtitleCue::clear(30_000));
        assert!(matches!(
            timeline.poll(Some(2_000_000_000)),
            SubtitleUpdate::Show(_)
        ));
        assert!(matches!(
            timeline.poll(Some(21_999_999_999)),
            SubtitleUpdate::Unchanged
        ));
        assert!(matches!(
            timeline.poll(Some(22_000_000_000)),
            SubtitleUpdate::Clear
        ));
    }

    #[test]
    fn late_caption_does_not_restart_its_duration() {
        let mut timeline = timeline();
        timeline.push(caption(10_000, Some(1_000)));
        assert!(matches!(
            timeline.poll(Some(4_000_000_000)),
            SubtitleUpdate::Clear
        ));
    }

    #[test]
    fn future_replacement_is_not_shown_early() {
        let mut timeline = timeline();
        timeline.push(caption(10_000, None));
        timeline.push(caption(15_000, None));
        let SubtitleUpdate::Show(cue) = timeline.poll(Some(2_000_000_000)) else {
            panic!()
        };
        assert_eq!(cue.pts_ms, Some(10_000));
        assert!(matches!(
            timeline.poll(Some(6_000_000_000)),
            SubtitleUpdate::Unchanged
        ));
        let SubtitleUpdate::Show(cue) = timeline.poll(Some(7_000_000_000)) else {
            panic!()
        };
        assert_eq!(cue.pts_ms, Some(15_000));
    }

    #[test]
    fn stalled_poll_uses_latest_timestamp_and_arrival_order_for_ties() {
        let mut timeline = timeline();
        timeline.push(caption(12_000, None));
        timeline.push(caption(15_000, None)); // Future screen must stay queued.
        timeline.push(SubtitleCue::clear(12_000)); // Same-time clear wins.
        timeline.push(caption(11_000, None)); // Late arrival of an older screen.
        assert!(matches!(
            timeline.poll(Some(4_000_000_000)),
            SubtitleUpdate::Clear
        ));
        assert_eq!(timeline.pending_count(), 1);

        timeline.push(SubtitleCue::clear(13_000));
        timeline.push(caption(13_000, Some(1_000))); // Same-time replacement wins.
        let SubtitleUpdate::Show(cue) = timeline.poll(Some(5_000_000_000)) else {
            panic!("the replacement screen should be shown");
        };
        assert_eq!(cue.pts_ms, Some(13_000));
        assert!(matches!(
            timeline.poll(Some(6_000_000_000)),
            SubtitleUpdate::Clear
        ));
        let SubtitleUpdate::Show(cue) = timeline.poll(Some(7_000_000_000)) else {
            panic!("the future screen should remain scheduled");
        };
        assert_eq!(cue.pts_ms, Some(15_000));
        assert_eq!(timeline.pending_count(), 0);
    }

    #[test]
    fn handles_pts_wraparound() {
        let anchor = Anchor {
            pts: (1 << 33) - 90_000,
            stream_ns: 5_000_000_000,
        };
        assert_eq!(anchor.map_ms(1_000), 7_000_000_000);
        assert_eq!(anchor.map_ticks((1 << 33) - 180_000), 4_000_000_000);
    }

    #[test]
    fn resets_on_channel_changes_and_timestamp_discontinuities() {
        let mut timeline = timeline();
        timeline.push(caption(15_000, None));
        timeline.reset();
        assert!(matches!(timeline.poll(None), SubtitleUpdate::Clear));
        timeline.anchor(Anchor {
            pts: 900_000,
            stream_ns: 2_000_000_000,
        });
        assert!(matches!(
            timeline.poll(Some(7_000_000_000)),
            SubtitleUpdate::Unchanged
        ));
        timeline.push(caption(15_000, None));
        timeline.anchor(Anchor {
            pts: 90_000,
            stream_ns: 2_000_000_000,
        });
        assert!(matches!(
            timeline.poll(Some(2_000_000_000)),
            SubtitleUpdate::Clear
        ));
        assert!(timeline.pending.is_empty());
    }
}
