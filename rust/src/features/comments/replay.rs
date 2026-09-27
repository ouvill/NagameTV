//! Playback projection over session-owned live history or persistent archives. Fetching and storage have an
//! independent lifetime, so seeks never cancel or invalidate a received response.
use super::mapping;
mod arrivals;
mod history;
mod timeline;
use crate::{channels::BroadcastService, transport::programs::catalog::ClockReading};
use arrivals::LiveArrivals;
use std::path::PathBuf;
pub(crate) use timeline::Timeline;
use viewer_comments::{
    Comment,
    cache::{self, Controller, Demand, Interval, Record, RecordOrigin, Source, View},
};
const FORWARD_SECONDS: i64 = 120;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum Status {
    #[default]
    Disabled,
    WaitingService,
    WaitingClock,
    Unsupported,
    Seeking,
    Loading,
    Importing,
    Ready,
    Pending,
    Waiting,
    Scheduled(u64),
    Retrying {
        message: String,
        minutes: u64,
    },
    Empty,
    Failed(String),
    StorageFailed(String),
}
pub(crate) struct Context {
    pub source: u64,
    pub service: Option<BroadcastService>,
    pub position: Option<u64>,
    pub clock: Option<ClockReading>,
    pub source_range: Source,
    pub enabled: bool,
    pub display: bool,
    pub paused: bool,
}
#[derive(Default)]
enum Storage {
    #[default]
    Dormant,
    Active(Controller),
    Failed(String),
}
#[derive(Default)]
enum ReplaySource {
    #[default]
    Unselected,
    Live(history::History),
    Recording,
}
#[derive(Default)]
pub(crate) struct Replay {
    storage: Storage,
    directory: Option<PathBuf>,
    budget: Option<i64>,
    target: Option<(u64, u16)>,
    source: ReplaySource,
    projected: Option<(u64, i64, String)>,
    clock: Option<ClockReading>,
    position: Option<u64>,
    seeking: bool,
    arrivals: LiveArrivals,
    timeline: Timeline,
    pub status: Status,
}
impl Replay {
    pub fn timeline(&self) -> &Timeline {
        &self.timeline
    }
    #[cfg(test)]
    fn in_directory(directory: PathBuf) -> Self {
        Self {
            directory: Some(directory),
            ..Default::default()
        }
    }
    pub fn is_live(&self) -> bool {
        matches!(self.source, ReplaySource::Live(_))
    }
    pub fn disconnect(&mut self) {
        if self.is_live() {
            self.source = ReplaySource::Unselected;
            self.target = None;
            self.clear_projection();
            self.status = Status::Disabled;
        }
    }
    pub fn disk_bytes(&self) -> u64 {
        match &self.storage {
            Storage::Active(store) => store.snapshot().disk_bytes,
            Storage::Dormant | Storage::Failed(_) => 0,
        }
    }
    pub fn open_cache(&mut self) {
        self.start_store();
    }
    pub fn clear_unused(&mut self) {
        self.start_store();
        if let Storage::Active(store) = &mut self.storage {
            store.clear_unused();
        }
    }
    pub fn refresh_current(&mut self) {
        if matches!(self.source, ReplaySource::Recording)
            && let Storage::Active(store) = &mut self.storage
        {
            store.refresh_current();
        }
    }
    pub fn set_budget(&mut self, bytes: i64) {
        if self.budget == Some(bytes) {
            return;
        }
        self.budget = Some(bytes);
        if let Storage::Active(store) = &mut self.storage {
            store.set_budget(bytes);
        }
    }
    pub fn shutdown(&mut self) {
        if let Storage::Active(store) = &mut self.storage {
            store.shutdown();
        }
        self.storage = Storage::Dormant;
        self.source = ReplaySource::Unselected;
        self.target = None;
        self.clear_projection();
        self.status = Status::Disabled;
    }
    fn clear_projection(&mut self) {
        self.timeline.reset();
        self.projected = None;
        self.clock = None;
        self.position = None;
        self.arrivals.clear();
    }
    fn start_store(&mut self) {
        if !matches!(self.storage, Storage::Dormant) {
            return;
        }
        let directory = self.directory.clone().map(Ok).unwrap_or_else(|| {
            crate::settings::comment_cache_directory().map_err(|e| e.to_string())
        });
        self.storage =
            match directory.and_then(|dir| Controller::start(dir).map_err(|e| e.to_string())) {
                Ok(mut store) => {
                    if let Some(bytes) = self.budget {
                        store.set_budget(bytes);
                    }
                    Storage::Active(store)
                }
                Err(error) => {
                    tracing::error!(%error, "Comment cache initialization failed");
                    Storage::Failed(error)
                }
            };
    }
    pub fn update(
        &mut self,
        context: Option<Context>,
        seeking: bool,
        comments: Vec<(Comment, bool)>,
        wall_ms: i64,
    ) {
        let target = context.as_ref().and_then(|c| {
            match c.service {
                Some(s) => mapping::resolve(s.network_id, s.service_id),
                None => self
                    .target
                    .filter(|(source, _)| *source == c.source)
                    .map(|(_, channel)| channel),
            }
            .map(|channel| (c.source, channel))
        });
        let recording = context
            .as_ref()
            .is_some_and(|c| matches!(c.source_range, Source::Recording(_)));
        if target != self.target
            || (target.is_some() && recording != matches!(self.source, ReplaySource::Recording))
        {
            self.target = target;
            self.source = match target {
                Some(_) if recording => ReplaySource::Recording,
                Some((_, channel)) => ReplaySource::Live(history::History::new(channel)),
                None => ReplaySource::Unselected,
            };
            self.clear_projection();
        }
        if self.seeking && !seeking {
            self.clear_projection();
        }
        self.seeking = seeking;
        if recording && context.as_ref().is_some_and(|c| c.enabled) && target.is_some() {
            self.start_store();
        }
        let reading = context.as_ref().and_then(|c| {
            let position = c.position?;
            let clock = c
                .clock
                .or_else(|| self.position.filter(|old| *old == position).and(self.clock))?;
            Some((clock, position, clock.utc(position)?))
        });
        let added = match (&mut self.source, context.as_ref()) {
            (ReplaySource::Live(history), Some(c)) => match &c.source_range {
                Source::Live {
                    earliest_media_ms,
                    spans,
                    ..
                } => {
                    let position = c.position.and_then(|ns| i64::try_from(ns / 1_000_000).ok());
                    history.update(*earliest_media_ms, position, spans, comments, wall_ms)
                }
                Source::Recording(_) => history::Received::default(),
            },
            _ => history::Received::default(),
        };
        if self.arrivals.mark_own(&added.own) {
            self.projected = None;
        }
        if let Some((clock, position, _)) = reading {
            if self.clock.is_some_and(|old| !old.agrees(clock, position)) {
                self.clear_projection();
            }
            self.clock = Some(clock);
            self.position = Some(position);
            let mut changed = self.arrivals.advance(clock, position);
            if let Some(c) = context.as_ref()
                && let Some(reception) = self.arrivals.at_edge(c, seeking, clock, position)
            {
                changed |= reception.receive(&added.new);
            }
            if changed {
                self.projected = None;
            }
        }
        let view = reading.and_then(|(clock, _, utc)| {
            Interval::new(
                (utc / 1000 - cache::LOOKBACK_SECONDS).max(0),
                utc / 1000 + FORWARD_SECONDS,
            )
            .map(|interval| View {
                clock_key: clock.key(),
                interval,
            })
        });
        let demand = context
            .as_ref()
            .zip(target)
            .and_then(|(c, (source, channel))| {
                let Source::Recording(recording) = &c.source_range else {
                    return None;
                };
                Some(Demand {
                    source,
                    channel,
                    view: (c.enabled && c.display && !seeking)
                        .then(|| view.clone())
                        .flatten(),
                    source_range: recording.clone(),
                    // Archive reads remain available while the worker settles seeks.
                    fetch: c.enabled && c.display && !seeking,
                })
            });
        if let Storage::Active(store) = &mut self.storage {
            // A sent recording request may finish in the background. Live
            // playback never submits an acquisition or received posts to it.
            store.configure(demand);
        }
        let enabled = context.as_ref().is_some_and(|c| c.enabled && c.display);
        if !enabled {
            if !self.timeline.records().is_empty() || !self.arrivals.is_empty() {
                self.clear_projection();
            }
            self.status = Status::Disabled;
            return;
        }
        if target.is_none() {
            self.status = if context.as_ref().is_some_and(|c| c.service.is_none()) {
                Status::WaitingService
            } else {
                Status::Unsupported
            };
            self.timeline.replace(Vec::new());
            return;
        }
        if seeking {
            self.status = Status::Seeking;
            return;
        }
        let Some((clock, _, utc)) = reading else {
            if self.status != Status::WaitingClock {
                tracing::debug!(target: "comment_archive", source = ?self.target, "waiting for broadcast clock");
            }
            self.status = Status::WaitingClock;
            self.timeline.replace(Vec::new());
            self.projected = None;
            return;
        };
        if self.status == Status::WaitingClock {
            tracing::debug!(target: "comment_archive", source = ?self.target, utc_ms = utc, "broadcast clock acquired");
        }
        let Some(view) = view else {
            self.status = Status::WaitingClock;
            self.timeline.replace(Vec::new());
            return;
        };
        if let ReplaySource::Live(history) = &self.source {
            let signature = (history.revision(), utc / 1000, clock.key());
            if self.projected.as_ref() != Some(&signature) {
                let position_ms = (reading.expect("verified clock").1 / 1_000_000) as i64;
                self.timeline.replace(project(
                    history.window(
                        &clock.key(),
                        position_ms.saturating_sub(cache::LOOKBACK_SECONDS * 1000),
                        position_ms.saturating_add(FORWARD_SECONDS * 1000),
                    ),
                    clock,
                    view.interval,
                    &self.arrivals,
                ));
                self.projected = Some(signature);
            }
            // Reception gaps are never advertised as an acquired empty archive.
            self.status = if self.timeline.records().is_empty() {
                Status::Waiting
            } else {
                Status::Ready
            };
            return;
        }
        let snapshot = match &self.storage {
            Storage::Active(store) => store.snapshot(),
            Storage::Failed(error) => {
                self.status = Status::StorageFailed(error.clone());
                return;
            }
            Storage::Dormant => {
                self.status = Status::Loading;
                return;
            }
        };
        let signature = (snapshot.revision, utc / 1000, clock.key());
        if self.projected.as_ref() != Some(&signature) {
            let records = if snapshot.source == target.map(|(source, _)| source)
                && snapshot
                    .view
                    .as_ref()
                    .is_some_and(|view| view.clock_key == clock.key())
            {
                &snapshot.records[..]
            } else {
                &[]
            };
            self.timeline
                .replace(project(records, clock, view.interval, &self.arrivals));
            self.projected = Some(signature);
        }
        self.status = match snapshot.state {
            cache::State::StorageFailed(error) => Status::StorageFailed(error),
            cache::State::FetchFailed { message, retry_at } => match retry_at {
                Some(until) => Status::Retrying {
                    message,
                    minutes: (until.saturating_sub(wall_ms / 1000).max(1) as u64).div_ceil(60),
                },
                None => Status::Failed(message),
            },
            cache::State::Loading => Status::Loading,
            cache::State::Importing => Status::Importing,
            cache::State::Waiting(until) => {
                Status::Scheduled((until.saturating_sub(wall_ms / 1000).max(1) as u64).div_ceil(60))
            }
            _ if !self.timeline.records().is_empty() => Status::Ready,
            _ if utc / 1000 >= cache::archive_end(wall_ms / 1000) => Status::Pending,
            cache::State::Starting => Status::Waiting,
            cache::State::Ready => match snapshot.availability {
                cache::Availability::Stored => Status::Empty,
                cache::Availability::Provisional => Status::Pending,
                cache::Availability::Unrequested => Status::Waiting,
            },
        };
    }
}

