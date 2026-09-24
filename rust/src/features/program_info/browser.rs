//! Borrow current events from the shared EPG; retain only change-detection keys.
use super::{
    model::{Program, Snapshot},
    schedule::{Resolution, Segment},
};
use crate::channels::{BroadcastService, Channel};
use serde::Serialize;

#[derive(PartialEq, Eq)]
struct Key {
    service: Option<BroadcastService>,
    segment: Option<Segment>,
    event: Option<(u64, u64, u64)>,
    next: Option<(u64, u64, u64)>,
}
impl Key {
    fn new(
        channel: &Channel,
        program: Option<&Program>,
        next: Option<&Program>,
        segment: Option<&Segment>,
    ) -> Self {
        Self {
            service: channel.broadcast,
            segment: segment.copied(),
            event: program.map(|p| (p.id, p.start_at, p.duration)),
            next: next.map(|p| (p.id, p.start_at, p.duration)),
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Summary<'a> {
    name: Option<&'a str>,
    start_at: u64,
    duration: u64,
}
impl<'a> From<&'a Program> for Summary<'a> {
    fn from(program: &'a Program) -> Self {
        Self {
            name: program.name.as_deref(),
            start_at: program.start_at,
            duration: program.duration,
        }
    }
}
#[derive(Serialize)]
struct Card<'a> {
    #[serde(flatten)]
    current: Summary<'a>,
    next: Option<Summary<'a>>,
    #[serde(rename = "scheduleState", skip_serializing_if = "Option::is_none")]
    state: Option<&'static str>,
}
#[derive(Default)]
pub struct Projection {
    revision: Option<u64>,
    keys: Vec<Key>,
    pub visible_json: String,
}
impl Projection {
    pub(super) fn update(
        &mut self,
        snapshot: &Snapshot,
        revision: u64,
        channels: &[Channel],
        now: Option<u64>,
    ) -> Result<Option<String>, serde_json::Error> {
        let current =
            |channel: &Channel| now.and_then(|time| snapshot.current(channel.broadcast, time));
        let segment =
            |channel: &Channel| now.and_then(|time| snapshot.segment(channel.broadcast, time));
        let next = |channel: &Channel| now.and_then(|time| snapshot.next(channel.broadcast, time));
        if self.revision == Some(revision)
            && self.keys.len() == channels.len()
            && self.keys.iter().zip(channels).all(|(key, channel)| {
                *key == Key::new(channel, current(channel), next(channel), segment(channel))
            })
        {
            return Ok(None);
        }
        let cards: Vec<_> = channels
            .iter()
            .map(|channel| {
                current(channel)
                    .map(|program| Card {
                        current: program.into(),
                        next: next(channel).map(Into::into),
                        state: None,
                    })
                    .or_else(|| {
                        segment(channel).map(|slot| Card {
                            current: Summary {
                                name: match slot.resolution {
                                    Resolution::Single(i) => snapshot.program(i).name.as_deref(),
                                    Resolution::Conflict(_) | Resolution::Gap => None,
                                },
                                start_at: slot.start,
                                duration: 0,
                            },
                            next: next(channel).map(Into::into),
                            state: Some(match slot.resolution {
                                Resolution::Single(_) => "unknownEnd",
                                Resolution::Conflict(_) => "conflict",
                                Resolution::Gap => "gap",
                            }),
                        })
                    })
            })
            .collect();
        let json = serde_json::to_string(&cards)?;
        let visible = super::visibility::with_uncertain(snapshot, channels, now);
        let visible_json = serde_json::to_string(&visible)?;
        // Commit keys only after successful serialization so errors remain retryable.
        self.keys = channels
            .iter()
            .map(|channel| Key::new(channel, current(channel), next(channel), segment(channel)))
            .collect();
        self.visible_json = visible_json;
        self.revision = Some(revision);
        Ok(Some(json))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uncertain_programs_do_not_hide_subchannels_or_choose_a_current_title()
    -> Result<(), Box<dyn std::error::Error>> {
        let channels = crate::channels::parse(br#"[
            {"id":1,"networkId":1,"serviceId":1,"name":"A","type":1,"channel":{"type":"GR","channel":"27"}},
            {"id":2,"networkId":1,"serviceId":2,"name":"B","type":1,"channel":{"type":"GR","channel":"27"}}
        ]"#)?;
        let snapshot = super::super::model::parse(br#"[
            {"id":1,"eventId":7,"networkId":1,"serviceId":1,"startAt":100,"duration":100,"name":"A"},
            {"id":2,"eventId":8,"networkId":1,"serviceId":1,"startAt":150,"duration":100,"name":"B"},
            {"id":3,"eventId":7,"networkId":1,"serviceId":2,"startAt":100,"duration":100,"name":"Simulcast"},
            {"id":4,"eventId":9,"networkId":1,"serviceId":2,"startAt":200,"duration":1,"name":"Unknown end"}
        ]"#)?;
        let mut projection = Projection::default();
        projection.update(&snapshot, 0, &channels, Some(100))?;
        assert_eq!(projection.visible_json, "[0]");
        let conflict: serde_json::Value = serde_json::from_str(
            &projection
                .update(&snapshot, 0, &channels, Some(150))?
                .unwrap(),
        )?;
        assert_eq!(projection.visible_json, "[0,1]");
        assert_eq!(conflict[0]["scheduleState"], "conflict");
        assert!(conflict[0]["name"].is_null());
        assert!(conflict[0]["next"].is_null());
        let unknown: serde_json::Value = serde_json::from_str(
            &projection
                .update(&snapshot, 0, &channels, Some(200))?
                .unwrap(),
        )?;
        assert_eq!(unknown[1]["scheduleState"], "unknownEnd");
        assert_eq!(unknown[1]["duration"], 0);
        assert_eq!(projection.visible_json, "[0,1]");
        let mut current = super::super::presentation::Projection::default();
        let detail: serde_json::Value = serde_json::from_str(
            &current
                .update(&snapshot, 0, channels[0].broadcast, 150)?
                .data
                .unwrap(),
        )?;
        assert_eq!(detail["scheduleState"], "conflict");
        assert!(detail["name"].is_null());
        assert!(
            current
                .update(&snapshot, 0, channels[0].broadcast, 151)?
                .data
                .is_none()
        );
        let detail: serde_json::Value = serde_json::from_str(
            &current
                .update(&snapshot, 0, channels[1].broadcast, 200)?
                .data
                .unwrap(),
        )?;
        assert_eq!(detail["endUnknown"], true);
        assert_eq!(detail["duration"], 1);
        Ok(())
    }
    #[test]
    fn snapshots_change_only_on_boundary_revision_or_catalog()
    -> Result<(), Box<dyn std::error::Error>> {
        let channels = crate::channels::parse(
            br#"[
            {"id":99,"networkId":1,"serviceId":2,"name":"A","type":1},
            {"id":100,"networkId":1,"serviceId":3,"name":"B","type":1}
        ]"#,
        )?;
        let snapshot = super::super::model::parse(br#"[
            {"id":1,"networkId":1,"serviceId":2,"startAt":100,"duration":100,"name":"First","description":"not copied"},
            {"id":2,"networkId":1,"serviceId":2,"startAt":200,"duration":100,"name":"Next"}
        ]"#)?;
        let mut projection = Projection::default();
        let initial = projection.update(&snapshot, 0, &channels, Some(100))?;
        assert_eq!(
            initial.as_deref(),
            Some(
                r#"[{"name":"First","startAt":100,"duration":100,"next":{"name":"Next","startAt":200,"duration":100}},null]"#
            )
        );
        assert!(
            projection
                .update(&snapshot, 0, &channels, Some(199))?
                .is_none()
        );
        assert!(
            projection
                .update(&snapshot, 0, &channels, Some(200))?
                .is_some()
        );
        assert!(
            projection
                .update(&snapshot, 1, &channels, Some(200))?
                .is_some()
        );
        assert!(
            projection
                .update(&snapshot, 1, &channels, Some(100))?
                .is_some()
        );
        assert_eq!(
            projection.update(&snapshot, 1, &channels, None)?.as_deref(),
            Some("[null,null]")
        );
        assert_eq!(
            projection.update(&snapshot, 1, &[], None)?.as_deref(),
            Some("[]")
        );
        Ok(())
    }
    #[test]
    fn next_program_follows_service_time_and_revision() -> Result<(), Box<dyn std::error::Error>> {
        let channels = crate::channels::parse(
            br#"[
            {"id":99,"networkId":1,"serviceId":2,"name":"A","type":1}
        ]"#,
        )?;
        let snapshot = super::super::model::parse(br#"[
            {"id":4,"networkId":1,"serviceId":2,"startAt":400,"duration":100,"name":"Later"},
            {"id":3,"networkId":1,"serviceId":2,"startAt":200,"duration":100,"name":"Next"},
            {"id":2,"networkId":1,"serviceId":2,"startAt":180,"duration":0,"name":"Empty"},
            {"id":9,"networkId":1,"serviceId":3,"startAt":150,"duration":100,"name":"Other channel"},
            {"id":1,"networkId":1,"serviceId":2,"startAt":100,"duration":100,"name":"Current"}
        ]"#)?;
        let mut projection = Projection::default();
        let initial: serde_json::Value = serde_json::from_str(
            &projection
                .update(&snapshot, 0, &channels, Some(100))?
                .unwrap(),
        )?;
        assert_eq!(initial[0]["next"]["name"], "Next");
        let boundary: serde_json::Value = serde_json::from_str(
            &projection
                .update(&snapshot, 0, &channels, Some(200))?
                .unwrap(),
        )?;
        assert_eq!(boundary[0]["name"], "Next");
        assert_eq!(boundary[0]["next"]["name"], "Later");
        let end: serde_json::Value = serde_json::from_str(
            &projection
                .update(&snapshot, 0, &channels, Some(400))?
                .unwrap(),
        )?;
        assert!(end[0]["next"].is_null());
        assert_eq!(
            projection.update(&snapshot, 0, &channels, None)?.as_deref(),
            Some("[null]")
        );
        let revised = super::super::model::parse(
            br#"[
            {"id":1,"networkId":1,"serviceId":2,"startAt":100,"duration":100,"name":"Current"},
            {"id":3,"networkId":1,"serviceId":2,"startAt":200,"duration":100,"name":"Updated next"}
        ]"#,
        )?;
        projection.update(&snapshot, 0, &channels, Some(100))?;
        let update: serde_json::Value = serde_json::from_str(
            &projection
                .update(&revised, 1, &channels, Some(100))?
                .unwrap(),
        )?;
        assert_eq!(update[0]["next"]["name"], "Updated next");
        Ok(())
    }
}
