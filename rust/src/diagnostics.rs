//! Application lifetime adapter for bounded diagnostics.
use std::{path::PathBuf, sync::OnceLock};
use viewer_diagnostics::recorder::{GcCategory, GcSink, Recorder};
static GC_SINK: OnceLock<GcSink> = OnceLock::new();

pub fn start(directory: PathBuf, isolated: bool) -> Result<Option<Recorder>, String> {
    let setting = std::env::var("MIRAKURUN_DIAGNOSTICS");
    if !enabled(setting.as_deref().ok(), isolated) {
        return Ok(None);
    }
    let recorder = Recorder::start_directory(directory.join("usage"), env!("CARGO_PKG_VERSION"))
        .map_err(|error| error.to_string())?;
    // One application Player owns the recorder. This process-wide callback keeps only Weak.
    GC_SINK
        .set(recorder.gc_sink())
        .map_err(|_| "診断機能は既に初期化されています".to_owned())?;
    Ok(Some(recorder))
}
pub fn record_qt_gc(category: &str, message: &str) {
    if let (Some(sink), Some(category)) = (GC_SINK.get(), GcCategory::parse(category)) {
        let _ = sink.record(category, message);
    }
}

fn enabled(setting: Option<&str>, isolated: bool) -> bool {
    setting != Some("0") && (!isolated || setting == Some("1"))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_disabled_and_isolated_launches_do_not_enable_recording_by_default() {
        assert!(enabled(None, false));
        assert!(!enabled(None, true));
        for isolated in [true, false] {
            assert!(!enabled(Some("0"), isolated));
            assert!(enabled(Some("1"), isolated));
        }
        assert!(!enabled(Some("invalid"), true));
    }
}
