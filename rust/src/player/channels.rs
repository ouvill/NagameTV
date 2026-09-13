//! Channel navigation and the atomic service-list projection.
use crate::channels::{Band, Channel};
use serde::Serialize;

#[derive(Serialize)]
struct Row<'a> {
    index: usize,
    label: &'a str,
    band: Band,
    logo: String,
}

pub fn presentation(channels: &[Channel], server: &str) -> Result<String, serde_json::Error> {
    // The small projection borrows labels. QML receives row indices, not u64 service
    // IDs (JavaScript numbers cannot represent every u64). Endpoint IDs are only
    // embedded in URL strings, preserving their exact decimal representation.
    let rows: Vec<_> = channels
        .iter()
        .enumerate()
        .map(|(index, channel)| Row {
            index,
            label: &channel.label,
            band: channel.band,
            logo: if channel.has_logo_data {
                format!(
                    "{}/api/services/{}/logo",
                    server.trim_end_matches('/'),
                    channel.id
                )
            } else {
                String::new()
            },
        })
        .collect();
    serde_json::to_string(&rows)
}

/// An empty snapshot does not mean this is a new connection.
#[derive(Default, Clone, Copy)]
pub(super) enum SelectionPolicy {
    #[default]
    Initial,
    Preserve,
}

/// First connection may choose a default. Refresh must not select a different broadcast
/// merely because the old index moved or its service disappeared from the catalog.
pub(super) fn selected_after_update(
    policy: SelectionPolicy,
    previous: &[Channel],
    selected: i32,
    next: &[Channel],
    preferences: &crate::settings::Preferences,
) -> Option<usize> {
    if matches!(policy, SelectionPolicy::Initial) {
        return preferences.selected_index(next.iter().map(|channel| channel.id));
    }
    let id = usize::try_from(selected)
        .ok()
        .and_then(|index| previous.get(index))
        .map(|channel| channel.id)
        .or_else(|| preferences.service_id.parse::<u64>().ok())?;
    next.iter().position(|channel| channel.id == id)
}

impl super::ffi::Player {
    pub(super) fn refresh_channels_if_due(self: std::pin::Pin<&mut Self>) {
        use cxx_qt::CxxQtType;
        let now = std::time::Instant::now();
        if !self
            .rust()
            .channel_refresh
            .due(now, self.rust().request.is_busy())
        {
            return;
        }
        self.request_channel_refresh(now);
    }

    pub fn refresh_channels(self: std::pin::Pin<&mut Self>, force: bool) {
        use cxx_qt::CxxQtType;
        let now = std::time::Instant::now();
        if self
            .rust()
            .channel_refresh
            .requested_by_user(now, self.rust().request.is_busy(), force)
        {
            self.request_channel_refresh(now);
        }
    }

    fn request_channel_refresh(mut self: std::pin::Pin<&mut Self>, now: std::time::Instant) {
        use cxx_qt::CxxQtType;
        if self.rust().network.is_none() {
            return;
        }
        let Ok(server) = crate::services::ServerUrl::parse(&self.rust().server.to_string()) else {
            return;
        };
        self.as_mut().rust_mut().request.request(server);
        self.as_mut().rust_mut().channel_refresh.requested(now);
    }

    pub fn step_channel(self: std::pin::Pin<&mut Self>, offset: i32) {
        use crate::channels::Step;
        use cxx_qt::CxxQtType;
        use std::time::{SystemTime, UNIX_EPOCH};
        let step = match offset {
            -1 => Step::Previous,
            1 => Step::Next,
            _ => return,
        };
        let this = self.rust();
        let now = if this.epg_enabled {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .ok()
                .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        } else {
            None
        };
        let selected = usize::try_from(this.selected).ok();
        let target = this
            .epg
            .adjacent_channel(&this.entries, selected, step, now)
            .and_then(|index| i32::try_from(index).ok());
        if let Some(index) = target {
            self.select(index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_preserves_identity_and_missing_service_never_selects_another()
    -> Result<(), Box<dyn std::error::Error>> {
        const INPUT: &[u8] = br#"[
            {"id":10,"name":"A","type":1,"channel":{"type":"GR"}},
            {"id":20,"name":"B","type":1,"channel":{"type":"GR"}}
        ]"#;
        let rows = crate::channels::parse(INPUT)?;
        let mut reversed = crate::channels::parse(INPUT)?;
        reversed.reverse();
        let preferences = crate::settings::Preferences {
            service_id: "10".into(),
            ..Default::default()
        };
        assert_eq!(
            selected_after_update(SelectionPolicy::Preserve, &rows, 0, &reversed, &preferences),
            Some(1)
        );
        assert_eq!(
            selected_after_update(
                SelectionPolicy::Preserve,
                &rows,
                0,
                &reversed[..1],
                &preferences
            ),
            None
        );
        assert_eq!(
            selected_after_update(
                SelectionPolicy::Preserve,
                &reversed[..1],
                -1,
                &rows,
                &preferences
            ),
            Some(0)
        );
        assert_eq!(
            selected_after_update(
                SelectionPolicy::Initial,
                &[],
                -1,
                &reversed[..1],
                &preferences
            ),
            Some(0)
        );
        assert_eq!(
            selected_after_update(
                SelectionPolicy::Preserve,
                &[],
                -1,
                &reversed[..1],
                &preferences
            ),
            None
        );
        assert_eq!(
            selected_after_update(SelectionPolicy::Preserve, &[], -1, &rows, &preferences),
            Some(0)
        );
        assert_eq!(
            selected_after_update(SelectionPolicy::Preserve, &rows, 0, &[], &preferences),
            None
        );
        Ok(())
    }

    #[test]
    fn selection_indices_match_sorted_channels_and_ids_stay_in_rust()
    -> Result<(), Box<dyn std::error::Error>> {
        let channels = crate::channels::parse(br#"[
            {"id":18446744073709551615,"serviceId":200,"name":"BS","type":1,"hasLogoData":true,"channel":{"type":"BS"}},
            {"id":10,"serviceId":99,"name":"GR","type":1,"remoteControlKeyId":2,"channel":{"type":"GR"}}
        ]"#)?;
        let rows: serde_json::Value =
            serde_json::from_str(&presentation(&channels, "http://localhost:40772/")?)?;
        assert_eq!(
            rows,
            serde_json::json!([
                {"index":0,"label":"02   GR","band":"GR","logo":""},
                {"index":1,"label":"200   BS","band":"BS","logo":"http://localhost:40772/api/services/18446744073709551615/logo"}
            ])
        );
        assert_eq!(channels[1].id, u64::MAX);
        let preferences = crate::settings::Preferences {
            service_id: u64::MAX.to_string(),
            ..Default::default()
        };
        assert_eq!(
            preferences.selected_index(channels.iter().map(|c| c.id)),
            Some(1)
        );
        assert_eq!(presentation(&[], "http://localhost:40772")?, "[]");
        Ok(())
    }
}
