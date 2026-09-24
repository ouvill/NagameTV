//! Acquisition uses programme schedules or a whole video's measured range;
//! playback clocks never grow to match it.
use super::{Demand, Interval, LOOKBACK_SECONDS, Source};
use serde::{Deserialize, Serialize};

pub const PROGRAM_PADDING_SECONDS: i64 = 120;
pub const WINDOW_SECONDS: i64 = 30 * 60;
const PROGRAM_CHUNK_SECONDS: i64 = 6 * 60 * 60;
const NEXT_PROGRAM_SECONDS: i64 = 60;
const REVISION_SETTLE_SECONDS: i64 = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgramId {
    pub network: u16,
    pub transport: u16,
    pub service: u16,
    pub event: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    id: ProgramId,
    range: Interval,
}
impl Program {
    pub fn new(id: ProgramId, start_seconds: i64, duration_seconds: i64) -> Option<Self> {
        let range = Interval::new(start_seconds, start_seconds.checked_add(duration_seconds)?)?;
        // Padding must remain representable in the database's microsecond clock.
        Interval::new(range.start, range.end.checked_add(PROGRAM_PADDING_SECONDS)?)?;
        Some(Self { id, range })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Recording {
    Discovering,
    /// A continuous recording with a known start and measured video duration.
    Whole(Interval),
    Observed {
        current: Option<Program>,
        next: Option<Program>,
        utc_seconds: Option<i64>,
        at_start: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Basis {
    Recording {
        start: i64,
    },
    Program {
        id: ProgramId,
        start: i64,
        chunk: i64,
    },
    Window {
        start: i64,
    },
    Live {
        clock: String,
        start: i64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Target {
    pub channel: u16,
    pub range: Interval,
    basis: Basis,
}
impl Target {
    pub fn key(&self) -> String {
        format!(
            "{}:{}",
            self.channel,
            serde_json::to_string(&self.basis).expect("target key")
        )
    }
    pub fn live_clock(&self) -> Option<&str> {
        match &self.basis {
            Basis::Live { clock, .. } => Some(clock),
            Basis::Recording { .. } | Basis::Program { .. } | Basis::Window { .. } => None,
        }
    }
    fn program(channel: u16, program: &Program, utc: Option<i64>) -> Self {
        let offset = utc
            .unwrap_or(program.range.start)
            .saturating_sub(program.range.start)
            .max(0);
        let chunk = (offset / PROGRAM_CHUNK_SECONDS) * PROGRAM_CHUNK_SECONDS;
        let start = program
            .range
            .start
            .saturating_add(chunk)
            .min(program.range.end - 1);
        let end = start
            .saturating_add(PROGRAM_CHUNK_SECONDS)
            .min(program.range.end);
        Self {
            channel,
            range: Interval::new(
                start.saturating_sub(PROGRAM_PADDING_SECONDS).max(0),
                end.saturating_add(PROGRAM_PADDING_SECONDS),
            )
            .expect("validated programme"),
            basis: Basis::Program {
                id: program.id,
                start: program.range.start,
                chunk,
            },
        }
    }
    fn window(channel: u16, utc: i64) -> Option<Self> {
        let start = utc.div_euclid(WINDOW_SECONDS) * WINDOW_SECONDS;
        let windows = if utc - start >= WINDOW_SECONDS - PROGRAM_PADDING_SECONDS {
            2
        } else {
            1
        };
        Some(Self {
            channel,
            range: Interval::new(
                (start - LOOKBACK_SECONDS).max(0),
                start.checked_add(windows * WINDOW_SECONDS + PROGRAM_PADDING_SECONDS)?,
            )?,
            basis: Basis::Window { start },
        })
    }
}

#[derive(Default)]
pub(super) struct Planner {
    source: Option<u64>,
    selected: Option<Target>,
    revision: Option<(Target, i64)>,
}
impl Planner {
    pub fn update(&mut self, demand: &Demand, now: i64) -> Option<Target> {
        if self.source != Some(demand.source) {
            self.source = Some(demand.source);
            self.selected = None;
            self.revision = None;
        }
        let view_utc = demand
            .view
            .as_ref()
            .map(|view| view.interval.start + LOOKBACK_SECONDS);
        let candidate = match &demand.source_range {
            Source::Pending | Source::Recording(Recording::Discovering) => None,
            Source::Recording(Recording::Whole(range)) => Some(Target {
                channel: demand.channel,
                range: *range,
                basis: Basis::Recording { start: range.start },
            }),
            Source::Recording(Recording::Observed {
                current,
                next,
                utc_seconds,
                at_start,
            }) => {
                let utc = view_utc.or(*utc_seconds);
                let following = at_start.then(|| next.as_ref()).flatten().filter(|next| {
                    utc.is_some_and(|utc| {
                        next.range.start >= utc && next.range.start - utc <= PROGRAM_PADDING_SECONDS
                    }) && current
                        .as_ref()
                        .is_some_and(|current| current.range.end == next.range.start)
                });
                let program = following.or(current.as_ref()).filter(|program| {
                    utc.is_none_or(|utc| utc < program.range.end + PROGRAM_PADDING_SECONDS)
                });
                program
                    .map(|program| Target::program(demand.channel, program, utc))
                    .or_else(|| utc.and_then(|utc| Target::window(demand.channel, utc)))
            }
            Source::Live { at_edge: true, .. } => None,
            Source::Live { spans, .. } => {
                let view = demand.view.as_ref()?;
                let utc = view_utc?;
                let span = spans.iter().find(|span| {
                    span.channel == demand.channel
                        && span.key == view.clock_key
                        && span.interval().is_some_and(|range| range.contains(utc))
                })?;
                let mut target = Target::window(demand.channel, utc)?;
                target.range = target.range.intersection(span.interval()?)?;
                target.range = Interval::new(
                    target.range.start,
                    target.range.end.min(super::archive_end(now)),
                )?;
                target.basis = Basis::Live {
                    clock: span.key.clone(),
                    start: target.range.start,
                };
                Some(target)
            }
        };
        let candidate = candidate?;
        if let Some(previous) = &self.selected
            && previous.channel == candidate.channel
            && matches!(demand.source_range, Source::Recording(_))
        {
            if previous.basis == candidate.basis && previous.range != candidate.range {
                if candidate.range.start >= previous.range.start
                    && candidate.range.end <= previous.range.end
                {
                    return Some(previous.clone());
                }
                let revision = self
                    .revision
                    .get_or_insert_with(|| (candidate.clone(), now));
                if revision.0 != candidate {
                    *revision = (candidate.clone(), now);
                }
                if now < revision.1 + REVISION_SETTLE_SECONDS {
                    return Some(previous.clone());
                }
            } else if previous.basis != candidate.basis
                && matches!(previous.basis, Basis::Program { .. })
                && view_utc.is_some_and(|utc| {
                    previous.range.contains(utc) && utc < previous.range.end - NEXT_PROGRAM_SECONDS
                })
            {
                // A few seconds of the following programme in recording padding
                // do not authorize a download of that entire programme.
                return Some(previous.clone());
            }
        }
        self.revision = None;
        self.selected = Some(candidate.clone());
        Some(candidate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::{Recording, View};
    const START: i64 = 1_000_800;
    const MOVIE_SECONDS: i64 = 114 * 60;
    fn program(event: u16, start: i64, seconds: i64) -> Program {
        Program::new(
            ProgramId {
                network: 32725,
                transport: 32725,
                service: 2088,
                event,
            },
            start,
            seconds,
        )
        .unwrap()
    }
    fn demand(
        utc: Option<i64>,
        current: Option<Program>,
        next: Option<Program>,
        at_start: bool,
    ) -> Demand {
        Demand {
            source: 1,
            channel: 4,
            fetch: true,
            view: utc.map(|utc| View {
                clock_key: "movie".into(),
                interval: Interval::new(utc - LOOKBACK_SECONDS, utc + 120).unwrap(),
            }),
            source_range: Source::Recording(Recording::Observed {
                current,
                next,
                utc_seconds: utc,
                at_start,
            }),
        }
    }
    #[test]
    fn whole_video_is_selected_before_playback_and_stays_selected_after_seeks() {
        const LONG_VIDEO_SECONDS: i64 = 8 * 60 * 60;
        let range = Interval::new(START, START + LONG_VIDEO_SECONDS).unwrap();
        let mut input = demand(None, None, None, false);
        input.source_range = Source::Recording(Recording::Whole(range));
        let mut planner = Planner::default();
        let selected = planner.update(&input, 0).unwrap();
        assert_eq!(selected.range, range);
        assert!(selected.live_clock().is_none());
        for offset in [LONG_VIDEO_SECONDS - 1, WINDOW_SECONDS, 0] {
            input.view = demand(Some(START + offset), None, None, false).view;
            assert_eq!(planner.update(&input, 1), Some(selected.clone()));
        }
        // Reopening the video uses the same persistent acquisition target.
        input.source += 1;
        input.view = None;
        assert_eq!(planner.update(&input, 2), Some(selected));
    }
    #[test]
    fn following_programme_supplies_one_movie_scope_without_a_complete_clock_map() {
        let previous = program(8061, START - 360, 360);
        let movie = program(8062, START, MOVIE_SECONDS);
        let mut planner = Planner::default();
        let selected = planner
            .update(
                &demand(Some(START - 8), Some(previous), Some(movie.clone()), true),
                0,
            )
            .unwrap();
        assert_eq!(
            selected.range,
            Interval::new(START - 120, START + MOVIE_SECONDS + 120).unwrap()
        );
        for offset in [40 * 60, 90 * 60, 1, MOVIE_SECONDS - 1] {
            assert_eq!(
                planner.update(
                    &demand(Some(START + offset), Some(movie.clone()), None, false),
                    10
                ),
                Some(selected.clone())
            );
        }
        let after = program(8063, START + MOVIE_SECONDS, 360);
        assert_eq!(
            planner.update(
                &demand(
                    Some(START + MOVIE_SECONDS + 2),
                    Some(after.clone()),
                    None,
                    false
                ),
                20
            ),
            Some(selected)
        );
        assert_ne!(
            planner
                .update(
                    &demand(Some(START + MOVIE_SECONDS + 61), Some(after), None, false),
                    21
                )
                .unwrap()
                .range
                .start,
            START - 120
        );
    }
    #[test]
    fn programme_can_be_fetched_before_clock_and_initial_discovery_waits() {
        let movie = program(8062, START, MOVIE_SECONDS);
        let mut planner = Planner::default();
        assert!(
            planner
                .update(&demand(None, Some(movie), None, false), 0)
                .is_some()
        );
        let mut unknown = demand(None, None, None, false);
        unknown.source_range = Source::Recording(Recording::Discovering);
        assert!(planner.update(&unknown, 1).is_none());
        assert!(
            planner
                .update(&demand(None, None, None, false), 2)
                .is_none()
        );
    }
    #[test]
    fn fallback_is_utc_anchored_and_prefetches_the_next_fixed_window() {
        let start = START.div_euclid(WINDOW_SECONDS) * WINDOW_SECONDS;
        let mut planner = Planner::default();
        let first = planner
            .update(&demand(Some(start + 100), None, None, false), 0)
            .unwrap();
        assert_eq!(
            first.range,
            Interval::new(start - LOOKBACK_SECONDS, start + WINDOW_SECONDS + 120).unwrap()
        );
        assert_eq!(
            planner.update(&demand(Some(start + 800), None, None, false), 1),
            Some(first.clone())
        );
        let near = demand(Some(start + WINDOW_SECONDS - 120), None, None, false);
        assert_eq!(planner.update(&near, 2), Some(first));
        let next = planner.update(&near, 7).unwrap();
        assert_eq!(next.range.end, start + 2 * WINDOW_SECONDS + 120);
    }
    #[test]
    fn extension_is_coalesced_but_seeking_to_a_different_programme_is_immediate() {
        let mut planner = Planner::default();
        let first = planner
            .update(
                &demand(Some(START + 30), Some(program(1, START, 3600)), None, false),
                0,
            )
            .unwrap();
        let revised = demand(Some(START + 31), Some(program(1, START, 4200)), None, false);
        assert_eq!(planner.update(&revised, 1), Some(first));
        assert_eq!(
            planner.update(&revised, 6).unwrap().range.end,
            START + 4200 + 120
        );
        let other = planner
            .update(
                &demand(
                    Some(START + 10_000),
                    Some(program(2, START + 9000, 3600)),
                    None,
                    false,
                ),
                7,
            )
            .unwrap();
        assert_eq!(other.range.start, START + 9000 - 120);
        assert!(
            Program::new(
                ProgramId {
                    network: 0,
                    transport: 0,
                    service: 0,
                    event: 0
                },
                START,
                0
            )
            .is_none()
        );
    }
    #[test]
    fn long_programmes_are_bounded_and_id_reuse_does_not_alias_another_broadcast() {
        let mut planner = Planner::default();
        let first = planner
            .update(
                &demand(Some(START), Some(program(1, START, 24 * 3600)), None, false),
                0,
            )
            .unwrap();
        assert_eq!(
            first.range.end - first.range.start,
            PROGRAM_CHUNK_SECONDS + 2 * PROGRAM_PADDING_SECONDS
        );
        let second = planner
            .update(
                &demand(
                    Some(START + 86400),
                    Some(program(1, START + 86400, 3600)),
                    None,
                    false,
                ),
                1,
            )
            .unwrap();
        assert_ne!(first.key(), second.key());
    }
}
