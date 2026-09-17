use super::{Information, Observation, Program};
use std::collections::VecDeque;

const MAX_PENDING_OBSERVATIONS: usize = 128;
const NANOSECONDS_PER_MILLISECOND: i128 = 1_000_000;

/// Bounded observations in this seek generation. Later reads cannot overwrite
/// the currently presented event until their PCR reaches the video position.
#[derive(Default)]
pub(crate) struct Timeline {
    pending: VecDeque<Observation>,
    current: Option<Observation>,
}
#[derive(Default)]
pub(crate) struct Presentation {
    pub program: Option<Program>,
    pub station: String,
    pub provider: String,
    pub progress: f64,
    playback_range: Option<(i64, i64)>,
}
impl Presentation {
    pub fn serialize(self) -> (String, f64) {
        let data = self
            .program
            .map(|program| {
                let mut value = serde_json::to_value(&program).unwrap_or(serde_json::Value::Null);
                value["station"] = self.station.into();
                value["provider"] = self.provider.into();
                value["playbackStartMs"] = self.playback_range.map(|(start, _)| start).into();
                value["playbackEndMs"] = self.playback_range.map(|(_, end)| end).into();
                if !program.extended.is_empty() {
                    value["description"] =
                        format!("{}\n\n{}", program.description, program.extended)
                            .trim()
                            .into();
                }
                value
            })
            .unwrap_or(serde_json::Value::Null);
        (data.to_string(), self.progress)
    }
}
impl Timeline {
    pub fn push(&mut self, observation: Observation) {
        if self.pending.len() < MAX_PENDING_OBSERVATIONS {
            self.pending.push_back(observation);
        }
    }
    pub fn poll(
        &mut self,
        position: Option<u64>,
        map: impl Fn(u64) -> Option<i128>,
    ) -> Presentation {
        let Some(position) = position.map(i128::from) else {
            return Presentation::default();
        };
        while self
            .pending
            .front()
            .is_some_and(|entry| map(entry.pcr).is_some_and(|time| time <= position))
        {
            self.current = self.pending.pop_front();
        }
        let Some(Observation {
            information:
                Information {
                    station,
                    provider,
                    current,
                    next,
                    time,
                },
            ..
        }) = &self.current
        else {
            return Presentation::default();
        };
        let now = time.and_then(|(pcr, unix)| {
            Some(i128::from(unix) + (position - map(pcr)?) / NANOSECONDS_PER_MILLISECOND)
        });
        let contains = |program: &Program| {
            now.zip(program.start_at.zip(program.duration)).is_some_and(
                |(now, (start, duration))| {
                    now >= i128::from(start) && now < i128::from(start) + i128::from(duration)
                },
            )
        };
        let program = if now.is_some() {
            current
                .iter()
                .chain(next.iter())
                .find(|program| contains(program))
                .cloned()
                // Undefined schedules can still provide a useful current title.
                .or_else(|| {
                    current
                        .as_ref()
                        .filter(|program| program.start_at.is_none() || program.duration.is_none())
                        .cloned()
                })
        } else {
            current.clone()
        };
        let progress = program
            .as_ref()
            .and_then(|p| {
                Some(
                    (now? - i128::from(p.start_at?)) as f64 / p.duration.filter(|d| *d > 0)? as f64,
                )
            })
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        // Preserve negative starts when reception began in the middle of a show.
        // No broadcast clock means no mapping; a zero progress is not an anchor.
        let playback_range = program.as_ref().and_then(|program| {
            let start =
                position / NANOSECONDS_PER_MILLISECOND + i128::from(program.start_at?) - now?;
            let end = start + i128::from(program.duration.filter(|duration| *duration > 0)?);
            Some((i64::try_from(start).ok()?, i64::try_from(end).ok()?))
        });
        Presentation {
            program,
            station: station.clone(),
            provider: provider.clone(),
            progress,
            playback_range,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn event(id: u16, start: i64) -> Program {
        Program {
            event_id: id,
            service_id: 1,
            network_id: 1,
            transport_stream_id: 1,
            start_at: Some(start),
            duration: Some(10_000),
            name: id.to_string(),
            description: String::new(),
            extended: String::new(),
            genres: Vec::new(),
        }
    }
    #[test]
    fn read_ahead_never_advances_presentation_and_reset_forgets_old_event() {
        let mut timeline = Timeline::default();
        timeline.push(Observation {
            pcr: 0,
            information: Information {
                current: Some(event(1, 0)),
                next: Some(event(2, 10_000)),
                time: Some((0, 0)),
                ..Information::default()
            },
        });
        timeline.push(Observation {
            pcr: 900_000,
            information: Information {
                current: Some(event(2, 10_000)),
                time: Some((900_000, 10_000)),
                ..Information::default()
            },
        });
        let map = |pcr| Some(i128::from(pcr) * 1_000_000_000 / 90_000);
        assert_eq!(
            timeline
                .poll(Some(5_000_000_000), map)
                .program
                .unwrap()
                .event_id,
            1
        );
        assert_eq!(timeline.poll(Some(5_000_000_000), map).progress, 0.5);
        assert_eq!(
            timeline
                .poll(Some(11_000_000_000), map)
                .program
                .unwrap()
                .event_id,
            2
        );
        timeline = Timeline::default();
        assert!(timeline.poll(Some(2_000_000_000), map).program.is_none());
    }
    #[test]
    fn program_range_preserves_start_before_reception_and_missing_clock_is_null() {
        const JOINED_AT_MS: i64 = 4000;
        const PLAYHEAD_MS: u64 = 2000;
        const PROGRAM_DURATION_MS: i64 = 10_000;
        let mut timeline = Timeline::default();
        timeline.push(Observation {
            pcr: 0,
            information: Information {
                current: Some(event(1, 0)),
                time: Some((0, JOINED_AT_MS)),
                ..Information::default()
            },
        });
        let presentation = timeline.poll(
            Some(PLAYHEAD_MS * NANOSECONDS_PER_MILLISECOND as u64),
            |ns| Some(i128::from(ns)),
        );
        assert_eq!(
            presentation.playback_range,
            Some((-JOINED_AT_MS, PROGRAM_DURATION_MS - JOINED_AT_MS))
        );
        let (json, progress) = presentation.serialize();
        assert_eq!(progress, 0.6);
        let data: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(data["playbackStartMs"], -JOINED_AT_MS);
        assert_eq!(data["playbackEndMs"], PROGRAM_DURATION_MS - JOINED_AT_MS);
        timeline.push(Observation {
            pcr: 0,
            information: Information {
                current: Some(event(1, 0)),
                ..Information::default()
            },
        });
        let (json, _) = timeline
            .poll(Some(0), |ns| Some(i128::from(ns)))
            .serialize();
        let data: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(data["playbackStartMs"].is_null());
        assert!(data["playbackEndMs"].is_null());
    }

    #[test]
    fn missing_clock_or_presentation_time_never_guesses_wall_time() {
        let mut timeline = Timeline::default();
        timeline.push(Observation {
            pcr: 90,
            information: Information {
                current: Some(event(1, 0)),
                ..Information::default()
            },
        });
        assert!(timeline.poll(Some(1_000_000), |_| None).program.is_none());
        assert!(timeline.poll(None, |_| Some(0)).program.is_none());
        let p = timeline.poll(Some(1_000_000), |_| Some(0));
        assert_eq!(p.program.unwrap().event_id, 1);
        assert_eq!(p.progress, 0.0);
        assert!(p.playback_range.is_none());
    }
}
