//! Build-time constants only: never inspect Git or the environment at runtime.
use std::sync::OnceLock;

include!(concat!(env!("OUT_DIR"), "/build_info.rs"));

pub fn json() -> &'static str {
    static JSON: OnceLock<String> = OnceLock::new();
    JSON.get_or_init(|| {
        serde_json::to_string_pretty(&INFO).expect("BuildInfo contains only JSON values")
    })
}
