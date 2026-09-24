//! Playback projection over persistent comments. Fetching and storage have an
//! independent lifetime, so seeks never cancel or invalidate a received response.
use super::mapping;
mod arrivals;
use crate::{channels::BroadcastService, transport::programs::catalog::ClockReading};
use arrivals::LiveArrivals;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};
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
struct Pending {
    source: u64,
    channel: u16,
    clock: Option<cache::ClockSpan>,
    comments: Vec<(Comment, bool)>,
}
#[derive(Default)]
pub(crate) struct Replay {
    storage: Storage,
    directory: Option<PathBuf>,
    budget: Option<i64>,
    target: Option<(u64, u16)>,
    pending: Option<Pending>,
    projected: Option<(u64, i64, String)>,
    clock: Option<ClockReading>,
    position: Option<u64>,
    seeking: bool,
    arrivals: LiveArrivals,
    pub generation: u64,
    pub data: String,
    pub status: Status,
}
impl Replay {
    #[cfg(test)]
    fn in_directory(directory: PathBuf) -> Self {
        Self {
            directory: Some(directory),
            ..Default::default()
        }
    }
    pub fn can_receive(&self) -> bool {
        self.pending.is_none()
            && match &self.storage {
                Storage::Dormant => true,
                Storage::Active(store) => {
                    store.has_capacity()
                        && !matches!(store.snapshot().state, cache::State::StorageFailed(_))
                }
                Storage::Failed(_) => false,
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
        if let Storage::Active(store) = &mut self.storage {
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
        self.pending = None;
        self.target = None;
        self.clear_projection();
        self.status = Status::Disabled;
    }
    fn clear_projection(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.data = "[]".into();
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
            c.service
                .and_then(|s| mapping::resolve(s.network_id, s.service_id))
                .or_else(|| {
                    self.target
                        .filter(|(source, _)| *source == c.source)
                        .map(|(_, channel)| channel)
                })
                .map(|channel| (c.source, channel))
        });
        if target != self.target {
            self.target = target;
            self.pending = None;
            self.clear_projection();
        }
        if self.seeking && !seeking {
            self.clear_projection();
        }
        self.seeking = seeking;
        if context.as_ref().is_some_and(|c| c.enabled) && target.is_some() {
            self.start_store();
        }
        let reading = context.as_ref().and_then(|c| {
            let position = c.position?;
            let clock = c
                .clock
                .or_else(|| self.position.filter(|old| *old == position).and(self.clock))?;
            Some((clock, position, clock.utc(position)?))
        });
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
                changed |= reception.receive(&comments);
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
        let mut demand = context
            .as_ref()
            .zip(target)
            .map(|(c, (source, channel))| Demand {
                source,
                channel,
                view: (c.enabled && c.display && !seeking)
                    .then(|| view.clone())
                    .flatten(),
                source_range: c.source_range.clone(),
                // The storage worker owns seek settling. Cache reads remain
                // available immediately after the output position is known.
                fetch: c.enabled && c.display && !seeking,
            });
        if let Storage::Active(store) = &mut self.storage {
            if let Some(mut pending) = self.pending.take() {
                match store.receive(
                    pending.source,
                    pending.channel,
                    pending.clock.clone(),
                    pending.comments,
                ) {
                    Ok(()) => {}
                    Err(comments) => {
                        pending.comments = comments;
                        self.pending = Some(pending);
                    }
                }
            }
            if !comments.is_empty()
                && let Some((source, channel)) = target
            {
                let clock = context.as_ref().and_then(|c| match &c.source_range {
                    Source::Live { spans, .. } => spans
                        .iter()
                        .filter(|s| s.channel == channel)
                        .max_by_key(|s| s.media_end_ms)
                        .cloned(),
                    Source::Pending | Source::Recording(_) => None,
                });
                match store.receive(source, channel, clock.clone(), comments) {
                    Ok(()) => {}
                    Err(comments) => {
                        // poll_comments checks can_receive before draining.
                        if let Some(pending) = &mut self.pending {
                            pending.comments.extend(comments);
                        } else {
                            self.pending = Some(Pending {
                                source,
                                channel,
                                clock,
                                comments,
                            });
                        }
                    }
                }
            }
            if self.pending.is_some()
                && let Some(Demand {
                    source_range: Source::Live { reception, .. },
                    ..
                }) = &mut demand
            {
                *reception = cache::Reception::Interrupted;
            }
            // Publish the reception tip after enqueuing its comments, so the
            // store can never mark this range complete before saving them.
            store.configure(demand);
        }
        let enabled = context.as_ref().is_some_and(|c| c.enabled && c.display);
        if !enabled {
            if self.data != "[]" || !self.arrivals.is_empty() {
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
            self.data = "[]".into();
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
            self.data = "[]".into();
            self.projected = None;
            return;
        };
        if self.status == Status::WaitingClock {
            tracing::debug!(target: "comment_archive", source = ?self.target, utc_ms = utc, "broadcast clock acquired");
        }
        let Some(view) = view else {
            self.status = Status::WaitingClock;
            self.data = "[]".into();
            return;
        };
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
            self.data = project(records, clock, view.interval, &self.arrivals);
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
            _ if self.data != "[]" => Status::Ready,
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

/// Pair original payloads across reception and archive once per occurrence.
/// Equal posts within the archive remain distinct, including identical IDs.
fn project(
    records: &[Record],
    clock: ClockReading,
    viewing: Interval,
    arrivals: &LiveArrivals,
) -> String {
    // Prefer the transient arrival over its persisted copy. Its ID and display
    // time survive worker snapshots, while the stored posting time stays intact.
    let stored = records.iter().filter(|record| {
        record.origin != RecordOrigin::Live || !arrivals.contains(&record.comment)
    });
    let records: Vec<_> = arrivals.records().into_iter().chain(stored).collect();
    let mut live: HashMap<String, Vec<usize>> = HashMap::new();
    let mut paired = HashSet::new();
    let payload = |record: &Record| {
        serde_json::to_string(&(
            record.comment.timestamp_micros,
            record.comment.origin,
            &record.comment.text,
            record.comment.style,
        ))
        .expect("comment key")
    };
    let mut live_ids = HashSet::new();
    let mut duplicate_live = HashSet::new();
    for (index, record) in records.iter().enumerate() {
        if record.origin == RecordOrigin::Live {
            if let Some(id) = record.comment.source_id
                && !live_ids.insert((id, record.comment.timestamp_micros, payload(record)))
            {
                duplicate_live.insert(index);
                continue;
            }
            live.entry(payload(record)).or_default().push(index);
        }
    }
    let mut selected = Vec::new();
    for (index, record) in records.iter().enumerate() {
        if record.origin == RecordOrigin::Live {
            if duplicate_live.contains(&index) {
                continue;
            }
        } else if let Some(candidates) = live.get(&payload(record)) {
            let same_user = |other: &Comment| match (&record.comment.identity, &other.identity) {
                (Some(a), Some(b)) => {
                    a.user_id
                        .trim_start_matches("nicolive:")
                        .trim_start_matches("rekari:")
                        == b.user_id
                            .trim_start_matches("nicolive:")
                            .trim_start_matches("rekari:")
                }
                _ => true,
            };
            if let Some(&matched) = candidates.iter().find(|&&candidate| {
                !paired.contains(&candidate) && same_user(&records[candidate].comment)
            }) {
                paired.insert(matched);
                continue;
            }
        }
        selected.push(index);
    }
    let selected_count = selected.len();
    let mut json = String::from("[");
    let mut count = 0;
    for index in selected {
        let record = records[index];
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
        let entry=serde_json::json!({"id":record.id.to_string(),"time":media as f64/1_000_000_000.,
            "text":record.comment.text,"type":record.comment.style.position,"color":record.comment.style.color,"own":record.own,
            "timing": if arrival.is_some() { viewer_comments::danmaku::Timing::Live } else { viewer_comments::danmaku::Timing::Scheduled }}).to_string();
        if count >= viewer_comments::danmaku::MAX_TIMELINE_COMMENTS
            || json.len() + entry.len() + 2 > viewer_comments::danmaku::MAX_TIMELINE_BYTES
        {
            break;
        }
        if count > 0 {
            json.push(',');
        }
        json.push_str(&entry);
        count += 1;
    }
    json.push(']');
    tracing::debug!(target: "comment_archive", cached = records.len(), merged = selected_count, projected = count,
        excluded_duplicate = records.len() - selected_count, excluded_clock_window_or_limit = selected_count - count, "projected comments");
    json
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::programs::{
        Information, Observation,
        catalog::{Accuracy, Catalog, ScanCursor, ScanPoint},
    };
    use std::time::{Duration, Instant};
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
    fn live_context(seconds: u64) -> Context {
        let mut ctx = context(1, seconds);
        if let Source::Live { at_edge, .. } = &mut ctx.source_range {
            *at_edge = true;
        }
        ctx
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
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
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
        let first = replay.data.clone();
        let value: serde_json::Value = serde_json::from_str(&first).unwrap();
        assert_eq!(value.as_array().unwrap().len(), 1);
        assert_eq!(value[0]["time"], RECEIVED as f64);
        assert_eq!(value[0]["timing"], "live");
        assert_eq!(value[0]["own"], true);
        // Wait for the actual storage worker, then ensure the saved copy and a
        // repeated delivery do not replace or duplicate the displayed arrival.
        let until = Instant::now() + Duration::from_secs(3);
        loop {
            let stored = match &replay.storage {
                Storage::Active(store) => !store.snapshot().records.is_empty(),
                _ => false,
            };
            if stored {
                break;
            }
            assert!(Instant::now() < until, "live comment was not persisted");
            std::thread::sleep(Duration::from_millis(5));
            replay.update(Some(live_context(RECEIVED)), false, vec![], UTC + 600_000);
        }
        replay.update(
            Some(live_context(RECEIVED)),
            false,
            vec![(comment(POSTED, 1), true)],
            UTC + 600_000,
        );
        assert_eq!(replay.data, first);
        replay.update(Some(context(1, RECEIVED)), true, vec![], UTC + 600_000);
        replay.update(Some(context(1, RECEIVED)), false, vec![], UTC + 600_000);
        let restored: serde_json::Value = serde_json::from_str(&replay.data).unwrap();
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
                .receive(&[(comment(1, 1), false), (comment(35, 2), false)])
        );
        let view = Interval::new(UTC / 1000 + 14, UTC / 1000 + 150).unwrap();
        let value: serde_json::Value =
            serde_json::from_str(&project(&[], clock, view, &arrivals)).unwrap();
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
            .receive(&[(comment(5, 1), false)]);
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
        assert_eq!(replay.data, "[]");
        assert!(!replay.arrivals.is_empty());
        let mut disabled = context_until(1, 3, 4);
        disabled.display = false;
        replay.update(Some(disabled), false, vec![], UTC + 600_000);
        assert!(replay.arrivals.is_empty());
        replay.shutdown();
    }
    #[test]
    fn disk_replay_survives_seek_display_off_and_rejects_old_sources() {
        let directory = tempfile::tempdir().unwrap();
        let mut replay = Replay::in_directory(directory.path().into());
        replay.update(
            Some(context(1, 3)),
            false,
            vec![(comment(1, 1), true), (comment(500, 2), false)],
            UTC + 600_000,
        );
        let until = Instant::now() + Duration::from_secs(3);
        while replay.data == "[]" && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
            replay.update(Some(context(1, 3)), false, vec![], UTC + 600_000);
        }
        let value: serde_json::Value = serde_json::from_str(&replay.data).unwrap();
        assert_eq!(value.as_array().unwrap().len(), 1);
        assert_eq!(value[0]["time"], 1.);
        assert_eq!(value[0]["own"], true);
        // A completed seek restores persisted comments before the worker's
        // HTTP settling deadline, in both directions on the same source.
        const CACHE_READ_DEADLINE: Duration = Duration::from_millis(400);
        for (position, expected_time) in [(500, 500.), (3, 1.)] {
            replay.update(Some(context(1, position)), true, vec![], UTC + 600_000);
            let generation = replay.generation;
            replay.update(Some(context(1, position)), false, vec![], UTC + 600_000);
            assert_ne!(replay.generation, generation);
            let until = Instant::now() + CACHE_READ_DEADLINE;
            loop {
                let value: serde_json::Value = serde_json::from_str(&replay.data).unwrap();
                if value[0]["time"] == expected_time {
                    assert_eq!(value.as_array().unwrap().len(), 1);
                    break;
                }
                assert!(
                    Instant::now() < until,
                    "cached seek did not restore comments"
                );
                std::thread::sleep(Duration::from_millis(5));
                replay.update(Some(context(1, position)), false, vec![], UTC + 600_000);
            }
        }
        let mut disabled = context(1, 3);
        disabled.enabled = false;
        replay.update(Some(disabled), false, vec![], UTC + 600_000);
        assert_eq!(replay.data, "[]");
        replay.update(Some(context(1, 3)), false, vec![], UTC + 600_000);
        let until = Instant::now() + Duration::from_secs(3);
        while replay.data == "[]" && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
            replay.update(Some(context(1, 3)), false, vec![], UTC + 600_000);
        }
        assert_ne!(replay.data, "[]");
        replay.update(Some(context(1, 500)), true, vec![], UTC + 600_000);
        replay.update(Some(context(2, 3)), false, vec![], UTC + 600_000);
        assert_eq!(replay.data, "[]");
        replay.shutdown();
    }
    #[test]
    fn pairing_preserves_archive_multiplicity_and_microseconds() {
        let ctx = context(1, 3);
        let clock = ctx.clock.unwrap();
        let mut posts = vec![
            Record {
                id: 1,
                comment: comment(1, 1),
                own: true,
                origin: RecordOrigin::Live,
                media_ms: None,
            },
            Record {
                id: 2,
                comment: comment(1, 11),
                own: false,
                origin: RecordOrigin::Archive,
                media_ms: None,
            },
            Record {
                id: 3,
                comment: comment(1, 11),
                own: false,
                origin: RecordOrigin::Archive,
                media_ms: None,
            },
        ];
        *posts[0].comment.timestamp_micros.as_mut().unwrap() += 123;
        posts[1].comment.timestamp_micros = posts[0].comment.timestamp_micros;
        posts[2].comment.timestamp_micros = posts[0].comment.timestamp_micros;
        // A reconnect may repeat a live post. It must not consume a second
        // archive occurrence when pairing the two origins.
        let duplicate = Record {
            id: 4,
            comment: posts[0].comment.clone(),
            own: true,
            origin: RecordOrigin::Live,
            media_ms: None,
        };
        posts.push(duplicate);
        let value: serde_json::Value = serde_json::from_str(&project(
            &posts,
            clock,
            Interval::new(UTC / 1000, UTC / 1000 + 10).unwrap(),
            &LiveArrivals::default(),
        ))
        .unwrap();
        assert_eq!(value.as_array().unwrap().len(), 2);
        assert_eq!(value[0]["time"], 1.000123);
        assert_eq!(value[0]["own"], true);
    }
}
