//! Validated half-open schedule intervals. Candidates remain owned by Snapshot.
//! Store O(n) boundaries, never every overlapping pair or a candidate list per cell.
use super::model::Program;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resolution {
    Gap,
    Single(usize),
    Conflict(usize),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    Known(u64),
    Unknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Segment {
    pub start: u64,
    pub end: u64,
    pub resolution: Resolution,
}
#[derive(Default, Debug)]
pub(super) struct Schedule {
    pub range: std::ops::Range<usize>,
    pub segments: Vec<Segment>,
    // An unknown end is bounded by the next distinct scheduled start for layout
    // only. This does not turn that bound into a known broadcast end.
    ends: Vec<u64>,
}
impl Schedule {
    pub fn build(programs: &[Program], range: std::ops::Range<usize>) -> Self {
        let entries = &programs[range.clone()];
        let mut ends = vec![0; entries.len()];
        let mut next_start = u64::MAX;
        let mut group_end = entries.len();
        while group_end > 0 {
            let time = entries[group_end - 1].start_at;
            let mut group_start = group_end - 1;
            while group_start > 0 && entries[group_start - 1].start_at == time {
                group_start -= 1;
            }
            for i in group_start..group_end {
                ends[i] = match entries[i].end() {
                    End::Known(end) => end,
                    End::Unknown => next_start,
                };
            }
            if entries[group_start..group_end]
                .iter()
                .any(|p| p.duration > 0)
            {
                next_start = time;
            }
            group_end = group_start;
        }
        let mut events = Vec::with_capacity(entries.len() * 2);
        for (i, program) in entries.iter().enumerate() {
            if program.start_at < ends[i] {
                events.push((program.start_at, true, range.start + i));
                events.push((ends[i], false, range.start + i));
            }
        }
        events.sort_unstable();
        let mut active = BTreeSet::new();
        let mut segments = Vec::with_capacity(events.len());
        let mut cursor = 0;
        while cursor < events.len() {
            let time = events[cursor].0;
            while cursor < events.len() && events[cursor].0 == time {
                let (_, starts, index) = events[cursor];
                if starts {
                    active.insert(index);
                } else {
                    active.remove(&index);
                }
                cursor += 1;
            }
            if let Some(&(end, _, _)) = events.get(cursor) {
                let resolution = match active.len() {
                    0 => Resolution::Gap,
                    1 => Resolution::Single(*active.first().expect("one active candidate")),
                    count => Resolution::Conflict(count),
                };
                if resolution != Resolution::Gap {
                    segments.push(Segment {
                        start: time,
                        end,
                        resolution,
                    });
                }
            }
        }
        Self {
            range,
            segments,
            ends,
        }
    }
    pub fn at(&self, time: u64) -> Option<&Segment> {
        let i = self
            .segments
            .partition_point(|s| s.start <= time)
            .checked_sub(1)?;
        self.segments.get(i).filter(|s| time < s.end)
    }
    pub fn candidates<'a>(
        &'a self,
        programs: &'a [Program],
        time: u64,
    ) -> impl Iterator<Item = usize> + 'a {
        self.range.clone().filter(move |&i| {
            programs[i].start_at <= time && time < self.ends[i - self.range.start]
        })
    }
    pub fn bytes(&self) -> usize {
        self.ends.capacity() * std::mem::size_of::<u64>()
            + self.segments.capacity() * std::mem::size_of::<Segment>()
    }
}

#[cfg(test)]
mod tests;
