//! Independent trajectories and time-based admission; no comparison of comments.
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, ops::Bound, time::Duration};

use super::{Position, TimedComment};

macro_rules! choice {
    ($name:ident, $default:ident, {$($variant:ident => $wire:literal),+ $(,)?}) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(from = "String", into = "String")]
        pub enum $name { $($variant),+ }
        impl Default for $name { fn default() -> Self { Self::$default } }
        impl $name {
            pub fn parse(value: &str) -> Option<Self> {
                match value { $($wire => Some(Self::$variant),)+ _ => None }
            }
            pub fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $wire,)+ }
            }
        }
        impl From<String> for $name {
            fn from(value: String) -> Self { Self::parse(&value).unwrap_or_default() }
        }
        impl From<$name> for String {
            fn from(value: $name) -> Self { value.as_str().into() }
        }
    };
}
choice!(DisplayMode, Scroll, { Scroll => "scroll", Pop => "pop" });
choice!(DensityMode, Normal, { Normal => "normal", All => "all" });

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum PlacementMode {
    #[default]
    Sequential,
    Random,
    // Evaluation only: pairwise overlap/catch-up adjustment is excluded from
    // normal builds because of JP4695583 (estimated expiry 2026-12-11),
    // JP6526304 and JP7178462 (2027-03-02). These are estimates, not verified
    // registry status or a non-infringement opinion. Never auto-enable by date.
    #[cfg(feature = "evaluation-collision-layout")]
    Collision,
}
impl PlacementMode {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "sequential" => Some(Self::Sequential),
            "random" => Some(Self::Random),
            #[cfg(feature = "evaluation-collision-layout")]
            "collision" => Some(Self::Collision),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sequential => "sequential",
            Self::Random => "random",
            #[cfg(feature = "evaluation-collision-layout")]
            Self::Collision => "collision",
        }
    }
    pub(super) fn row(self, sequence: u128, seed: u64, count: usize) -> usize {
        match self {
            Self::Sequential => (sequence % count as u128) as usize,
            Self::Random => (seed % count as u64) as usize,
            #[cfg(feature = "evaluation-collision-layout")]
            Self::Collision => (sequence % count as u128) as usize,
        }
    }
}
impl From<String> for PlacementMode {
    fn from(value: String) -> Self {
        Self::parse(&value).unwrap_or_default()
    }
}
impl From<PlacementMode> for String {
    fn from(value: PlacementMode) -> Self {
        value.as_str().into()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "PresentationWire", into = "PresentationWire")]
pub struct Presentation {
    display: DisplayMode,
    placement: PlacementMode,
}
impl Presentation {
    pub fn new(display: DisplayMode, placement: PlacementMode) -> Option<Self> {
        #[cfg(feature = "evaluation-collision-layout")]
        if display == DisplayMode::Pop && placement == PlacementMode::Collision {
            return None;
        }
        Some(Self { display, placement })
    }
    pub fn display(self) -> DisplayMode {
        self.display
    }
    pub fn placement(self) -> PlacementMode {
        self.placement
    }
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct PresentationWire {
    display: DisplayMode,
    placement: PlacementMode,
}
impl From<PresentationWire> for Presentation {
    fn from(wire: PresentationWire) -> Self {
        Self::new(wire.display, wire.placement).unwrap_or(Self {
            display: wire.display,
            placement: PlacementMode::Sequential,
        })
    }
}
impl From<Presentation> for PresentationWire {
    fn from(value: Presentation) -> Self {
        Self {
            display: value.display,
            placement: value.placement,
        }
    }
}

const LANES_PER_SLOT_PER_SECOND: usize = 3;
const MAX_SLOTS_PER_SECOND: usize = 6;
const COMMENTS_PER_SLOT: usize = 2;
const NANOS_PER_SECOND: u128 = 1_000_000_000;
const SCROLL_DENSITY_WINDOW: Duration = Duration::from_secs(3);

/// Only admitted input can supply a placement budget. Neither glyph
/// measurements nor the positions/lifetimes of displayed comments enter it.
pub(super) struct Admitted {
    sequence: u128,
    scroll_rows: usize,
}
impl Admitted {
    pub fn sequence(&self) -> u128 {
        self.sequence
    }
    pub fn scroll_rows(&self) -> usize {
        self.scroll_rows
    }
}

#[derive(Default)]
pub(super) struct Admission {
    slots: BTreeMap<u128, Counts>,
}
#[derive(Default)]
struct Counts {
    total: usize,
    scrolling: usize,
}
impl Admission {
    fn interval(lanes: usize) -> u128 {
        let rate = (lanes / LANES_PER_SLOT_PER_SECOND).clamp(1, MAX_SLOTS_PER_SECOND);
        NANOS_PER_SECOND / rate as u128
    }

