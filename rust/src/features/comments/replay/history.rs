//! Session-owned live history. UTC is mapped only inside verified clock spans;
//! media position (not UTC) orders retention across broadcast clock rollbacks.
use super::{Comment, Record, RecordOrigin};
use std::collections::{BTreeMap, BTreeSet};
use viewer_comments::cache::ClockSpan;

const HISTORY_BYTES: usize = 256 * 1024 * 1024;
const PENDING_BYTES: usize = 4 * 1024 * 1024;
const PENDING_MILLIS: i64 = super::FORWARD_SECONDS * 1000;
const LOOKBACK_MILLIS: i64 = viewer_comments::cache::LOOKBACK_SECONDS * 1000;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Identity {
    origin: viewer_comments::Origin,
    source: (u64, u64),
    micros: u64,
}
impl Identity {
    fn of(comment: &Comment) -> Option<Self> {
        Some(Self {
            origin: comment.origin,
            source: comment.source_id?,
            micros: comment.timestamp_micros?,
        })
    }
}

enum Position {
    Waiting { expires: i64 },
    Mapped(i64),
}
struct Entry {
    record: Record,
    clock: Option<String>,
    position: Position,
    bytes: usize,
}

#[derive(Default)]
pub(super) struct Received {
    pub new: Vec<Record>,
    pub own: Vec<u64>,
}

/// Constructors bind a history to one commentary channel. The owner replaces
/// the entire value on a viewing-session/selection change, never on a seek.
pub(super) struct History {
    channel: u16,
    serial: u64,
    entries: BTreeMap<u64, Entry>,
    media: BTreeSet<(i64, u64)>,
    pending: BTreeSet<u64>,
    identities: BTreeMap<Identity, u64>,
    spans: Vec<ClockSpan>,
    bytes: usize,
    pending_bytes: usize,
    budget: usize,
    revision: u64,
    overflow_reported: bool,
}
impl History {
    pub(super) fn new(channel: u16) -> Self {
        Self {
            channel,
            serial: 0,
            entries: BTreeMap::new(),
            media: BTreeSet::new(),
            pending: BTreeSet::new(),
            identities: BTreeMap::new(),
            spans: Vec::new(),
            bytes: 0,
            pending_bytes: 0,
            budget: HISTORY_BYTES,
            revision: 0,
            overflow_reported: false,
        }
    }

    pub(super) fn revision(&self) -> u64 {
        self.revision
    }

    /// Charge owned strings plus a conservative allowance for B-tree nodes,
    /// their unused slots and allocation overhead. No capacity survives removal
    /// of the corresponding nodes, unlike a retained HashMap/Vec allocation.
    fn charge(record: &Record, clock: Option<&str>) -> usize {
        const TREE_BYTES: usize = 3
            * (std::mem::size_of::<(u64, Entry)>()
                + std::mem::size_of::<(i64, u64)>()
                + std::mem::size_of::<u64>()
                + std::mem::size_of::<(Identity, u64)>())
            + 16 * std::mem::size_of::<usize>();
        TREE_BYTES
            + record.comment.text.len()
            + record
                .comment
                .identity
                .as_ref()
                .map_or(0, |id| id.user_id.len())
            + clock.map_or(0, str::len)
    }

    fn remove(&mut self, id: u64) {
        let entry = self.entries.remove(&id).expect("indexed live comment");
        if let Some(identity) = Identity::of(&entry.record.comment) {
            self.identities.remove(&identity);
        }
        self.bytes -= entry.bytes;
        match entry.position {
            Position::Waiting { .. } => {
                self.pending.remove(&id);
                self.pending_bytes -= entry.bytes;
            }
            Position::Mapped(ms) => {
                self.media.remove(&(ms, id));
            }
        }
        self.revision = self.revision.wrapping_add(1);
    }

