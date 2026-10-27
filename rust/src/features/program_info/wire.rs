//! Normalize optional API metadata before building authoritative schedules.
use super::{Error, MAX_PROGRAMS, details, genre::Genre, model::Program};
use crate::{audio::Descriptor, channels::BroadcastService};
use serde::Deserialize;
use std::collections::BTreeMap;

const UNKNOWN_DURATION_MS: u64 = 1;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    id: u64,
    event_id: Option<u16>,
    #[serde(flatten)]
    service: BroadcastService,
    start_at: Option<u64>,
    duration: Option<u64>,
    name: Option<String>,
    description: Option<String>,
    extended: Option<details::Extended>,
    video: Option<details::Video>,
    series: Option<details::Series>,
    is_free: Option<bool>,
    #[serde(default)]
    genres: Genre,
    #[serde(default)]
    audios: Box<[Descriptor]>,
    #[serde(default)]
    related_items: Box<[Related]>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Related {
    #[serde(rename = "type")]
    kind: Relation,
    network_id: Option<u16>,
    service_id: u16,
    event_id: u16,
}
#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum Relation {
    Shared,
    #[serde(other)]
    Other,
}

fn descriptive(program: &Program) -> bool {
    program.name.as_ref().is_some_and(|s| !s.is_empty())
        || program.description.as_ref().is_some_and(|s| !s.is_empty())
        || program.extended.is_some()
        || program.video.is_some()
        || program.genre != Genre::Unknown
        || !program.audios.is_empty()
        || program
            .series
            .as_ref()
            .is_some_and(|s| s.name.is_some() || s.episode.is_some() || s.last_episode.is_some())
}

pub(super) fn parse(bytes: &[u8]) -> Result<Vec<Program>, Error> {
    let entries: Vec<Entry> = crate::json::from_slice(bytes)?;
    if entries.len() > MAX_PROGRAMS {
        return Err(Error::TooManyPrograms {
            actual: entries.len(),
            limit: MAX_PROGRAMS,
        });
    }
    let mut dropped = 0;
    let mut programs = Vec::with_capacity(entries.len());
    let mut relations = Vec::with_capacity(entries.len());
    for entry in entries {
        let Some(start_at) = entry.start_at else {
            dropped += 1;
            continue;
        };
        let series = entry.series.map(|mut series| {
            series.name = series.name.filter(|name| !name.is_empty());
            // ARIB STD-B10 6.2.33: zero means unspecified, not episode zero.
            series.episode = series.episode.filter(|episode| *episode != 0);
            series.last_episode = series.last_episode.filter(|episode| *episode != 0);
            series
        });
        programs.push(Program {
            id: entry.id,
            event_id: entry.event_id,
            service: entry.service,
            start_at,
            duration: entry.duration.unwrap_or(UNKNOWN_DURATION_MS),
            name: entry.name,
            description: entry.description,
            extended: entry.extended,
            video: entry.video,
            series,
            is_free: entry.is_free,
            genre: entry.genres,
            audios: entry.audios,
        });
        relations.push(entry.related_items);
    }
    if dropped != 0 {
        tracing::warn!(
            dropped_programs = dropped,
            "Program guide fetch discarded programs without startAt"
        );
    }

    // EIT event IDs are reused. Only copy from an unambiguous occurrence at the
    // same start time; never let an old or conflicting revision supply a title.
    let mut sources = BTreeMap::new();
    for (index, program) in programs.iter().enumerate().filter(|(_, p)| descriptive(p)) {
        if let Some(event) = program.event_id {
            let key = (program.service(), event, program.start_at);
            sources
                .entry(key)
                .and_modify(|source: &mut Option<usize>| {
                    if source.is_some_and(|previous| programs[previous] != *program) {
                        *source = None;
                    }
                })
                .or_insert(Some(index));
        }
    }
    // Sources contain only original descriptive records, so cycles/self-links
    // cannot manufacture metadata or make the result depend on input order.
    for (index, related) in relations.iter().enumerate() {
        if descriptive(&programs[index]) {
            continue;
        }
        let target = &programs[index];
        let source = related
            .iter()
            .filter(|r| matches!(r.kind, Relation::Shared))
            .find_map(|r| {
                let service = BroadcastService {
                    network_id: r.network_id.unwrap_or(target.service.network_id),
                    service_id: r.service_id,
                };
                sources
                    .get(&(service, r.event_id, target.start_at))
                    .copied()
                    .flatten()
            });
        if let Some(source) = source {
            let source = programs[source].clone();
            let target = &mut programs[index];
            target.name = source.name;
            target.description = source.description;
            target.extended = source.extended;
            target.genre = source.genre;
            target.video = source.video;
            target.audios = source.audios;
            target.series = source.series;
        }
    }
    Ok(programs)
}

#[cfg(test)]
mod tests {
    use super::super::{model, schedule::End, visibility};
    use super::*;
    type TestResult = Result<(), Box<dyn std::error::Error>>;
    const REAL_SHARED: &[u8] = include_bytes!("fixtures/mirakc-shared.json");