    /// A temporal budget for normal density, bypassed by all-comment density.
    /// Independent of glyph widths, live positions and velocities; overlap is
    /// allowed. This replaces the default
    /// pairwise adjustment described in JP4695583 / JP6526304 / JP7178462;
    /// estimated terms are documented on PlacementMode::Collision above.
    pub fn reserve(
        &mut self,
        time: Duration,
        lanes: usize,
        position: Position,
        density: DensityMode,
    ) -> Option<Admitted> {
        // Use absolute nanosecond keys so resizing does not mix different units.
        let interval = Self::interval(lanes);
        let bucket = time.as_nanos() / interval;
        let start = bucket * interval;
        let end = start + interval;
        let preceding: usize = self
            .slots
            .range(start..end)
            .map(|(_, count)| count.total)
            .sum();
        match density {
            DensityMode::Normal if preceding >= COMMENTS_PER_SLOT => return None,
            DensityMode::Normal | DensityMode::All => {}
        }
        let count = self.slots.entry(time.as_nanos()).or_default();
        count.total += 1;
        count.scrolling += usize::from(position == Position::Right);
        let earliest = time.saturating_sub(super::MAX_LIFETIME).as_nanos();
        while self
            .slots
            .first_key_value()
            .is_some_and(|(&key, _)| key < earliest)
        {
            self.slots.pop_first();
        }
        let since = time
            .checked_sub(SCROLL_DENSITY_WINDOW)
            .map_or(Bound::Unbounded, |start| Bound::Excluded(start.as_nanos()));
        let scroll_rows = self
            .slots
            .range((since, Bound::Included(time.as_nanos())))
            .map(|(_, count)| count.scrolling)
            .sum::<usize>()
            .max(1);
        Some(Admitted {
            sequence: bucket * COMMENTS_PER_SLOT as u128 + preceding as u128,
            scroll_rows,
        })
    }

    /// Seed the temporal budget before the oldest comment a seek can restore.
    /// Include the whole first slot so a fixed comment that won that slot is
    /// not mistaken for a subsequent scrolling comment in the same slot.
    pub fn restore_density(
        &mut self,
        records: &[TimedComment],
        through: Duration,
        lanes: usize,
        density: DensityMode,
    ) {
        let interval = Self::interval(lanes);
        let since = through.saturating_sub(SCROLL_DENSITY_WINDOW).as_nanos();
        let first_slot = since / interval * interval;
        let start = records.partition_point(|record| record.time.as_nanos() < first_slot);
        let end = records.partition_point(|record| record.time <= through);
        for record in &records[start..end] {
            self.reserve(record.time, lanes, record.comment.position, density);
        }
    }
}

/// Stable FNV-1a, not RandomState: a seek must reconstruct the same trajectory.
pub(super) fn seed(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
    bytes.iter().fold(OFFSET, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(PRIME)
    })
}
struct Random(u64);
impl Random {
    fn unit(&mut self) -> f64 {
        // SplitMix64; deterministic variation, not used for security.
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^= z >> 31;
        (z >> 11) as f64 / ((1u64 << 53) as f64)
    }
    fn between(&mut self, min: f64, max: f64) -> f64 {
        min + (max - min) * self.unit()
    }
}

pub const POP_MAX_ROTATION_DEGREES: f64 = 20.;
const POP_MIN_SECONDS: f64 = 3.6;
const POP_MAX_SECONDS: f64 = 4.6;
const POP_MIN_RISE: f64 = 0.55;
const POP_MAX_RISE: f64 = 0.82;
const POP_MAX_DRIFT: f64 = 0.65;
const POP_FADE_START: f64 = 0.72;
const POP_ENTRY_FADE: f64 = 0.07;
const POP_LAUNCH_COLUMNS: usize = 5;
const POP_SIDE_MARGIN: f64 = 0.2;

#[derive(Clone, Copy, Debug)]
pub(super) struct Arc {
    anchor: f64,
    drift: f64,
    rise: f64,
    pub rotation: f64,
    pub duration: Duration,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Visual {
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
    pub opacity: f64,
}
impl Arc {
    pub fn new(seed: u64, sequence: u128, placement: PlacementMode, speed: f64) -> Self {
        let mut random = Random(seed);
        let column = placement.row(sequence, seed, POP_LAUNCH_COLUMNS);
        let anchor = POP_SIDE_MARGIN
            + (1. - 2. * POP_SIDE_MARGIN) * (column as f64 + random.between(0.2, 0.8))
                / POP_LAUNCH_COLUMNS as f64;
        Self {
            anchor,
            drift: random.between(-POP_MAX_DRIFT, POP_MAX_DRIFT),
            rise: random.between(POP_MIN_RISE, POP_MAX_RISE),
            rotation: random.between(-POP_MAX_ROTATION_DEGREES, POP_MAX_ROTATION_DEGREES),
            duration: Duration::from_secs_f64(
                random.between(POP_MIN_SECONDS, POP_MAX_SECONDS) / speed,
            ),
        }
    }
    pub fn at(self, age: Duration, width: f64, viewport: super::Viewport) -> Visual {
        let progress = (age.as_secs_f64() / self.duration.as_secs_f64()).clamp(0., 1.);
        let rotation_margin =
            width.min(viewport.width) * self.rotation.to_radians().sin().abs() / 2.;
        let margin = viewport.text_height + rotation_margin;
        let rise = (viewport.height - viewport.top()).max(0.) * self.rise + margin;
        Visual {
            x: self.anchor * viewport.width + self.drift * viewport.height * (progress - 0.5)
                - width / 2.,
            y: viewport.height + margin - 4. * rise * progress * (1. - progress),
            rotation: self.rotation,
            opacity: (progress / POP_ENTRY_FADE).min(1.)
                * ((1. - progress) / (1. - POP_FADE_START)).clamp(0., 1.),
        }
    }
}