/// Project a window from exactly one source. Archive multiplicity is preserved;
/// the live history already deduplicated identified redeliveries on ingestion.
fn project<'a>(
    records: impl IntoIterator<Item = &'a Record>,
    clock: ClockReading,
    viewing: Interval,
    arrivals: &'a LiveArrivals,
) -> Vec<viewer_comments::danmaku::TimedComment> {
    // Prefer a temporary live display time to the same session history record.
    // The stored posting time is never changed by arrival latency.
    let stored = records
        .into_iter()
        .filter(|record| record.origin != RecordOrigin::Live || !arrivals.contains(record.id));
    let records: Vec<_> = arrivals.records().into_iter().chain(stored).collect();
    let mut projected = Vec::new();
    let mut bytes = 0;
    let mut invalid = 0;
    for record in &records {
        let Some(micros) = record.comment.timestamp_micros else {
            continue;
        };
        let arrival = arrivals.start(record.id);
        if arrival.is_none() && !viewing.contains((micros / 1_000_000) as i64) {
            continue;
        }
        let media = if record.origin == RecordOrigin::Live {
            record
                .media_ms
                .and_then(|ms| u64::try_from(ms).ok())
                .and_then(|ms| ms.checked_mul(1_000_000))
                .or_else(|| clock.media((micros / 1000) as i64))
        } else {
            clock.media((micros / 1000) as i64)
        };
        let Some(media) =
            arrival.or_else(|| media.and_then(|ns| ns.checked_add(micros % 1000 * 1000)))
        else {
            continue;
        };
        use viewer_comments::danmaku::{self, TimedComment, Timing};
        let Some(mut comment) = danmaku::Comment::new(
            &record.comment.text,
            record.comment.style.position,
            record.comment.style.color,
        ) else {
            invalid += 1;
            continue;
        };
        comment.own = record.own;
        let id = record.id.to_string().into_boxed_str();
        // Bound record, ID and text payloads to 8 MiB (excluding allocator overhead).
        let charge = std::mem::size_of::<TimedComment>() + id.len() + comment.text.len();
        if projected.len() >= danmaku::MAX_TIMELINE_COMMENTS
            || charge > danmaku::MAX_TIMELINE_BYTES - bytes
        {
            break;
        }
        bytes += charge;
        projected.push(TimedComment {
            id,
            time: std::time::Duration::from_nanos(media),
            comment,
            timing: if arrival.is_some() {
                Timing::Live
            } else {
                Timing::Scheduled
            },
        });
    }
    if invalid > 0 {
        tracing::warn!(target: "comment_archive", invalid, "Discarded invalid comments while projecting playback window");
    }
    let count = projected.len();
    tracing::debug!(target: "comment_playback", available = records.len(), projected = count, "projected comments");
    projected
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::programs::{
        Information, Observation,
        catalog::{Accuracy, Catalog, ScanCursor, ScanPoint},
    };
    use std::time::Duration;
    fn as_json(records: &[viewer_comments::danmaku::TimedComment]) -> serde_json::Value {
        serde_json::Value::Array(records.iter().map(|r| serde_json::json!({
            "id": r.id, "time": r.time.as_secs_f64(), "own": r.comment.own, "timing": r.timing,
        })).collect())
    }
    const UTC: i64 = 1_700_000_000_000;
    const SECOND: u64 = 1_000_000_000;
    fn context(source: u64, seconds: u64) -> Context {
        context_until(source, seconds, 1000)
    }
    fn context_until(source: u64, seconds: u64, end_seconds: u64) -> Context {
        let mut catalog = Catalog::default();
        catalog.observe(
            &mut ScanCursor::default(),
            ScanPoint {
                accuracy: Accuracy::Indexed,
                epoch: 0,
                offset: 0,
                position: 0,
                end: end_seconds * SECOND,
                observation: Some(&Observation {
                    pcr: 0,
                    information: Information {
                        time: Some((0, UTC)),
                        ..Default::default()
                    },
                }),
            },
        );
        let clock = catalog.view(seconds * SECOND).clock.unwrap();
        let span = cache::ClockSpan {
            key: clock.key(),
            channel: 101,
            media_start_ms: 0,
            media_end_ms: end_seconds as i64 * 1000,
            utc_start_ms: UTC,
        };
        Context {
            source,
            service: Some(BroadcastService {
                network_id: 4,
                service_id: 101,
            }),
            position: Some(seconds * SECOND),
            clock: Some(clock),
            source_range: Source::Live {
                earliest_media_ms: 0,
                spans: vec![span],
                reception: cache::Reception::Interrupted,
                at_edge: false,
            },
            enabled: true,
            display: true,
            paused: false,
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
    fn live_record(seconds: u64, id: u64) -> Record {
        Record {
            id,
            comment: comment(seconds, id),
            own: false,
            origin: RecordOrigin::Live,
            media_ms: None,
        }
    }
    fn live_context(seconds: u64) -> Context {
        let mut ctx = context(1, seconds);
        if let Source::Live { at_edge, .. } = &mut ctx.source_range {
            *at_edge = true;
        }
        ctx
    }
    #[test]
    fn typed_projection_preserves_attributes_and_submillisecond_timing() {
        let mut post = comment(1, 1);
        post.text = "改行\n引用\"\\\u{2028}終端".into();
        post.timestamp_micros = post.timestamp_micros.map(|time| time + 123);
        post.style.position = viewer_comments::Position::Top;
        post.style.color = 0x123456;
        let records = project(
            &[Record {
                id: u64::MAX,
                comment: post,
                own: true,
                origin: RecordOrigin::Archive,
                media_ms: None,
            }],
            ClockReading::recording(UTC, 20 * SECOND).unwrap(),
            Interval::new(UTC / 1000, UTC / 1000 + 20).unwrap(),
            &LiveArrivals::default(),
        );
        assert_eq!(records.len(), 1);
        let record = &records[0];
        assert_eq!(record.id.as_ref(), u64::MAX.to_string());
        assert_eq!(record.time, Duration::from_nanos(SECOND + 123_000));
        assert_eq!(record.comment.text.as_ref(), "改行 引用\"\\ 終端");
        assert_eq!(record.comment.position, viewer_comments::Position::Top);
        assert_eq!(record.comment.color.rgb(), 0x123456);
        assert!(record.comment.own);
        assert_eq!(record.timing, viewer_comments::danmaku::Timing::Scheduled);
    }

    #[test]
    fn typed_projection_enforces_count_and_payload_limits() {
        use viewer_comments::danmaku::{MAX_TIMELINE_BYTES, MAX_TIMELINE_COMMENTS, TimedComment};
        for (text, count) in [
            ("x".to_owned(), MAX_TIMELINE_COMMENTS + 1),
            ("x".repeat(viewer_comments::MAX_COMMENT_BYTES), 2_100),
        ] {
            let records: Vec<_> = (0..count)
                .map(|index| {
                    let mut post = comment(1, index as u64);
                    post.text = text.as_str().into();
                    Record {
                        id: index as u64,
                        comment: post,
                        own: false,
                        origin: RecordOrigin::Archive,
                        media_ms: None,
                    }
                })
                .collect();
            let projected = project(
                &records,
                ClockReading::recording(UTC, 20 * SECOND).unwrap(),
                Interval::new(UTC / 1000, UTC / 1000 + 20).unwrap(),
                &LiveArrivals::default(),
            );
            assert!(!projected.is_empty());
            assert!(projected.len() < count);
            assert!(projected.len() <= MAX_TIMELINE_COMMENTS);
            let bytes: usize = projected
                .iter()
                .map(|r| std::mem::size_of::<TimedComment>() + r.id.len() + r.comment.text.len())
                .sum();
            assert!(bytes <= MAX_TIMELINE_BYTES);
        }
    }

    #[test]
    fn encoded_recording_projection_includes_recording_lead_in_and_original_comment_timestamp() {
        let post = Record {
            id: 1,
            comment: comment(2, 1),
            own: false,
            origin: RecordOrigin::Archive,
            media_ms: None,
        };
        let clock = ClockReading::recording(UTC - 5000, 20 * SECOND).unwrap();
        let json = project(
            &[post],
            clock,
            Interval::new(UTC / 1000, UTC / 1000 + 20).unwrap(),
            &LiveArrivals::default(),
        );
        let value: serde_json::Value = as_json(&json);
        assert_eq!(value[0]["time"], 7.0);
        assert_eq!(value[0]["timing"], "scheduled");
    }
    #[test]
    fn live_arrival_is_immediate_and_keeps_original_time_for_seek() {
        let directory = tempfile::tempdir().unwrap();
        let mut replay = Replay::in_directory(directory.path().into());
        const POSTED: u64 = 1;
        const RECEIVED: u64 = 3;
        replay.update(
            Some(live_context(RECEIVED)),
            false,
            vec![(comment(POSTED, 1), true)],
            UTC + 600_000,
        );
        let first = replay.timeline.records().to_vec();
        let value: serde_json::Value = as_json(&first);
        assert_eq!(value.as_array().unwrap().len(), 1);
        assert_eq!(value[0]["time"], RECEIVED as f64);
        assert_eq!(value[0]["timing"], "live");
        assert_eq!(value[0]["own"], true);
        // A redelivery is deduplicated synchronously, without opening a DB.
        assert!(matches!(replay.storage, Storage::Dormant));
        assert!(
            std::fs::read_dir(directory.path())
                .unwrap()
                .next()
                .is_none()
        );
        replay.update(
            Some(live_context(RECEIVED)),
            false,
            vec![(comment(POSTED, 1), true)],
            UTC + 600_000,
        );
        assert_eq!(replay.timeline.records(), first);
        replay.update(Some(context(1, RECEIVED)), true, vec![], UTC + 600_000);
        replay.update(Some(context(1, RECEIVED)), false, vec![], UTC + 600_000);
        let restored: serde_json::Value = as_json(replay.timeline.records());
        assert_eq!(restored.as_array().unwrap().len(), 1);
        assert_eq!(restored[0]["time"], POSTED as f64);
        assert_eq!(restored[0]["timing"], "scheduled");
        replay.shutdown();
    }
    #[test]
    fn live_arrivals_outlive_network_delay_but_wait_for_future_video() {
        let mut arrivals = LiveArrivals::default();
        let ctx = live_context(30);
        let clock = ctx.clock.unwrap();
        let position = ctx.position.unwrap();
        assert!(
            arrivals
                .at_edge(&ctx, false, clock, position)
                .unwrap()
                .receive(&[live_record(1, 1), live_record(35, 2)])
        );
        let view = Interval::new(UTC / 1000 + 14, UTC / 1000 + 150).unwrap();
        let value: serde_json::Value = as_json(&project(&[], clock, view, &arrivals));
        assert_eq!(value.as_array().unwrap().len(), 2);
        assert_eq!(value[0]["time"], 30.);
        assert_eq!(value[1]["time"], 35.);
        assert!(arrivals.advance(clock, 46 * SECOND));
        assert_eq!(arrivals.records().len(), 1);
        assert!(arrivals.advance(clock, 51 * SECOND));
        assert!(arrivals.records().is_empty());
    }
    #[test]
    fn history_pause_seek_and_timeshift_do_not_create_live_arrivals() {
        let directory = tempfile::tempdir().unwrap();
        let mut replay = Replay::in_directory(directory.path().into());
        let mut paused = live_context(3);
        paused.paused = true;
        let mut history = comment(1, 1);
        history.phase = viewer_comments::Phase::History;
        for (ctx, seeking, post) in [
            (live_context(3), false, history),
            (paused, false, comment(1, 2)),
            (live_context(3), true, comment(1, 3)),
            (context(1, 3), false, comment(1, 4)),
        ] {
            replay.update(Some(ctx), seeking, vec![(post, false)], UTC + 600_000);
            assert!(replay.arrivals.records().is_empty());
        }
        replay.shutdown();
    }
    #[test]
    fn live_arrival_waits_for_verified_clock_before_showing_future_post() {
        let mut arrivals = LiveArrivals::default();
        let mut ctx = context_until(1, 3, 4);
        if let Source::Live { at_edge, .. } = &mut ctx.source_range {
            *at_edge = true;
        }
        let clock = ctx.clock.unwrap();
        arrivals
            .at_edge(&ctx, false, clock, ctx.position.unwrap())
            .unwrap()
            .receive(&[live_record(5, 1)]);
        assert!(arrivals.records().is_empty());
        assert!(!arrivals.advance(clock, ctx.position.unwrap()));
        let ready = context_until(1, 5, 7);
        assert!(arrivals.advance(ready.clock.unwrap(), ready.position.unwrap()));
        let posts = arrivals.records();
        assert_eq!(posts.len(), 1);
        assert_eq!(arrivals.start(posts[0].id), Some(5 * SECOND));
    }
    #[test]
    fn disabling_display_releases_arrivals_still_waiting_for_video_clock() {
        let directory = tempfile::tempdir().unwrap();
        let mut replay = Replay::in_directory(directory.path().into());
        let mut ctx = context_until(1, 3, 4);
        if let Source::Live { at_edge, .. } = &mut ctx.source_range {
            *at_edge = true;
        }
        replay.update(
            Some(ctx),
            false,
            vec![(comment(5, 1), false)],
            UTC + 600_000,
        );
        assert!(replay.timeline.records().is_empty());
        assert!(!replay.arrivals.is_empty());
        let mut disabled = context_until(1, 3, 4);
        disabled.display = false;
        replay.update(Some(disabled), false, vec![], UTC + 600_000);
        assert!(replay.arrivals.is_empty());
        replay.shutdown();
    }
    #[test]
    fn memory_replay_survives_pause_seek_and_toggles_without_opening_archive_storage() {
        let directory = tempfile::tempdir().unwrap();
        let mut replay = Replay::in_directory(directory.path().join("must-not-be-created"));
        let mut paused = context(1, 3);
        paused.paused = true;
        replay.update(
            Some(paused),
            false,
            vec![(comment(1, 1), true), (comment(500, 2), false)],
            UTC,
        );
        for position in [500, 3, 500, 3] {
            replay.update(Some(context(1, position)), true, vec![], UTC);
            let previous = replay.timeline.clone();
            replay.update(Some(context(1, position)), false, vec![], UTC);
            assert!(!replay.timeline.same_generation(&previous));
            assert_eq!(replay.timeline.records().len(), 1);
            assert_eq!(
                replay.timeline.records()[0].time.as_secs(),
                if position == 3 { 1 } else { 500 }
            );
        }
        for enabled in [true, false] {
            let mut hidden = context(1, 3);
            hidden.enabled = enabled;
            hidden.display = false;
            replay.update(
                Some(hidden),
                false,
                vec![(comment(4, if enabled { 3 } else { 4 }), false)],
                UTC,
            );
            assert!(replay.timeline.records().is_empty());
            replay.update(Some(context(1, 3)), false, vec![], UTC);
            assert!(!replay.timeline.records().is_empty());
        }
        assert!(matches!(replay.storage, Storage::Dormant));
        assert!(
            std::fs::read_dir(directory.path())
                .unwrap()
                .next()
                .is_none()
        );
        replay.update(Some(context(2, 3)), false, vec![], UTC);
        assert!(replay.timeline.records().is_empty());
        replay.update(
            Some(context(2, 3)),
            false,
            vec![(comment(1, 1), false)],
            UTC,
        );
        replay.update(None, false, vec![], UTC);
        assert!(matches!(replay.source, ReplaySource::Unselected));
        replay.update(Some(context(2, 3)), false, vec![], UTC);
        assert!(replay.timeline.records().is_empty());
    }

    #[test]
    fn failed_archive_storage_does_not_block_live_arrivals_or_rewind() {
        let mut replay = Replay {
            storage: Storage::Failed("injected I/O failure".into()),
            ..Default::default()
        };
        replay.update(
            Some(live_context(3)),
            false,
            vec![(comment(1, 1), true)],
            UTC,
        );
        assert_eq!(replay.timeline.records()[0].time.as_secs(), 3);
        replay.update(Some(context(1, 3)), true, vec![], UTC);
        replay.update(Some(context(1, 3)), false, vec![], UTC);
        assert_eq!(replay.timeline.records()[0].time.as_secs(), 1);
        assert_eq!(replay.status, Status::Ready);
    }

    #[test]
    fn live_history_waits_for_initial_clock_without_starting_an_archive_worker() {
        let mut replay = Replay::default();
        let mut starting = context(1, 3);
        starting.position = None;
        starting.clock = None;
        let Source::Live { spans, .. } = &mut starting.source_range else {
            unreachable!()
        };
        spans.clear();
        replay.update(Some(starting), false, vec![(comment(1, 1), false)], UTC);
        assert_eq!(replay.status, Status::WaitingClock);
        assert!(matches!(replay.storage, Storage::Dormant));
        replay.update(Some(context(1, 3)), false, vec![], UTC + 1);
        assert_eq!(replay.timeline.records()[0].time.as_secs(), 1);
    }

    #[test]
    fn delayed_own_recognition_updates_the_display_without_restarting_a_redelivery() {
        let mut replay = Replay::default();
        replay.update(
            Some(live_context(3)),
            false,
            vec![(comment(1, 1), false)],
            UTC,
        );
        let first = replay.timeline.records()[0].clone();
        replay.update(
            Some(live_context(5)),
            false,
            vec![(comment(1, 1), true)],
            UTC + 1,
        );
        let current = &replay.timeline.records()[0];
        assert_eq!(current.time, first.time);
        assert_eq!(current.id, first.id);
        assert!(current.comment.own);
    }

    #[test]
    fn retention_follows_video_window_after_confirmed_live_return() {
        for always in [false, true] {
            let mut replay = Replay::default();
            replay.update(
                Some(live_context(3)),
                false,
                vec![(comment(1, 1), false)],
                UTC,
            );
            let mut paused = context(1, 3);
            paused.paused = true;
            replay.update(
                Some(paused),
                false,
                vec![(comment(98, 2), false)],
                UTC + 100_000,
            );
            // The pause frame keeps the same arrival and newly received posts
            // are scheduled at their original video time, never at this frame.
            assert_eq!(replay.timeline.records()[0].time.as_secs(), 3);
            assert_eq!(replay.timeline.records()[1].time.as_secs(), 98);
            replay.update(Some(context(1, 100)), true, vec![], UTC + 100_001);
            let mut returned = live_context(100);
            let Source::Live {
                earliest_media_ms, ..
            } = &mut returned.source_range
            else {
                unreachable!()
            };
            *earliest_media_ms = if always { 0 } else { 100_000 };
            replay.update(Some(returned), false, vec![], UTC + 100_002);
            assert_eq!(replay.timeline.records().len(), 1);
            let ReplaySource::Live(history) = &replay.source else {
                unreachable!()
            };
            let records = history.window(&context(1, 3).clock.unwrap().key(), 0, 100_000);
            assert_eq!(records.len(), if always { 2 } else { 1 });
        }
    }

    #[test]
    fn an_active_archive_worker_failure_is_independent_of_live_playback() {
        use std::time::Instant;
        // A file where a directory is required forces a real worker I/O error.
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut replay = Replay::in_directory(file.path().into());
        replay.open_cache();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let Storage::Active(store) = &replay.storage else {
                panic!("worker was not started")
            };
            if matches!(store.snapshot().state, cache::State::StorageFailed(_)) {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        replay.update(
            Some(live_context(3)),
            false,
            vec![(comment(1, 1), false)],
            UTC,
        );
        assert_eq!(replay.timeline.records()[0].time.as_secs(), 3);
        replay.update(Some(context(1, 3)), true, vec![], UTC);
        replay.update(Some(context(1, 3)), false, vec![], UTC);
        assert_eq!(replay.timeline.records()[0].time.as_secs(), 1);
        replay.disconnect();
        assert!(matches!(replay.source, ReplaySource::Unselected));
        assert!(replay.timeline.records().is_empty());
        replay.shutdown();
    }
    #[test]
    fn archive_projection_preserves_multiplicity_and_microseconds() {
        let clock = context(1, 3).clock.unwrap();
        let mut post = live_record(1, 1);
        post.origin = RecordOrigin::Archive;
        *post.comment.timestamp_micros.as_mut().unwrap() += 123;
        let duplicate = Record {
            id: 2,
            ..post.clone()
        };
        let records = project(
            &[post, duplicate],
            clock,
            Interval::new(UTC / 1000, UTC / 1000 + 10).unwrap(),
            &LiveArrivals::default(),
        );
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].time, Duration::from_nanos(SECOND + 123_000));
    }
}