    pub(super) fn update(
        &mut self,
        earliest_ms: i64,
        display_ms: Option<i64>,
        spans: &[ClockSpan],
        comments: Vec<(Comment, bool)>,
        now_ms: i64,
    ) -> Received {
        let cutoff = earliest_ms.saturating_sub(LOOKBACK_MILLIS);
        let display = display_ms.map(|ms| ms.saturating_sub(LOOKBACK_MILLIS)..=ms);
        let retained =
            |ms: i64| ms >= cutoff || display.as_ref().is_some_and(|range| range.contains(&ms));
        while let Some(&(ms, id)) = self.media.first() {
            if retained(ms) {
                break;
            }
            self.remove(id);
        }
        // A paused frame may outlive the seekable video. Keep only its display
        // window, not the entire expired gap between that frame and the buffer.
        if let Some(position) = display_ms
            && position.saturating_add(1) < cutoff
        {
            while let Some(&(_, id)) = self.media.range((position + 1, 0)..(cutoff, 0)).next() {
                self.remove(id);
            }
        }
        let clocks_changed = self.spans != spans;
        if clocks_changed {
            self.spans = spans.to_vec();
        }
        let mut lost_clock = 0;
        // Only the bounded unmapped buffer is revisited when a clock extends.
        // Display polls never scan the retained, mapped history.
        for id in self.pending.iter().copied().collect::<Vec<_>>() {
            let entry = self.entries.get_mut(&id).expect("pending live comment");
            let Position::Waiting { expires } = entry.position else {
                unreachable!("pending index contains only unmapped comments")
            };
            if now_ms >= expires {
                self.remove(id);
                lost_clock += 1;
                continue;
            }
            if !clocks_changed {
                continue;
            }
            let micros = entry
                .record
                .comment
                .timestamp_micros
                .expect("timestamp validated");
            let mut candidates = spans.iter().filter(|span| {
                span.channel == self.channel
                    && entry.clock.as_ref().is_none_or(|key| *key == span.key)
                    && span.media_ms(micros).is_some()
            });
            let Some(span) = candidates.next() else {
                continue;
            };
            if candidates.next().is_some() {
                // No clock at reception and overlapping UTC epochs: guessing
                // could put an old post on unrelated video after a rollback.
                self.remove(id);
                lost_clock += 1;
                continue;
            }
            let media = span.media_ms(micros).expect("verified candidate");
            if !retained(media) {
                self.remove(id);
                continue;
            }
            self.pending_bytes -= entry.bytes;
            entry.clock = Some(span.key.clone());
            self.bytes -= entry.bytes;
            entry.bytes = Self::charge(&entry.record, entry.clock.as_deref());
            self.bytes += entry.bytes;
            entry.position = Position::Mapped(media);
            entry.record.media_ms = Some(media);
            self.pending.remove(&id);
            self.media.insert((media, id));
            self.revision = self.revision.wrapping_add(1);
        }

        let mut added = Received::default();
        for (comment, own) in comments {
            let Some(micros) = comment.timestamp_micros else {
                lost_clock += 1;
                continue;
            };
            let identity = Identity::of(&comment);
            if let Some(id) = identity.and_then(|key| self.identities.get(&key)) {
                let entry = self.entries.get_mut(id).expect("identified live comment");
                if own && !entry.record.own {
                    entry.record.own = true;
                    added.own.push(*id);
                    self.revision = self.revision.wrapping_add(1);
                }
                continue;
            }
            // Bind new arrivals to the current receiving epoch, not the epoch
            // currently being watched during rewind. Never rebind known epochs.
            let tip = spans
                .iter()
                .filter(|s| s.channel == self.channel)
                .max_by_key(|s| s.media_end_ms);
            let mapped = tip.and_then(|span| span.media_ms(micros));
            self.serial = self
                .serial
                .checked_add(1)
                .expect("live comment ID exhausted");
            let id = self.serial;
            let record = Record {
                id,
                comment,
                own,
                origin: RecordOrigin::Live,
                media_ms: mapped,
            };
            // The transient display may show late posts whose original video
            // has expired. They must not extend the retained history backwards.
            if record.comment.phase == viewer_comments::Phase::Live {
                added.new.push(record.clone());
            }
            if mapped.is_some_and(|ms| !retained(ms)) {
                continue;
            }
            let clock = tip.map(|span| span.key.clone());
            let bytes = Self::charge(&record, clock.as_deref());
            let position = match mapped {
                Some(ms) => {
                    self.media.insert((ms, id));
                    Position::Mapped(ms)
                }
                None => {
                    self.pending.insert(id);
                    self.pending_bytes += bytes;
                    Position::Waiting {
                        expires: now_ms.saturating_add(PENDING_MILLIS),
                    }
                }
            };
            self.entries.insert(
                id,
                Entry {
                    record,
                    clock,
                    position,
                    bytes,
                },
            );
            if let Some(identity) = identity {
                self.identities.insert(identity, id);
            }
            self.bytes += bytes;
            self.revision = self.revision.wrapping_add(1);
        }
        let mut overflow = 0;
        while self.pending_bytes > PENDING_BYTES {
            self.remove(*self.pending.first().expect("pending bytes without entries"));
            overflow += 1;
        }
        while self.bytes > self.budget {
            // Preserve current reception when a pathological stream fills the
            // emergency budget. Losing old history never stops the receiver.
            let id = self
                .media
                .first()
                .map(|(_, id)| *id)
                .or_else(|| self.pending.first().copied())
                .expect("bytes without entries");
            self.remove(id);
            overflow += 1;
        }
        if overflow > 0 && !self.overflow_reported {
            tracing::warn!(
                dropped = overflow,
                budget_bytes = self.budget,
                "Live comment history exceeded memory budget; oldest entries discarded"
            );
            self.overflow_reported = true;
        } else if self.bytes < self.budget / 2 && self.pending_bytes < PENDING_BYTES / 2 {
            self.overflow_reported = false;
        }
        if lost_clock > 0 {
            tracing::warn!(
                dropped = lost_clock,
                "Live comments discarded without an unambiguous verified video clock before expiry"
            );
        }
        added
    }

