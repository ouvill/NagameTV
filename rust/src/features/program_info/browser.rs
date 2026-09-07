//! Borrow current events from the shared EPG; retain only change-detection keys.
use super::model::{Program, Snapshot};
use crate::channels::{BroadcastService, Channel};
use serde::Serialize;

#[derive(PartialEq, Eq)]
struct Key {
    service: Option<BroadcastService>,
    event: Option<(u64, u64, u64)>,
    next: Option<(u64, u64, u64)>,
}
impl Key {
    fn new(channel: &Channel, program: Option<&Program>, next: Option<&Program>) -> Self {
        Self {
            service: channel.broadcast,
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
        let next = |channel: &Channel| now.and_then(|time| snapshot.next(channel.broadcast, time));
        if self.revision == Some(revision)
            && self.keys.len() == channels.len()
            && self
                .keys
                .iter()
                .zip(channels)
                .all(|(key, channel)| *key == Key::new(channel, current(channel), next(channel)))
        {
            return Ok(None);
        }
        let cards: Vec<_> = channels
            .iter()
            .map(|channel| {
                current(channel).map(|program| Card {
                    current: program.into(),
                    next: next(channel).map(Into::into),
                })
            })
            .collect();
        let json = serde_json::to_string(&cards)?;
        let visible = super::visibility::indices(channels, |channel| current(channel));
        let visible_json = serde_json::to_string(&visible)?;
        // Commit keys only after successful serialization so errors remain retryable.
        self.keys = channels
            .iter()
            .map(|channel| Key::new(channel, current(channel), next(channel)))
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
