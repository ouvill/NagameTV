//! Convert legacy presentation fixtures to real Mirakurun inputs for UI tests.
use super::*;
use std::collections::HashMap;
fn load(json: &str) -> Result<(View, HashMap<String, String>), Box<dyn std::error::Error>> {
    use std::hash::{Hash, Hasher};
    let columns: Vec<serde_json::Value> = serde_json::from_str(json)?;
    let mut raw = Vec::new();
    let mut channel_values = Vec::new();
    let mut aliases = Vec::new();
    let mut low = u64::MAX;
    let mut high = 0;
    let max_channel = columns.iter().filter_map(|c| c["index"].as_u64()).max();
    for i in 0..max_channel.map_or(0, |i| i + 1) {
        channel_values.push(serde_json::json!({"id":i+1,"networkId":1,"serviceId":i+1,"name":format!("Channel {i}"),"type":1}));
    }
    for column in columns {
        let channel = column["index"].as_u64().ok_or("fixture index")?;
        for input in column["programs"].as_array().ok_or("fixture programs")? {
            let mut p = input.clone();
            let alias = p["watchKey"].as_str().unwrap_or("").to_owned();
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            alias.hash(&mut hasher);
            p["id"] = hasher.finish().into();
            p["networkId"] = 1.into();
            p["serviceId"] = (channel + 1).into();
            if let Some(genre) = p["genre"].as_u64() {
                p["genres"] = serde_json::json!([{"lv1":genre}]);
            }
            let extended = p
                .as_object_mut()
                .ok_or("fixture program object")?
                .remove("extended");
            let start = p["startAt"].as_u64().ok_or("fixture start")?;
            low = low.min(start);
            high = high.max(
                start.saturating_add(p["duration"].as_u64().ok_or("fixture duration")?.max(2)),
            );
            aliases.push((channel as usize, start, alias));
            let mut wire = serde_json::to_string(&p)?;
            if let Some(sections) = extended.and_then(|v| v.as_array().cloned()) {
                let fields = sections
                    .iter()
                    .map(|section| -> Result<String, serde_json::Error> {
                        Ok(format!(
                            "{}:{}",
                            serde_json::to_string(section["heading"].as_str().unwrap_or(""))?,
                            serde_json::to_string(&section["text"])?
                        ))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                wire.pop();
                wire.push_str(&format!(",\"extended\":{{{}}}}}", fields.join(",")));
            }
            raw.push(wire);
        }
    }
    if raw.is_empty() {
        return Ok((View::default(), Default::default()));
    }
    let snapshot =
        crate::features::program_info::model::parse(format!("[{}]", raw.join(",")).as_bytes())?;
    let channels = crate::channels::parse(&serde_json::to_vec(&channel_values)?)?;
    let window = crate::features::program_info::guide::DayWindow::new(
        low as f64,
        high.min(low.saturating_add(26 * 3600000)) as f64,
    )?;
    let view = View::new(snapshot, channels.into(), window, 0)?;
    let keys = aliases
        .into_iter()
        .filter_map(|(channel, start, alias)| {
            view.nearest(channel, start)
                .map(|row| (alias, view.cells()[row].key.clone()))
        })
        .collect();
    Ok((view, keys))
}
impl ffi::GuideModel {
    /// Test-only adapter for historical QML fixtures. Production always uses the
    /// validated domain View, including these fixtures' overlap computation.
    pub fn load_test(mut self: Pin<&mut Self>, json: QString) -> bool {
        let result = load(&json.to_string());
        match result {
            Ok((view, keys)) => {
                self.as_mut().rust_mut().test_keys = keys;
                self.replace(view);
                true
            }
            Err(error) => {
                tracing::error!(error = &*error, "Guide fixture loading failed");
                false
            }
        }
    }
    pub fn test_key(&self, alias: QString) -> QString {
        QString::from(
            self.rust()
                .test_keys
                .get(&alias.to_string())
                .map_or("", String::as_str),
        )
    }
}