    pub(super) fn window(&self, key: &str, start_ms: i64, end_ms: i64) -> Vec<&Record> {
        self.media
            .range((start_ms, 0)..(end_ms, 0))
            .filter_map(|(_, id)| {
                let entry = &self.entries[id];
                (entry.clock.as_deref() == Some(key)).then_some(&entry.record)
            })
            .take(viewer_comments::danmaku::MAX_TIMELINE_COMMENTS)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    const UTC: i64 = 1_700_000_000_000;

    fn span(key: &str, start: i64, end: i64) -> ClockSpan {
        ClockSpan {
            key: key.into(),
            channel: 1,
            media_start_ms: start,
            media_end_ms: end,
            utc_start_ms: UTC,
        }
    }
    fn post(ms: i64, id: u64) -> Comment {
        Comment {
            identity: None,
            source_id: Some((1, id)),
            text: "同じ本文".into(),
            origin: viewer_comments::Origin::Nx,
            phase: viewer_comments::Phase::History,
            unix_seconds: ((UTC + ms) / 1000) as u64,
            timestamp_micros: Some(((UTC + ms) * 1000) as u64),
            style: Default::default(),
        }
    }

    #[test]
    fn duplicates_out_of_order_and_identical_distinct_posts_keep_identity_and_own_flag() {
        let mut history = History::new(1);
        let spans = [span("clock", 0, 100_000)];
        let mut anonymous = post(2000, 2);
        anonymous.source_id = None;
        history.update(
            0,
            None,
            &spans,
            vec![
                (post(3000, 3), false),
                (post(1000, 1), false),
                (post(1000, 1), true),
                (post(1000, 2), false),
                (anonymous.clone(), false),
                (anonymous, false),
            ],
            0,
        );
        let records = history.window("clock", 0, 100_000);
        assert_eq!(records.len(), 5);
        assert_eq!(
            records
                .iter()
                .map(|r| r.media_ms.unwrap())
                .collect::<Vec<_>>(),
            [1000, 1000, 2000, 2000, 3000]
        );
        assert!(records[0].own);
        let revision = history.revision();
        history.update(0, None, &spans, vec![(post(1000, 1), false)], 1);
        assert_eq!(history.revision(), revision);
    }

    #[test]
    fn unresolved_posts_wait_for_verified_time_and_never_move_between_clock_epochs() {
        let mut history = History::new(1);
        history.update(0, None, &[], vec![(post(1000, 1), false)], 0);
        assert!(history.window("old", 0, 100_000).is_empty());
        history.update(
            0,
            None,
            &[span("old", 0, 4000)],
            vec![(post(5000, 2), false)],
            1,
        );
        assert_eq!(history.window("old", 0, 100_000).len(), 1);
        history.update(0, None, &[span("old", 0, 6000)], vec![], 2);
        assert_eq!(history.window("old", 0, 100_000).len(), 2);
        history.update(
            0,
            None,
            &[span("old", 0, 6000)],
            vec![(post(8000, 3), false)],
            3,
        );
        history.update(
            0,
            None,
            &[span("old", 0, 6000), span("new", 60_000, 90_000)],
            vec![(post(1000, 4), true)],
            4,
        );
        let new = history.window("new", 0, 100_000);
        assert_eq!(new.len(), 1);
        assert_eq!(new[0].media_ms, Some(61_000));
        assert_eq!(history.pending.len(), 1);
        history.update(
            60_000,
            None,
            &[span("new", 60_000, 90_000)],
            vec![],
            PENDING_MILLIS + 3,
        );
        assert!(history.pending.is_empty());
        assert!(history.window("old", 0, 100_000).is_empty());
        assert_eq!(history.window("new", 0, 100_000).len(), 1);

        let mut ambiguous = History::new(1);
        ambiguous.update(0, None, &[], vec![(post(1000, 1), false)], 0);
        ambiguous.update(
            0,
            None,
            &[span("old", 0, 6000), span("new", 60_000, 90_000)],
            vec![],
            1,
        );
        assert!(ambiguous.entries.is_empty());
    }

    #[test]
    fn retention_keeps_sixteen_seconds_and_releases_all_indexes() {
        let mut history = History::new(1);
        let spans = [span("clock", 0, 100_000)];
        history.update(
            0,
            None,
            &spans,
            vec![(post(83_999, 1), false), (post(84_000, 2), true)],
            0,
        );
        history.update(100_000, None, &spans, vec![], 1);
        assert_eq!(history.entries.len(), 1);
        assert_eq!(
            history.window("clock", 0, 100_000)[0].comment.source_id,
            Some((1, 2))
        );
        history.update(200_000, None, &spans, vec![], 2);
        assert!(
            history.entries.is_empty() && history.identities.is_empty() && history.media.is_empty()
        );
        assert_eq!(history.bytes, 0);
    }

    #[test]
    fn emergency_budget_evicts_oldest_media_and_keeps_accepting_new_posts() {
        let mut history = History::new(1);
        let spans = [span("clock", 0, 100_000)];
        history.update(0, None, &spans, vec![(post(1000, 1), false)], 0);
        history.budget = history.bytes * 2;
        history.update(
            0,
            None,
            &spans,
            vec![(post(3000, 3), false), (post(2000, 2), false)],
            1,
        );
        assert_eq!(
            history
                .window("clock", 0, 100_000)
                .iter()
                .map(|r| r.comment.source_id.unwrap().1)
                .collect::<Vec<_>>(),
            [2, 3]
        );
        assert!(history.bytes <= history.budget);
        history.update(0, None, &spans, vec![(post(4000, 4), false)], 2);
        assert_eq!(history.window("clock", 0, 100_000).len(), 2);
        assert!(
            history
                .identities
                .contains_key(&Identity::of(&post(4000, 4)).unwrap())
        );
    }

    #[test]
    fn a_paused_frame_does_not_pin_the_expired_video_between_it_and_the_retained_range() {
        let mut history = History::new(1);
        let spans = [span("clock", 0, 300_000)];
        history.update(
            0,
            Some(30_000),
            &spans,
            (0..300)
                .map(|id| (post(id as i64 * 1000, id), false))
                .collect(),
            0,
        );
        history.update(200_000, Some(30_000), &spans, vec![], 1);
        assert!(history.window("clock", 0, 14_000).is_empty());
        assert_eq!(history.window("clock", 14_000, 31_000).len(), 17);
        assert!(history.window("clock", 31_000, 184_000).is_empty());
        assert_eq!(history.window("clock", 184_000, 300_000).len(), 116);
        history.update(200_000, Some(200_000), &spans, vec![], 2);
        assert!(history.window("clock", 0, 184_000).is_empty());
    }

    #[test]
    fn unmapped_buffer_is_bounded_even_without_any_video_or_clock_progress() {
        let mut history = History::new(1);
        for begin in (0..10_000).step_by(64) {
            let comments = (begin..begin + 64)
                .map(|id| (post(1000, id), false))
                .collect();
            history.update(0, None, &[], comments, 0);
        }
        assert!(history.pending_bytes <= PENDING_BYTES);
        assert!(history.pending.len() < 10_000);
        history.update(0, None, &[], vec![], PENDING_MILLIS);
        assert!(history.entries.is_empty());
        assert_eq!(history.bytes, 0);
    }

    #[test]
    fn history_exceeds_display_limits_and_remains_seekable_with_measured_memory() {
        const COUNT: u64 = 60_000;
        // glibc allocation counters include node slack and allocator retention.
        // This diagnostic measures heap allocation, not physical RSS or hardware.
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        let before = unsafe { libc::mallinfo2().uordblks };
        let mut history = History::new(1);
        let spans = [span("clock", 0, 600_000)];
        for begin in (0..COUNT).step_by(64) {
            let comments = (begin..(begin + 64).min(COUNT))
                .map(|id| {
                    let mut post = post(id as i64 * 10, id);
                    post.text = "あ".repeat(40).into();
                    (post, false)
                })
                .collect();
            history.update(0, None, &spans, comments, 0);
        }
        assert_eq!(history.entries.len(), COUNT as usize);
        assert!(history.bytes < HISTORY_BYTES);
        assert_eq!(
            history.window("clock", 0, 10)[0].comment.source_id,
            Some((1, 0))
        );
        assert_eq!(
            history.window("clock", 599_990, 600_000)[0]
                .comment
                .source_id,
            Some((1, COUNT - 1))
        );
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        {
            // SAFETY: mallinfo2 reads process allocator statistics, takes no
            // pointers, and may be called while the allocator is in use.
            let allocated = unsafe { libc::mallinfo2().uordblks }.saturating_sub(before);
            eprintln!(
                "Live history: posts={COUNT}, text_bytes=120, charged_bytes={}, glibc_allocated_bytes={allocated}",
                history.bytes
            );
        }
        history.update(100_000, None, &spans, vec![], 1);
        assert!(history.window("clock", 0, 84_000).is_empty());
        assert_eq!(history.window("clock", 84_000, 85_000).len(), 100);
    }

    proptest! {
        #[test]
        fn reordered_delivery_deduplication_and_retention_match_a_reference(
            actions in prop::collection::vec((0u8..3, 0u64..300, any::<bool>()), 1..200)
        ) {
            let mut history = History::new(1);
            let spans = [span("clock", 0, 300_000)];
            let mut reference = BTreeMap::<u64, bool>::new();
            let mut earliest = 0;
            for (operation, id, own) in actions {
                let mut comments = Vec::new();
                if operation == 0 {
                    earliest = earliest.max(id as i64 * 1000);
                } else {
                    comments.push((post(id as i64 * 1000, id), own));
                    if id as i64 * 1000 >= earliest - LOOKBACK_MILLIS {
                        *reference.entry(id).or_default() |= own;
                    }
                }
                reference.retain(|id, _| *id as i64 * 1000 >= earliest - LOOKBACK_MILLIS);
                history.update(earliest, None, &spans, comments, 0);
                let actual: BTreeMap<_, _> = history.window("clock", 0, 300_000).iter()
                    .map(|r| (r.comment.source_id.unwrap().1, r.own)).collect();
                prop_assert_eq!(&actual, &reference);
                prop_assert_eq!(history.identities.len(), reference.len());
                prop_assert_eq!(history.bytes, history.entries.values().map(|entry| entry.bytes).sum::<usize>());
            }
        }
    }
}
