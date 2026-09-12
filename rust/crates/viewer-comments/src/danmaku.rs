//! Device-independent danmaku scheduling, placement and ownership.
//! Qt supplies text measurements; it never decides which comments are alive.
use crate::Position;
use serde::Deserialize;
use std::time::Duration;

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
    velocity: f64,
    born: Duration,
    expires: Duration,
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
    pending: Option<(Id, Comment)>,
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
    pub fn configure(&mut self, viewport: Viewport, speed: f64) -> Option<bool> {
        if !viewport.valid() || !speed.is_finite() || !(0.5..=2.).contains(&speed) {
            return None;
        }
        // Controls move row groups; only structural geometry changes invalidate
        // measured text or horizontal trajectories.
        let changed = self.viewport.width != viewport.width
            || self.viewport.height != viewport.height
            || self.viewport.text_height != viewport.text_height
            || self.viewport.font_size != viewport.font_size
            || self.viewport.full_screen != viewport.full_screen;
        if changed {
            self.clear();
        }
        self.viewport = viewport;
        self.speed = Some(speed);
        self.reflow();
        Some(changed)
    }
    pub fn clear(&mut self) {
        self.pending = None;
        self.lanes = Default::default();
        self.active = 0;
        self.reflow();
    }
    pub fn reset(&mut self) {
        self.clear();
        self.timeline = Vec::new();
        self.next = 0;
    }
    pub fn advance_wall(&mut self, elapsed: Duration) -> Vec<Id> {
        if self.paused {
            return Vec::new();
        }
        self.clock = self.clock.saturating_add(elapsed);
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
                (bottom - spacing)
                    .max(top + extent)
                    .min(hard_bottom - spacing)
                    .max(40. + extent)
            } else {
                let extent = rows as f64 * spacing;
                top.min(bottom - extent).max(40.).min(hard_bottom - extent)
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
    pub fn prepare(&mut self, comment: Comment) -> Option<Measurement> {
        if self.hidden || self.paused || self.lane_count() == 0 || self.viewport.width <= 0. {
            return None;
        }
        self.serial = self.serial.checked_add(1)?; // Never reuse a stale UI token.
        let id = Id(self.serial);
        let text = comment.text.clone();
        self.pending = Some((id, comment)); // At most one outstanding measurement.
        Some(Measurement { id, text })
    }
    pub fn measured(&mut self, token: u32, width: f64) -> Option<Spawn> {
        if self.pending.as_ref()?.0.value() != token {
            return None;
        }
        let (id, comment) = self.pending.take()?;
        if !width.is_finite() || width <= 0. || self.paused {
            return None;
        }
        let seconds = match (comment.position, self.viewport.full_screen) {
            (Position::Right, false) => 5.,
            (Position::Right, true) => 8.,
            (_, false) => 4.,
            (_, true) => 6.,
        } / self.speed.unwrap_or(1.);
        let lifetime = Duration::from_secs_f64(seconds);
        let velocity = (self.viewport.width + width) / seconds;
        if !velocity.is_finite() {
            return None;
        }
        let kind = kind(comment.position);
        let range = self.admission_rows(kind);
        let (first, limit) = (range.start, range.end);
        let lanes = &mut self.lanes[kind];
        let lane = (first..limit.min(lanes.len().max(first) + 1)).find(|&index| {
            lanes.get(index).is_none_or(|entries| {
                entries.iter().all(|entry| {
                    if comment.position != Position::Right {
                        return false;
                    }
                    let elapsed = self.clock.saturating_sub(entry.born).as_secs_f64();
                    let right = self.viewport.width + entry.width - elapsed * entry.velocity;
                    let gap = self.viewport.width - right - 10.;
                    gap >= 0.
                        && (velocity <= entry.velocity
                            || gap / (velocity - entry.velocity) >= right / entry.velocity)
                })
            })
        })?;
        if lane >= lanes.len() {
            lanes.resize_with(lane + 1, Vec::new);
        }
        lanes[lane].push(Active {
            id,
            width,
            velocity,
            born: self.clock,
            expires: self.clock.saturating_add(lifetime),
        });
        self.active += 1;
        let (from_x, to_x) = if comment.position == Position::Right {
            (self.viewport.width, -width)
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
    pub fn load(&mut self, mut records: Vec<TimedComment>) {
        records.sort_by_key(|record| record.time);
        self.timeline = records;
        self.seek(self.position);
    }
    pub fn seek(&mut self, position: Duration) {
        self.clear();
        self.position = position;
        self.next = self
            .timeline
            .partition_point(|record| record.time < position);
    }
    /// Returns true on a backwards discontinuity; the view must clear too.
    pub fn set_position(&mut self, position: Duration) -> bool {
        let backwards = position < self.position;
        if backwards {
            self.seek(position);
        } else {
            self.position = position;
        }
        backwards
    }
    pub fn next_due(&mut self) -> Option<Comment> {
        if self.paused {
            return None;
        }
        let record = self.timeline.get(self.next)?;
        if record.time >= self.position {
            return None;
        }
        self.next += 1;
        Some(record.comment.clone())
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
        time: f64,
        text: String,
        #[serde(rename = "type")]
        kind: Option<Kind>,
        color: Option<Rgb>,
    }
    let records: Vec<Record> = serde_json::from_str(json)?;
    records
        .into_iter()
        .map(|r| {
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
                time,
                comment: Comment::new(&r.text, kind, color).ok_or(LoadError::Invalid)?,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests;
