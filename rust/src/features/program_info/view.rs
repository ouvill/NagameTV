//! Read-only day projection: interval references, never copies of program bodies.
use super::{
    guide::DayWindow,
    model::{Program, Snapshot},
    schedule::{Resolution, Segment},
    watch,
};
use crate::channels::Channel;
use std::sync::Arc;

pub struct Cell {
    pub channel: usize,
    pub segment: Segment,
    pub begin: u64,
    pub end: u64,
    pub key: String,
}
#[derive(Default)]
pub struct View {
    snapshot: Snapshot,
    channels: Arc<[Channel]>,
    generation: u64,
    cells: Vec<Cell>,
    keys: Vec<usize>,
}
impl View {
    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }
    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }
    pub fn new(
        snapshot: Snapshot,
        channels: Arc<[Channel]>,
        window: DayWindow,
        generation: u64,
    ) -> Result<Self, serde_json::Error> {
        let mut cells = Vec::new();
        for (channel_index, channel) in channels.iter().enumerate() {
            for segment in snapshot.segments(channel.broadcast) {
                if window.overlaps(segment.start, segment.end - segment.start) {
                    cells.push(Cell {
                        channel: channel_index,
                        segment: *segment,
                        begin: segment.start.max(window.start()),
                        end: segment.end.min(window.end()),
                        key: watch::Identity::segment(generation, channel, segment, &snapshot)
                            .key()?,
                    });
                }
            }
        }
        let mut keys: Vec<_> = (0..cells.len()).collect();
        keys.sort_unstable_by(|&a, &b| cells[a].key.cmp(&cells[b].key));
        Ok(Self {
            snapshot,
            channels,
            generation,
            cells,
            keys,
        })
    }
    pub fn program(&self, cell: &Cell) -> Option<&Program> {
        match cell.segment.resolution {
            Resolution::Single(i) => Some(self.snapshot.program(i)),
            Resolution::Gap | Resolution::Conflict(_) => None,
        }
    }
    pub fn find(&self, key: &str) -> Option<usize> {
        self.keys
            .binary_search_by(|&i| self.cells[i].key.as_str().cmp(key))
            .ok() // A key from a retired projection is normally absent.
            .map(|i| self.keys[i])
    }
    pub fn nearest(&self, channel: usize, time: u64) -> Option<usize> {
        self.cells
            .iter()
            .enumerate()
            .filter(|(_, c)| c.channel == channel)
            .min_by_key(|(_, c)| {
                if time < c.begin {
                    c.begin - time
                } else if time >= c.end {
                    time.saturating_sub(c.end).saturating_add(1)
                } else {
                    0
                }
            })
            .map(|(i, _)| i)
    }
    pub fn adjacent(&self, channel: usize, key: &str, step: i32) -> Option<usize> {
        let row = self.find(key)?;
        let next = if step < 0 {
            row.saturating_sub(1)
        } else {
            row.saturating_add(1)
        };
        Some(
            if self.cells.get(next).is_some_and(|c| c.channel == channel) {
                next
            } else {
                row
            },
        )
    }
    pub fn candidates(&self, key: &str) -> Vec<usize> {
        self.find(key).map_or_else(Vec::new, |row| {
            let cell = &self.cells[row];
            self.snapshot
                .candidates(self.channels[cell.channel].broadcast, cell.segment.start)
        })
    }
    pub fn action(&self, key: &str, now: u64) -> Option<watch::Action> {
        watch::resolve(&self.snapshot, self.generation, key, &self.channels, now)
            .ok()
            // A non-current/retired selection is normal absence of an action, not an operational failure.
            .map(|(_, action)| action)
    }
}
