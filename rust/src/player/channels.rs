//! One atomic presentation snapshot, produced only when the service list changes.
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

#[cfg(test)]
mod tests {
    use super::*;

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