    #[test]
    fn real_shared_events_keep_identity_and_simulcast_grouping() -> TestResult {
        let snapshot = model::parse(REAL_SHARED)?;
        assert_eq!(snapshot.len(), 3);
        let main = snapshot.program(0);
        assert_eq!(main.service.service_id, 2056);
        for index in 1..snapshot.len() {
            let sub = snapshot.program(index);
            assert_ne!(sub.id, main.id);
            assert_ne!(sub.service, main.service);
            assert_eq!(sub.event_id, main.event_id);
            assert_eq!((sub.start_at, sub.duration), (main.start_at, main.duration));
            assert_eq!(sub.name, main.name);
            assert_eq!(sub.description, main.description);
            assert_eq!(sub.extended, main.extended);
            assert_eq!(sub.genre, main.genre);
            assert_eq!(sub.video, main.video);
            assert_eq!(sub.audios, main.audios);
        }
        assert!(!main.audios.is_empty());
        assert!(
            main.audios
                .iter()
                .all(|audio| audio.component_tag.is_none())
        );
        assert!(crate::audio::matching(&main.audios, Some(0)).is_none());
        let channels = crate::channels::parse(br#"[
            {"id":1,"name":"Main","type":1,"networkId":32721,"serviceId":2056,"channel":{"type":"GR","channel":"27"}},
            {"id":2,"name":"Sub","type":1,"networkId":32721,"serviceId":2057,"channel":{"type":"GR","channel":"27"}}
        ]"#)?;
        assert_eq!(
            visibility::indices(&channels, |c| snapshot.current(c.broadcast, main.start_at)),
            [0]
        );
        Ok(())
    }

    #[test]
    fn nullable_times_drop_only_unplaceable_entries_and_log_one_count() -> TestResult {
        let (result, output) = crate::logging::tests::capture_output("info", || {
            model::parse(
                br#"[
            {"id":1,"networkId":1,"serviceId":1,"startAt":null,"duration":null},
            {"id":2,"networkId":1,"serviceId":1,"startAt":null,"duration":3000},
            {"id":3,"networkId":1,"serviceId":1,"startAt":100,"duration":null},
            {"id":4,"networkId":1,"serviceId":2,"startAt":100,"duration":1},
            {"id":5,"networkId":1,"serviceId":3,"startAt":100,"duration":3000}
        ]"#,
            )
        });
        let snapshot = result?;
        assert_eq!(snapshot.len(), 3);
        assert_eq!(snapshot.unknown_ends(), 2);
        assert_eq!(snapshot.program(0).end(), End::Unknown);
        assert_eq!(snapshot.program(0).progress(200), 0.0);
        assert_eq!(output.lines().count(), 1, "{output}");
        assert!(output.contains("dropped_programs=2"), "{output}");
        assert!(
            model::parse(
                br#"[{"id":1,"networkId":1,"serviceId":1,"startAt":"bad","duration":null}]"#
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn series_defaults_and_shared_details_are_normalized_without_overwriting_local_data()
    -> TestResult {
        let input = br#"[
            {"id":1,"eventId":7,"networkId":1,"serviceId":1,"startAt":100,"duration":50,"name":"Main",
             "series":{"name":"","episode":0,"lastEpisode":0,"expireAt":-1}},
            {"id":2,"eventId":8,"networkId":2,"serviceId":1,"startAt":100,"duration":70,"isFree":false,
             "relatedItems":[{"type":"shared","networkId":1,"serviceId":1,"eventId":7}]},
            {"id":3,"eventId":7,"networkId":1,"serviceId":2,"startAt":100,"duration":50,"name":"Local",
             "series":{"name":"Series","episode":3,"lastEpisode":12},
             "relatedItems":[{"type":"shared","serviceId":1,"eventId":7}]}
        ]"#;
        let programs = parse(input)?;
        let sub = &programs[1];
        assert_eq!(sub.name.as_deref(), Some("Main"));
        assert_eq!(
            (sub.event_id, sub.duration, sub.is_free),
            (Some(8), 70, Some(false))
        );
        let series = sub.series.as_ref().ok_or("missing shared series")?;
        assert_eq!(
            (&series.name, series.episode, series.last_episode),
            (&None, None, None)
        );
        assert_eq!(programs[2].name.as_deref(), Some("Local"));
        assert_eq!(programs[2].series.as_ref().and_then(|s| s.episode), Some(3));
        Ok(())
    }

    #[test]
    fn missing_cyclic_relay_stale_and_ambiguous_references_do_not_invent_metadata() -> TestResult {
        let mut entries: Vec<serde_json::Value> = crate::json::from_slice(REAL_SHARED)?;
        for case in ["missing", "cycle", "relay", "stale", "ambiguous"] {
            let mut input = entries.clone();
            match case {
                "missing" => {
                    input.remove(0);
                }
                "cycle" => {
                    input.remove(0);
                    input[0]["relatedItems"][0]["serviceId"] = 2058.into();
                    input[1]["relatedItems"][0]["serviceId"] = 2057.into();
                }
                "relay" => {
                    input[1]["relatedItems"][0]["type"] = "relay".into();
                    input[2]["relatedItems"][0]["type"] = "relay".into();
                }
                "stale" => {
                    input[0]["startAt"] = 1.into();
                }
                "ambiguous" => {
                    let mut other = input[0].clone();
                    other["name"] = "Conflicting title".into();
                    input.push(other);
                }
                _ => unreachable!(),
            }
            let programs = parse(&serde_json::to_vec(&input)?)?;
            for program in programs.iter().filter(|p| p.service.service_id != 2056) {
                assert!(program.name.is_none(), "{case}: {program:?}");
            }
        }
        entries.reverse();
        assert!(
            parse(&serde_json::to_vec(&entries)?)?
                .iter()
                .all(|p| p.name.is_some())
        );
        Ok(())
    }
}
