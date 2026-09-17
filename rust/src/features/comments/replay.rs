//! Broadcast-time comment retention and cancellable, bounded archive windows.
use super::mapping;
use crate::{
    channels::BroadcastService,
    services::{Job, Network, Progress, Stopping},
    transport::programs::catalog::ClockReading,
};
use std::{
    collections::{BTreeMap, HashSet},
    time::{Duration, Instant},
};
use viewer_comments::{Comment, archive};
const WINDOW_MS: i64 = 120_000;
const LOOKBACK_MS: i64 = viewer_comments::danmaku::MAX_LIFETIME.as_millis() as i64;
const PREFETCH_MS: i64 = 30_000;
const MAX_CACHE_BYTES: usize = 16 * 1024 * 1024;
const MAX_CACHE_COMMENTS: usize = 50_000;
const MAX_WINDOWS: usize = 32;
const RETRY: Duration = Duration::from_secs(30);
const ARCHIVE_SETTLE_MS: i64 = 10 * 60 * 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Target {
    source: u64,
    channel: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Window {
    start: i64,
    end: i64,
}
impl Window {
    fn contains(self, utc: i64) -> bool {
        self.start <= utc && utc < self.end
    }
    fn url(self, target: Target) -> String {
        format!(
            "https://jikkyo.tsukumijima.net/api/kakolog/jk{}?starttime={}&endtime={}&format=json",
            target.channel,
            self.start / 1000,
            self.end / 1000
        )
    }
}
#[derive(Default)]
enum Request {
    #[default]
    Idle,
    Loading {
        target: Target,
        window: Window,
        job: Job<Vec<Comment>, archive::Error>,
    },
    Cancelling(Stopping),
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Origin {
    Reception,
    Archive,
}
struct Entry {
    origin: Origin,
    id: u64,
    key: String,
    comment: Comment,
    own: bool,
}
impl Entry {
    fn charge(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.key.capacity()
            + self.comment.text.len()
            + self
                .comment
                .identity
                .as_ref()
                .map_or(0, |identity| identity.user_id.len())
    }
}
#[derive(Default)]
struct Buffer {
    serial: u64,
    entries: BTreeMap<(u64, u64), Entry>,
    bytes: usize,
    revision: u64,
}
fn normalize_user(user: &str) -> &str {
    user.trim_start_matches("nicolive:")
        .trim_start_matches("rekari:")
}
impl Buffer {
    fn insert(&mut self, comment: Comment, own: bool) {
        self.insert_from(comment, own, Origin::Reception, &mut HashSet::new());
    }
    fn merge_archive(&mut self, comments: impl Iterator<Item = Comment>) {
        let mut paired = HashSet::new();
        for comment in comments {
            self.insert_from(comment, false, Origin::Archive, &mut paired);
        }
    }
    fn insert_from(
        &mut self,
        comment: Comment,
        own: bool,
        origin: Origin,
        paired: &mut HashSet<u64>,
    ) {
        let Some(time) = comment.timestamp_micros else {
            return;
        };
        // Comment number alone is not unique in kakolog. Include timestamp,
        // origin, text and style; distinct numbered repeats remain distinct.
        let key = serde_json::to_string(&(
            time,
            comment.source_id,
            comment.origin,
            &comment.text,
            comment.style,
            comment.identity.as_ref().map(|id| {
                id.user_id
                    .as_ref()
                    .trim_start_matches("nicolive:")
                    .trim_start_matches("rekari:")
            }),
        ))
        .expect("comment key");
        // A live relay and kakolog can assign different thread/comment IDs.
        // Match equal original timestamps/payloads across the two paths once
        // per occurrence. Never collapse repeated posts within either path.
        let same_user = |other: &Comment| match (&comment.identity, &other.identity) {
            (Some(left), Some(right)) => {
                normalize_user(&left.user_id) == normalize_user(&right.user_id)
            }
            (None, _) | (_, None) => true,
        };
        let existing = self
            .entries
            .range((time, 0)..=(time, u64::MAX))
            .find(|(_, entry)| {
                entry.key == key
                    || (entry.origin != origin
                        && !paired.contains(&entry.id)
                        && entry.comment.origin == comment.origin
                        && entry.comment.text == comment.text
                        && entry.comment.style == comment.style
                        && same_user(&entry.comment))
            })
            .map(|(&position, _)| position);
        if let Some(position) = existing {
            let entry = self.entries.get_mut(&position).expect("found above");
            if origin == Origin::Archive {
                paired.insert(entry.id);
            }
            if origin == Origin::Reception && (entry.origin != origin || (own && !entry.own)) {
                self.bytes = self.bytes.saturating_sub(entry.charge());
                entry.origin = origin;
                entry.key = key.clone();
                entry.comment = comment;
                entry.own |= own;
                self.bytes += entry.charge();
                self.revision += 1;
            }
            return;
        }
        self.serial += 1;
        let entry = Entry {
            origin,
            id: self.serial,
            key,
            comment,
            own,
        };
        self.bytes += entry.charge();
        self.entries.insert((time, entry.id), entry);
        self.revision += 1;
    }
    fn remove(&mut self, key: (u64, u64)) {
        if let Some(entry) = self.entries.remove(&key) {
            self.bytes = self.bytes.saturating_sub(entry.charge());
            self.revision += 1;
        }
    }
    fn retain(&mut self, earliest_ms: Option<i64>, viewing: Window) -> bool {
        let mut evicted = false;
        if let Some(earliest) = earliest_ms.and_then(|time| u64::try_from(time).ok()) {
            while let Some((&key, _)) = self.entries.first_key_value() {
                if key.0 / 1000 >= earliest {
                    break;
                }
                self.remove(key);
            }
        }
        while self.bytes > MAX_CACHE_BYTES || self.entries.len() > MAX_CACHE_COMMENTS {
            let victim = self
                .entries
                .keys()
                .find(|(time, _)| !viewing.contains((time / 1000) as i64))
                .copied()
                .or_else(|| self.entries.keys().next().copied());
            let Some(victim) = victim else {
                break;
            };
            self.remove(victim);
            evicted = true;
        }
        evicted
    }
    fn project(&self, clock: ClockReading, viewing: Window) -> String {
        let start = viewing.start.max(0) as u64 * 1000;
        let end = viewing.end.max(0) as u64 * 1000;
        let records: Vec<_> = self
            .entries
            .range((start, 0)..(end, 0))
            .filter_map(|(_, entry)| {
                let micros = entry.comment.timestamp_micros?;
                let position = clock
                    .media(i64::try_from(micros / 1000).ok()?)?
                    .checked_add(micros % 1000 * 1000)?;
                Some(serde_json::json!({
                    "id": entry.id.to_string(), "time": position as f64 / 1_000_000_000.,
                    "text": entry.comment.text, "type": entry.comment.style.position,
                    "color": entry.comment.style.color, "own": entry.own,
                }))
            })
            .take(archive::MAX_COMMENTS)
            .collect();
        serde_json::to_string(&records).expect("finite replay window")
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Status {
    #[default]
    Disabled,
    WaitingService,
    WaitingClock,
    Unsupported,
    Seeking,
    Loading,
    Ready,
    Pending,
    Empty,
    Failed,
}
/// Source identity plus an optional output-confirmed position. Reception may
/// begin before the first frame; no wall-clock position is substituted.
pub(crate) struct Context {
    pub source: u64,
    pub service: Option<BroadcastService>,
    pub position: Option<u64>,
    pub clock: Option<ClockReading>,
    pub earliest_utc: Option<i64>,
}
#[derive(Default)]
pub(crate) struct Replay {
    target: Option<Target>,
    buffer: Buffer,
    request: Request,
    // Successful windows (including empty ones) and failed attempts have separate
    // states, so an HTTP failure never becomes a permanent empty result.
    covered: Vec<(Window, Instant)>,
    failed: Option<(Window, Instant)>,
    projected: Option<(u64, i64, i64)>,
    clock: Option<ClockReading>,
    position: Option<u64>,
    pub generation: u64,
    pub data: String,
    pub status: Status,
    was_seeking: bool,
}
impl Replay {
    pub fn disable(&mut self) {
        self.configure(None);
        self.status = Status::Disabled;
    }
    fn cancel(&mut self) {
        self.request = match std::mem::take(&mut self.request) {
            Request::Loading { job, .. } => Request::Cancelling(job.cancel()),
            state @ (Request::Idle | Request::Cancelling(_)) => state,
        };
    }
    fn configure(&mut self, target: Option<Target>) {
        if self.target != target {
            self.cancel();
            self.target = target;
            self.buffer = Buffer::default();
            self.covered.clear();
            self.failed = None;
            self.projected = None;
            self.clock = None;
            self.position = None;
            self.generation += 1;
            self.data = "[]".into();
        }
    }
    pub fn update(
        &mut self,
        network: Option<&Network>,
        context: Option<Context>,
        seeking: bool,
        comments: Vec<(Comment, bool)>,
        now: Instant,
        wall_ms: i64,
    ) {
        if seeking
            && context
                .as_ref()
                .zip(self.target)
                .is_some_and(|(context, target)| context.source == target.source)
        {
            for (comment, own) in comments {
                self.buffer.insert(comment, own);
            }
            let utc = self.projected.map_or(0, |(_, seconds, _)| seconds * 1000);
            if self.buffer.retain(
                None,
                Window {
                    start: utc - LOOKBACK_MS,
                    end: utc + WINDOW_MS,
                },
            ) {
                self.covered.clear();
            }
            self.was_seeking = true;
            self.status = Status::Seeking;
            return;
        }
        let target = context.as_ref().and_then(|context| {
            let Some(service) = context.service else {
                return self.target.filter(|target| target.source == context.source);
            };
            mapping::resolve(service.network_id, service.service_id).map(|channel| Target {
                source: context.source,
                channel,
            })
        });
        self.configure(target);
        for (comment, own) in comments {
            if target.is_some() {
                self.buffer.insert(comment, own);
            }
        }
        let restored = self.was_seeking && !seeking;
        self.was_seeking = seeking;
        let reading = context.as_ref().and_then(|context| {
            let position = context.position?;
            let clock = context.clock.or_else(|| {
                self.position
                    .filter(|previous| *previous == position)
                    .and(self.clock)
            })?;
            clock.utc(position).map(|utc| (clock, position, utc))
        });
        let Some((clock, position, utc)) = reading.filter(|_| target.is_some()) else {
            self.status = if context.is_none() {
                Status::Disabled
            } else if context
                .as_ref()
                .is_some_and(|context| context.service.is_none())
            {
                Status::WaitingService
            } else if target.is_none() {
                Status::Unsupported
            } else {
                Status::WaitingClock
            };
            self.cancel();
            self.poll_cancel();
            if self.clock.take().is_some() {
                self.generation += 1;
            }
            self.data = "[]".into();
            self.projected = None;
            if self.buffer.retain(None, Window { start: 0, end: 0 }) {
                self.covered.clear();
            }
            return;
        };
        let changed_clock = self.clock.is_none_or(|old| !old.agrees(clock, position));
        if restored || changed_clock {
            self.generation += 1;
            self.projected = None;
        }
        self.clock = Some(clock);
        self.position = Some(position);
        let viewing = Window {
            start: utc.saturating_sub(LOOKBACK_MS),
            end: utc.saturating_add(WINDOW_MS),
        };
        if self.buffer.retain(
            context.as_ref().and_then(|context| context.earliest_utc),
            viewing,
        ) {
            self.covered.clear();
        }
        let bucket = utc.saturating_sub(LOOKBACK_MS).max(0) / WINDOW_MS * WINDOW_MS;
        let desired = [
            Window {
                start: bucket,
                end: (bucket + WINDOW_MS).min(wall_ms / 1000 * 1000),
            },
            Window {
                start: bucket + WINDOW_MS,
                end: (bucket + 2 * WINDOW_MS).min(wall_ms / 1000 * 1000),
            },
        ];
        let needed = |window: Window| window.end > window.start && window.start < utc + PREFETCH_MS;
        if let Request::Loading {
            target: loaded,
            window,
            ..
        } = &self.request
            && (Some(*loaded) != self.target
                || !desired.iter().any(|desired| desired.start == window.start))
        {
            self.cancel();
        }
        match std::mem::take(&mut self.request) {
            Request::Loading {
                target: loaded,
                window,
                job,
            } => match job.poll() {
                Progress::Pending(job) => {
                    self.request = Request::Loading {
                        target: loaded,
                        window,
                        job,
                    }
                }
                Progress::Complete(result) if Some(loaded) == self.target => match result {
                    Ok(comments) => {
                        self.buffer
                            .merge_archive(comments.into_iter().filter(|comment| {
                                comment
                                    .timestamp_micros
                                    .is_some_and(|time| window.contains((time / 1000) as i64))
                            }));
                        self.covered.retain(|(old, _)| old.start != window.start);
                        self.covered.push((window, now));
                        if self.covered.len() > MAX_WINDOWS {
                            self.covered.remove(0);
                        }
                        self.failed = None;
                    }
                    Err(error) => {
                        tracing::warn!("Comment archive: {error}");
                        self.failed = Some((window, now));
                    }
                },
                Progress::Complete(_) => {}
            },
            Request::Cancelling(stopping) => {
                if let Progress::Pending(stopping) = stopping.poll() {
                    self.request = Request::Cancelling(stopping);
                }
            }
            Request::Idle => {}
        }
        let missing = desired
            .into_iter()
            .filter(|window| needed(*window))
            .find(|window| {
                !self.covered.iter().any(|(covered, fetched)| {
                    covered.start == window.start
                        && (covered.end >= window.end && covered.end < wall_ms - ARCHIVE_SETTLE_MS
                            || now.duration_since(*fetched) < RETRY)
                })
            });
        if let Some(window) = missing
            && matches!(self.request, Request::Idle)
            && !self.failed.is_some_and(|(failed, when)| {
                failed.start == window.start && now.duration_since(when) < RETRY
            })
            && let (Some(network), Some(target)) = (network, self.target)
        {
            self.request = Request::Loading {
                target,
                window,
                job: network.fetch_json(
                    window.url(target),
                    archive::MAX_RESPONSE_BYTES,
                    archive::parse,
                ),
            };
        }
        if self.buffer.retain(
            context.as_ref().and_then(|context| context.earliest_utc),
            viewing,
        ) {
            self.covered.clear();
        }
        // One-second window movement bounds Qt serialization while renderer
        // positions advance from the actual media clock on every render frame.
        let signature = (
            self.buffer.revision,
            utc / 1000,
            clock.range().1 as i64 / 1_000_000_000,
        );
        if self.projected != Some(signature) {
            self.data = self.buffer.project(clock, viewing);
            self.projected = Some(signature);
        }
        self.status = if self.failed.is_some_and(|(failed, _)| failed.contains(utc)) {
            Status::Failed
        } else if self.data != "[]" {
            Status::Ready
        } else if matches!(self.request, Request::Loading { .. }) {
            Status::Loading
        } else if missing.is_some() {
            Status::Pending
        } else {
            Status::Empty
        };
    }
    fn poll_cancel(&mut self) {
        if let Request::Cancelling(stopping) = std::mem::take(&mut self.request)
            && let Progress::Pending(stopping) = stopping.poll()
        {
            self.request = Request::Cancelling(stopping);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::programs::{
        Information, Observation,
        catalog::{Accuracy, Catalog, ScanCursor, ScanPoint},
    };
    const UTC: i64 = 1_700_000_000_000;
    const SECOND: u64 = 1_000_000_000;
    fn context(source: u64, seconds: u64) -> Context {
        let mut catalog = Catalog::default();
        catalog.observe(
            &mut ScanCursor::default(),
            ScanPoint {
                accuracy: Accuracy::Indexed,
                epoch: 0,
                offset: 0,
                position: 0,
                end: 1000 * SECOND,
                observation: Some(&Observation {
                    pcr: 0,
                    information: Information {
                        time: Some((0, UTC)),
                        ..Default::default()
                    },
                }),
            },
        );
        Context {
            source,
            service: Some(BroadcastService {
                network_id: 4,
                service_id: 101,
            }),
            position: Some(seconds * SECOND),
            clock: catalog.view(seconds * SECOND).clock,
            earliest_utc: None,
        }
    }
    fn comment(seconds: u64, number: u64) -> Comment {
        Comment {
            identity: None,
            source_id: Some((1, number)),
            text: "same text".into(),
            origin: viewer_comments::Origin::Nx,
            phase: viewer_comments::Phase::Live,
            unix_seconds: UTC as u64 / 1000 + seconds,
            timestamp_micros: Some((UTC as u64 + seconds * 1000) * 1000),
            style: Default::default(),
        }
    }
    #[test]
    fn replay_retains_received_comments_during_seek_and_rejects_future_comments_at_projection() {
        let mut replay = Replay::default();
        let now = Instant::now();
        replay.update(
            None,
            Some(context(1, 3)),
            false,
            vec![(comment(1, 1), true), (comment(500, 2), false)],
            now,
            UTC + 600_000,
        );
        let value: serde_json::Value = serde_json::from_str(&replay.data).unwrap();
        assert_eq!(value.as_array().unwrap().len(), 1);
        assert_eq!(value[0]["time"], 1.);
        assert_eq!(value[0]["own"], true);
        let generation = replay.generation;
        replay.update(None, Some(context(1, 4)), false, vec![], now, UTC + 600_000);
        assert_eq!(replay.generation, generation);
        replay.update(
            None,
            Some(context(1, 100)),
            true,
            vec![(comment(2, 3), false)],
            now,
            UTC + 600_000,
        );
        assert_eq!(replay.buffer.entries.len(), 3);
        replay.update(None, Some(context(1, 3)), false, vec![], now, UTC + 600_000);
        assert!(replay.generation > generation);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&replay.data)
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            2
        );
        replay.update(None, Some(context(2, 3)), false, vec![], now, UTC + 600_000);
        assert_eq!(replay.data, "[]");
        assert!(replay.buffer.entries.is_empty());
    }
    #[test]
    fn buffer_deduplicates_overlap_without_collapsing_distinct_repeated_posts_and_is_bounded() {
        let mut buffer = Buffer::default();
        buffer.insert(comment(1, 1), false);
        buffer.insert(comment(1, 1), false);
        buffer.insert(comment(1, 2), false);
        assert_eq!(buffer.entries.len(), 2);
        let protected = Window {
            start: UTC,
            end: UTC + 20_000,
        };
        for number in 3..8000 {
            let mut comment = comment(number, number);
            comment.text = "x".repeat(viewer_comments::MAX_COMMENT_BYTES).into();
            buffer.insert(comment, false);
        }
        assert!(buffer.retain(None, protected));
        assert!(buffer.bytes <= MAX_CACHE_BYTES);
        assert!(buffer.entries.len() <= MAX_CACHE_COMMENTS);
        assert_eq!(
            buffer.entries.first_key_value().unwrap().0.0,
            (UTC as u64 + 1000) * 1000
        );
        buffer.retain(Some(UTC + 30_000), Window { start: 0, end: 0 });
        assert!(
            buffer
                .entries
                .keys()
                .all(|(time, _)| time / 1000 >= (UTC + 30_000) as u64)
        );
    }
    #[test]
    fn archive_relay_ids_do_not_duplicate_posts_or_collapse_their_multiplicity() {
        let mut buffer = Buffer::default();
        buffer.insert(comment(1, 1), false);
        buffer.insert(comment(1, 2), false);
        buffer.merge_archive([comment(1, 11), comment(1, 12), comment(1, 13)].into_iter());
        assert_eq!(buffer.entries.len(), 3);
        buffer.merge_archive([comment(1, 11), comment(1, 12), comment(1, 13)].into_iter());
        assert_eq!(buffer.entries.len(), 3);
        buffer.insert(comment(1, 3), true);
        assert_eq!(buffer.entries.len(), 3);
        assert_eq!(buffer.entries.values().filter(|entry| entry.own).count(), 1);
    }
    #[test]
    fn missing_clock_preserves_cache_but_never_projects_by_wall_time() {
        let mut replay = Replay::default();
        let now = Instant::now();
        let mut missing = context(1, 3);
        missing.clock = None;
        replay.update(
            None,
            Some(missing),
            false,
            vec![(comment(1, 1), false)],
            now,
            UTC + 600_000,
        );
        assert_eq!(replay.status, Status::WaitingClock);
        assert_eq!(replay.data, "[]");
        assert_eq!(replay.buffer.entries.len(), 1);
        replay.update(None, Some(context(1, 3)), false, vec![], now, UTC + 600_000);
        assert_eq!(replay.status, Status::Ready);
    }
    #[test]
    fn reception_before_first_frame_waits_for_a_confirmed_position() {
        let mut replay = Replay::default();
        let now = Instant::now();
        let mut starting = context(1, 3);
        starting.position = None;
        replay.update(
            None,
            Some(starting),
            false,
            vec![(comment(1, 1), false)],
            now,
            UTC + 600_000,
        );
        assert_eq!(replay.status, Status::WaitingClock);
        assert_eq!(replay.data, "[]");
        assert_eq!(replay.buffer.entries.len(), 1);
        replay.update(None, Some(context(1, 3)), false, vec![], now, UTC + 600_000);
        assert_eq!(replay.status, Status::Ready);
        let value: serde_json::Value = serde_json::from_str(&replay.data).unwrap();
        assert_eq!(value[0]["time"], 1.);
    }
}

#[cfg(test)]
mod request_tests {
    use super::*;
    fn completed(body: &str) -> (Network, Job<Vec<Comment>, archive::Error>) {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            thread,
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/archive", listener.local_addr().unwrap());
        let body = body.to_owned();
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = [0; 4096];
            let received = socket.read(&mut request).unwrap();
            assert!(received > 0);
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        });
        let network = Network::new().unwrap();
        let job = network.fetch_json(url, archive::MAX_RESPONSE_BYTES, archive::parse);
        let deadline = Instant::now() + Duration::from_secs(3);
        while !job.is_finished() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(1));
        }
        assert!(job.is_finished());
        server.join().unwrap();
        (network, job)
    }
    #[test]
    fn queued_archive_result_cannot_cross_a_source_change_even_back_to_same_channel() {
        let (_network, job) =
            completed(r#"{"packet":[{"chat":{"date":100,"content":"old generation"}}]}"#);
        let first = Target {
            source: 1,
            channel: 101,
        };
        let mut replay = Replay::default();
        replay.configure(Some(first));
        replay.request = Request::Loading {
            target: first,
            window: Window {
                start: 0,
                end: WINDOW_MS,
            },
            job,
        };
        replay.configure(Some(Target {
            source: 2,
            channel: 101,
        }));
        assert!(matches!(replay.request, Request::Cancelling(_)));
        replay.poll_cancel();
        assert!(matches!(replay.request, Request::Idle));
        assert!(replay.buffer.entries.is_empty());
        assert_eq!(replay.data, "[]");
    }
    #[test]
    fn empty_and_failure_remain_distinct_results() {
        let (_network, empty) = completed(r#"{"packet":[]}"#);
        assert!(matches!(empty.poll(), Progress::Complete(Ok(comments)) if comments.is_empty()));
        let (_network, failed) = completed(r#"{"error":"archive temporarily unavailable"}"#);
        assert!(matches!(failed.poll(), Progress::Complete(Err(_))));
    }
}
