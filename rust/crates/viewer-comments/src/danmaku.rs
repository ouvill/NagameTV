//! Device-independent danmaku scheduling, placement and ownership.
//! Qt supplies text measurements; it never decides which comments are alive.
use crate::Position;
use serde::Deserialize;
use std::{collections::HashMap, time::Duration};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color(u32);
impl Color {
    pub fn new(rgb: u32) -> Option<Self> {
        (rgb <= 0xffffff).then_some(Self(rgb))
    }
    pub fn rgb(self) -> u32 {
        self.0
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Comment {
    pub own: bool,
    pub text: Box<str>,
    pub position: Position,
    pub color: Color,
}
impl Comment {
    pub fn new(text: &str, position: Position, color: u32) -> Option<Self> {
        if text.is_empty() || text.len() > crate::MAX_COMMENT_BYTES {
            return None;
        }
        let color = Color::new(color)?;
        let text = text
            .replace(['\r', '\n', '\u{2028}', '\u{2029}'], " ")
            .into_boxed_str();
        Some(Self {
            own: false,
            text,
            position,
            color,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Id(u32);
impl Id {
    pub fn value(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub width: f64,
    pub height: f64,
    pub text_height: f64,
    pub font_size: f64,
    pub title_overlaps: bool,
    pub title_bottom: f64,
    pub full_screen: bool,
    pub controls_overlaps: bool,
    pub controls_top: f64,
}
impl Default for Viewport {
    fn default() -> Self {
        Self {
            width: 0.,
            height: 0.,
            text_height: 24.,
            font_size: 21.,
            title_overlaps: false,
            title_bottom: 0.,
            full_screen: false,
            controls_overlaps: false,
            controls_top: 0.,
        }
    }
}
impl Viewport {
    fn valid(self) -> bool {
        [
            self.width,
            self.height,
            self.text_height,
            self.font_size,
            self.title_bottom,
            self.controls_top,
        ]
        .into_iter()
        .all(f64::is_finite)
            && self.width >= 0.
            && self.height >= 0.
            && self.text_height > 0.
            && self.font_size > 0.
    }
    fn top(self) -> f64 {
        if self.title_overlaps {
            40f64.max(self.title_bottom + 16.).min(self.height)
        } else {
            40.
        }
    }
    fn spacing(self) -> f64 {
        30f64.max(self.text_height + 6.)
    }
    fn bottom(self) -> f64 {
        if self.controls_overlaps {
            (self.controls_top - 16.).min(self.height - 24.)
        } else {
            self.height - 24.
        }
    }
    pub fn lanes(self) -> usize {
        ((self.bottom() - self.top()) / self.spacing()).max(0.) as usize
    }
}
// The UI animates each group for 180ms. Keep the whole transition envelope
// when admitting new rows, including rapid reversals before an animation ends.
#[derive(Clone, Copy, Debug, Default)]
struct Origin {
    target: f64,
    low: f64,
    high: f64,
    until: Duration,
}
impl Origin {
    fn envelope(self, clock: Duration) -> (f64, f64) {
        if clock < self.until {
            (self.low, self.high)
        } else {
            (self.target, self.target)
        }
    }
    fn move_to(&mut self, target: f64, clock: Duration, animate: bool) {
        if !animate {
            *self = Self {
                target,
                low: target,
                high: target,
                until: clock,
            };
        } else if target != self.target {
            let (low, high) = self.envelope(clock);
            *self = Self {
                target,
                low: low.min(target),
                high: high.max(target),
                until: clock.saturating_add(Duration::from_millis(180)),
            };
        }
    }
}
fn kind(position: Position) -> usize {
    match position {
        Position::Right => 0,
        Position::Top => 1,
        Position::Bottom => 2,
    }
}

#[derive(Debug)]
struct Active {
    id: Id,
    width: f64,
    motion: Motion,
    expires: Duration,
}
#[derive(Debug)]
enum Motion {
    Scrolling {
        from_x: f64,
        velocity: f64,
        born: Duration,
    },
    Fixed,
}
impl Active {
    fn x(&self, clock: Duration, viewport_width: f64) -> f64 {
        match self.motion {
            Motion::Fixed => (viewport_width - self.width) / 2.,
            Motion::Scrolling {
                from_x,
                velocity,
                born,
            } => from_x - clock.saturating_sub(born).as_secs_f64() * velocity,
        }
    }

    fn separate_from(&self, other: &Self, clock: Duration) -> bool {
        match (&self.motion, &other.motion) {
            (Motion::Scrolling { .. }, Motion::Scrolling { .. }) => {
                let end = self.expires.min(other.expires);
                let separated = |left: &Self, right: &Self| {
                    [clock, end]
                        .into_iter()
                        .all(|time| left.x(time, 0.) + left.width + 10. <= right.x(time, 0.))
                };
                separated(self, other) || separated(other, self)
            }
            (Motion::Fixed, Motion::Fixed | Motion::Scrolling { .. })
            | (Motion::Scrolling { .. }, Motion::Fixed) => false,
        }
    }

    fn admits(&self, left: f64, velocity: f64, clock: Duration) -> bool {
        match self.motion {
            Motion::Fixed => false,
            Motion::Scrolling {
                from_x,
                velocity: previous_velocity,
                born,
            } => {
                // A resize changes the new comment's entry point, but not an
                // existing comment's trajectory or the gap needed to follow it.
                let elapsed = clock.saturating_sub(born).as_secs_f64();
                let right = from_x + self.width - elapsed * previous_velocity;
                if right <= 0. {
                    return true;
                }
                let gap = left - right - 10.;
                gap >= 0.
                    && (velocity <= previous_velocity
                        || gap / (velocity - previous_velocity) >= right / previous_velocity)
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ConfigurationChange {
    Preserved,
    Remeasure { round: Id, comments: Vec<Id> },
}

#[derive(Default)]
enum LayoutState {
    #[default]
    Ready,
    Measuring(PendingMeasurements),
}
struct PendingMeasurements {
    round: Id,
    widths: HashMap<Id, Option<f64>>,
}
// Only a completed measurement batch can rebuild placement. Its constructor
// and the unfinished widths remain private to this module.
struct MeasuredLayout(HashMap<Id, f64>);
enum MeasurementProgress {
    Pending(PendingMeasurements),
    Complete(MeasuredLayout),
}
impl PendingMeasurements {
    fn record(mut self, round: u32, id: Id, width: f64) -> MeasurementProgress {
        if self.round.value() == round
            && width.is_finite()
            && width > 0.
            && let Some(slot) = self.widths.get_mut(&id)
        {
            *slot = Some(width);
        }
        match self
            .widths
            .iter()
            .map(|(&id, &width)| width.map(|width| (id, width)))
            .collect()
        {
            Some(widths) => MeasurementProgress::Complete(MeasuredLayout(widths)),
            None => MeasurementProgress::Pending(self),
        }
    }
}

#[derive(Debug)]
pub struct Relayout {
    pub id: Id,
    pub width: f64,
    pub from_x: f64,
    pub to_x: f64,
    pub y: f64,
    pub remaining: Duration,
}
#[derive(Debug)]
pub struct Spawn {
    pub id: Id,
    pub comment: Comment,
    pub width: f64,
    pub from_x: f64,
    pub to_x: f64,
    pub y: f64,
    pub lifetime: Duration,
}
#[derive(Debug)]
pub struct Measurement {
    pub id: Id,
    pub text: Box<str>,
}
#[derive(Clone, Debug)]
pub struct TimedComment {
    pub id: Box<str>,
    pub time: Duration,
    pub comment: Comment,
}

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("Invalid danmaku JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Invalid danmaku time, text, position or color")]
    Invalid,
}

#[derive(Default)]
enum Timebase { #[default] Wall, Media }
#[derive(Default)]
enum CursorMode { #[default] Playing, Restoring }
enum Start { Now, At(Duration) }
struct Pending { id: Id, comment: Comment, start: Start }
/// Longest supported scrolling lifetime: fullscreen, at half speed.
pub const MAX_LIFETIME: Duration = Duration::from_secs(16);

/// All persistent comment data, lane occupancy, pending measurement and clocks
/// have one owner. Lane storage is lazy: unused rows allocate nothing.
#[derive(Default)]
pub struct Engine {
    viewport: Viewport,
    origins: [Origin; 3],
    speed: Option<f64>,
    clock: Duration,
    paused: bool,
    hidden: bool,
    serial: u32,
    pending: Option<Pending>,
    timebase: Timebase,
    cursor_mode: CursorMode,
    delivered: HashMap<Box<str>, Duration>,
    layout: LayoutState,
    lanes: [Vec<Vec<Active>>; 3],
    active: usize,
    timeline: Vec<TimedComment>,
    next: usize,
    position: Duration,
}
impl Engine {
    pub fn lane_count(&self) -> usize {
        self.viewport.lanes()
    }
    pub fn active_count(&self) -> usize {
        self.active
    }
    pub fn timeline_count(&self) -> usize {
        self.timeline.len()
    }
    pub fn paused(&self) -> bool {
        self.paused
    }
    pub fn set_visible(&mut self, visible: bool) {
        self.hidden = !visible;
        if self.hidden {
            self.clear();
        }
    }
    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        if paused {
            self.pending = None;
        }
    }
    pub fn configure(&mut self, viewport: Viewport, speed: f64) -> Option<ConfigurationChange> {
        if !viewport.valid() || !speed.is_finite() || !(0.5..=2.).contains(&speed) {
            return None;
        }
        // Resizing and fullscreen changes preserve live trajectories and their
        // deadlines. A font change must remeasure every surviving label first.
        let change = if self.viewport.text_height != viewport.text_height
            || self.viewport.font_size != viewport.font_size
        {
            let round = Id(self.serial.checked_add(1)?);
            self.serial = round.value();
            self.pending = None;
            let comments: Vec<_> = self
                .lanes
                .iter()
                .flatten()
                .flatten()
                .map(|entry| entry.id)
                .collect();
            self.layout = if comments.is_empty() {
                LayoutState::Ready
            } else {
                LayoutState::Measuring(PendingMeasurements {
                    round,
                    widths: comments.iter().map(|&id| (id, None)).collect(),
                })
            };
            ConfigurationChange::Remeasure { round, comments }
        } else {
            ConfigurationChange::Preserved
        };
        self.viewport = viewport;
        self.speed = Some(speed);
        self.reflow();
        Some(change)
    }
    pub fn remeasured(&mut self, round: u32, token: u32, width: f64) -> Vec<Relayout> {
        match std::mem::take(&mut self.layout) {
            LayoutState::Ready => Vec::new(),
            LayoutState::Measuring(pending) => match pending.record(round, Id(token), width) {
                MeasurementProgress::Pending(pending) => {
                    self.layout = LayoutState::Measuring(pending);
                    Vec::new()
                }
                MeasurementProgress::Complete(measured) => self.relayout(measured),
            },
        }
    }
    fn relayout(&mut self, measured: MeasuredLayout) -> Vec<Relayout> {
        let mut updates = Vec::with_capacity(self.active);
        let previous = std::mem::take(&mut self.lanes);
        for (kind, lanes) in previous.into_iter().enumerate() {
            for (old_row, entries) in lanes.into_iter().enumerate() {
                for mut entry in entries {
                    let width = measured.0[&entry.id];
                    let remaining = entry.expires.saturating_sub(self.clock);
                    let from_x = match entry.motion {
                        Motion::Fixed => (self.viewport.width - width) / 2.,
                        Motion::Scrolling { .. } => {
                            entry.x(self.clock, self.viewport.width).max(-width)
                        }
                    };
                    entry.width = width;
                    let to_x = match &mut entry.motion {
                        Motion::Fixed => from_x,
                        motion @ Motion::Scrolling { .. } => {
                            *motion = Motion::Scrolling {
                                from_x,
                                velocity: (from_x + width) / remaining.as_secs_f64(),
                                born: self.clock,
                            };
                            -width
                        }
                    };
                    let rows = &mut self.lanes[kind];
                    let fits = |row: usize| {
                        rows.get(row).is_none_or(|entries| {
                            entries
                                .iter()
                                .all(|other| entry.separate_from(other, self.clock))
                        })
                    };
                    let row = if fits(old_row) {
                        old_row
                    } else {
                        (0..=rows.len())
                            .find(|&row| fits(row))
                            .expect("new row is empty")
                    };
                    if row >= rows.len() {
                        rows.resize_with(row + 1, Vec::new);
                    }
                    updates.push(Relayout {
                        id: entry.id,
                        width,
                        from_x,
                        to_x,
                        y: row as f64 * self.viewport.spacing() * if kind == 2 { -1. } else { 1. },
                        remaining,
                    });
                    rows[row].push(entry);
                }
            }
        }
        self.reflow();
        updates
    }
    pub fn clear(&mut self) {
        self.pending = None;
        self.layout = LayoutState::Ready;
        self.lanes = Default::default();
        self.active = 0;
        self.reflow();
    }
    pub fn reset(&mut self) {
        self.clear();
        self.timeline = Vec::new();
        self.delivered.clear();
        self.timebase = Timebase::Wall;
        self.next = 0;
    }
    pub fn advance_wall(&mut self, elapsed: Duration) -> Vec<Id> {
        if self.paused {
            return Vec::new();
        }
        if matches!(self.timebase, Timebase::Wall) { self.clock = self.clock.saturating_add(elapsed); }
        let mut expired = Vec::new();
        for lanes in &mut self.lanes {
            for lane in lanes {
                lane.retain(|entry| {
                    if entry.expires <= self.clock {
                        expired.push(entry.id);
                        false
                    } else {
                        true
                    }
                });
            }
        }
        self.active -= expired.len();
        if let LayoutState::Measuring(pending) = &mut self.layout {
            for id in &expired {
                pending.widths.remove(id);
            }
            if pending.widths.is_empty() {
                self.layout = LayoutState::Ready;
            }
        }
        if !expired.is_empty() {
            self.reflow();
        }
        expired
    }
    pub fn origin(&self, position: Position) -> f64 {
        self.origins[kind(position)].target
    }
    fn reflow(&mut self) {
        let spacing = self.viewport.spacing();
        let top = self.viewport.top();
        let bottom = self.viewport.bottom();
        let hard_bottom = self.viewport.height - 24.;
        for (kind, lanes) in self.lanes.iter().enumerate() {
            let rows = lanes
                .iter()
                .rposition(|row| !row.is_empty())
                .map_or(0, |index| index + 1);
            let target = if rows == 0 {
                if kind == 2 { bottom - spacing } else { top }
            } else if kind == 2 {
                let extent = (rows - 1) as f64 * spacing;
                // If a resize leaves fewer rows than are occupied, keep their
                // spacing and clip the overflow, anchored to the same edge.
                (bottom - spacing)
                    .max(top + extent)
                    .max(40. + extent)
                    .min(hard_bottom - spacing)
            } else {
                let extent = rows as f64 * spacing;
                top.min(bottom - extent).min(hard_bottom - extent).max(40.)
            };
            self.origins[kind].move_to(target, self.clock, self.active > 0);
        }
    }
    fn admission_rows(&self, kind: usize) -> std::ops::Range<usize> {
        let spacing = self.viewport.spacing();
        let origin = self.origins[kind];
        let (low, high) = origin.envelope(self.clock);
        let (first, end) = if kind == 2 {
            let first = ((origin.target + spacing - self.viewport.bottom()) / spacing)
                .ceil()
                .max(((high + spacing - (self.viewport.height - 24.)) / spacing).ceil());
            let end = ((origin.target + spacing - self.viewport.top()) / spacing)
                .floor()
                .min(((low + spacing - 40.) / spacing).floor());
            (first, end)
        } else {
            let first = ((self.viewport.top() - origin.target) / spacing)
                .ceil()
                .max(((40. - low) / spacing).ceil());
            let end = ((self.viewport.bottom() - origin.target) / spacing)
                .floor()
                .min(((self.viewport.height - 24. - high) / spacing).floor());
            (first, end)
        };
        let first = first.max(0.) as usize;
        first..(end.max(0.) as usize).max(first)
    }
    pub fn media_driven(&self) -> bool { matches!(self.timebase, Timebase::Media) }
    pub fn positions(&self) -> Vec<(Id, f64)> {
        self.lanes.iter().flatten().flatten().map(|entry| (entry.id, entry.x(self.clock, self.viewport.width))).collect()
    }
    pub fn prepare_timed(&mut self, record: TimedComment) -> Option<Measurement> {
        self.prepare_at(record.comment, Start::At(record.time))
    }
    pub fn prepare(&mut self, comment: Comment) -> Option<Measurement> {
        self.prepare_at(comment, Start::Now)
    }
    fn prepare_at(&mut self, comment: Comment, start: Start) -> Option<Measurement> {
        if self.hidden
            || (self.paused && matches!(start, Start::Now))
            || self.lane_count() == 0
            || self.viewport.width <= 0.
            || matches!(self.layout, LayoutState::Measuring(_))
        {
            return None;
        }
        self.serial = self.serial.checked_add(1)?; // Never reuse a stale UI token.
        let id = Id(self.serial);
        let text = comment.text.clone();
        self.pending = Some(Pending { id, comment, start }); // At most one outstanding measurement.
        Some(Measurement { id, text })
    }
    pub fn measured(&mut self, token: u32, width: f64) -> Option<Spawn> {
        if self.pending.as_ref()?.id.value() != token {
            return None;
        }
        let Pending { id, comment, start } = self.pending.take()?;
        if !width.is_finite() || width <= 0. || (self.paused && matches!(start, Start::Now)) || self.viewport.width <= 0. {
            return None;
        }
        let seconds = match (comment.position, self.viewport.full_screen) {
            (Position::Right, false) => 5.,
            (Position::Right, true) => 8.,
            (_, false) => 4.,
            (_, true) => 6.,
        } / self.speed.unwrap_or(1.);
        let age = match start { Start::Now => Duration::ZERO, Start::At(start) => self.position.checked_sub(start)? };
        let lifetime = Duration::from_secs_f64(seconds).checked_sub(age).filter(|lifetime| !lifetime.is_zero())?;
        let from_x = if comment.position == Position::Right {
            self.viewport.width - (self.viewport.width + width) * age.as_secs_f64() / seconds
        } else { (self.viewport.width - width) / 2. };
        let velocity = (self.viewport.width + width) / seconds;
        if !velocity.is_finite() {
            return None;
        }
        let active = Active {
            id, width,
            motion: match comment.position {
                Position::Right => Motion::Scrolling { from_x, velocity, born: self.clock },
                Position::Top | Position::Bottom => Motion::Fixed,
            },
            expires: self.clock.saturating_add(lifetime),
        };
        let kind = kind(comment.position);
        let range = self.admission_rows(kind);
        let (first, limit) = (range.start, range.end);
        let lanes = &mut self.lanes[kind];
        let lane = (first..limit.min(lanes.len().max(first) + 1)).find(|&index| {
            lanes.get(index).is_none_or(|entries| entries.iter().all(|entry| match start {
                Start::Now => entry.admits(from_x, velocity, self.clock),
                Start::At(_) => active.separate_from(entry, self.clock),
            }))
        })?;
        if lane >= lanes.len() { lanes.resize_with(lane + 1, Vec::new); }
        lanes[lane].push(active);
        self.active += 1;
        let (from_x, to_x) = if comment.position == Position::Right {
            (from_x, -width)
        } else {
            let x = (self.viewport.width - width) / 2.;
            (x, x)
        };
        let y = if comment.position == Position::Bottom {
            -(lane as f64) * self.viewport.spacing()
        } else {
            lane as f64 * self.viewport.spacing()
        };
        Some(Spawn {
            id,
            comment,
            width,
            from_x,
            to_x,
            y,
            lifetime,
        })
    }
    pub fn load(&mut self, records: Vec<TimedComment>) {
        self.timebase = Timebase::Media;
        self.timeline = records;
        self.timeline.sort_by_key(|record| record.time);
        self.seek(self.position);
    }
    /// Refresh a bounded window without restarting surviving labels. Newly
    /// acquired comments may already be partway across the screen.
    pub fn replace(&mut self, mut records: Vec<TimedComment>) {
        self.timebase = Timebase::Media;
        records.sort_by_key(|record| record.time);
        self.timeline = records;
        let earliest = self.position.saturating_sub(MAX_LIFETIME);
        self.delivered.retain(|_, time| *time >= earliest);
        self.next = self.timeline.partition_point(|record| record.time < earliest);
        self.cursor_mode = CursorMode::Restoring;
    }
    pub fn seek(&mut self, position: Duration) {
        self.clear();
        self.delivered.clear();
        self.position = position;
        self.clock = position;
        self.next = self.timeline.partition_point(|record| record.time < position.saturating_sub(MAX_LIFETIME));
        self.cursor_mode = CursorMode::Restoring;
    }
    /// Returns true on a backwards discontinuity; the view must clear too.
    pub fn set_position(&mut self, position: Duration) -> bool {
        let backwards = position < self.position;
        if backwards { self.seek(position); } else {
            self.position = position;
            if self.media_driven() { self.clock = position; }
        }
        backwards
    }
    pub fn next_due(&mut self) -> Option<TimedComment> {
        if self.paused && matches!(self.cursor_mode, CursorMode::Playing) { return None; }
        loop {
            let Some(record) = self.timeline.get(self.next).filter(|record| record.time <= self.position) else {
                self.cursor_mode = CursorMode::Playing;
                return None;
            };
            self.next += 1;
            if self.position.saturating_sub(record.time) >= MAX_LIFETIME || self.delivered.contains_key(&record.id) { continue; }
            self.delivered.insert(record.id.clone(), record.time);
            return Some(record.clone());
        }
    }

}

pub fn seconds(value: f64) -> Option<Duration> {
    Duration::try_from_secs_f64(value).ok()
}
pub fn position(value: &str) -> Option<Position> {
    match value {
        "right" => Some(Position::Right),
        "top" => Some(Position::Top),
        "bottom" => Some(Position::Bottom),
        _ => None,
    }
}
/// JSON exists only at the import boundary, never between core and rendering.
pub fn parse_timeline(json: &str) -> Result<Vec<TimedComment>, LoadError> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Kind {
        Name(String),
        Number(u8),
    }
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Rgb {
        Number(u32),
        Hex(String),
    }
    #[derive(Deserialize)]
    struct Record {
        id: Option<String>,
        time: f64,
        text: String,
        #[serde(rename = "type")]
        kind: Option<Kind>,
        color: Option<Rgb>,
        #[serde(default)]
        own: bool,
    }
    if json.len() > crate::archive::MAX_RESPONSE_BYTES { return Err(LoadError::Invalid); }
    let records: Vec<Record> = serde_json::from_str(json)?;
    if records.len() > crate::archive::MAX_COMMENTS { return Err(LoadError::Invalid); }
    records
        .into_iter()
        .enumerate()
        .map(|(index, r)| {
            let time = seconds(r.time).ok_or(LoadError::Invalid)?;
            let kind = match r.kind {
                None => Position::Right,
                Some(Kind::Name(name)) => position(&name).ok_or(LoadError::Invalid)?,
                Some(Kind::Number(0)) => Position::Right,
                Some(Kind::Number(1)) => Position::Top,
                Some(Kind::Number(2)) => Position::Bottom,
                _ => return Err(LoadError::Invalid),
            };
            let color = match r.color {
                None => 0xffffff,
                Some(Rgb::Number(color)) => color,
                Some(Rgb::Hex(hex)) => {
                    let hex = hex
                        .strip_prefix('#')
                        .filter(|s| s.len() == 6 && s.bytes().all(|b| b.is_ascii_hexdigit()))
                        .ok_or(LoadError::Invalid)?;
                    u32::from_str_radix(hex, 16).map_err(|_| LoadError::Invalid)?
                }
            };
            Ok(TimedComment {
                id: r.id.unwrap_or_else(|| format!("{index}:{}", r.time)).into_boxed_str(),
                time,
                comment: { let mut comment = Comment::new(&r.text, kind, color).ok_or(LoadError::Invalid)?; comment.own = r.own; comment },
            })
        })
        .collect()
}

#[cfg(test)]
mod tests;
