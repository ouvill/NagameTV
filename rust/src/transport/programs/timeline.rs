use super::{Information, Observation, Program};
use std::collections::VecDeque;

const MAX_PENDING_OBSERVATIONS: usize = 128;

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
}
impl Presentation {
    pub fn serialize(self) -> (String, f64) {
        let data = self
            .program
            .map(|program| {
                let mut value = serde_json::to_value(&program).unwrap_or(serde_json::Value::Null);
                value["station"] = self.station.into();
                value["provider"] = self.provider.into();
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
        let now = time
            .and_then(|(pcr, unix)| Some(i128::from(unix) + (position - map(pcr)?) / 1_000_000));
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
        Presentation {
            program,
            station: station.clone(),
            provider: provider.clone(),
            progress,
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
    }
}
