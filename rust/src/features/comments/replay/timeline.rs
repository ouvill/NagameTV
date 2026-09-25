//! Immutable playback windows. A reset has its own identity even when empty.
use std::sync::Arc;
use viewer_comments::danmaku::TimedComment;

#[derive(Clone)]
pub(crate) struct Timeline {
    generation: Arc<()>,
    records: Arc<[TimedComment]>,
}
impl Default for Timeline {
    fn default() -> Self {
        Self {
            generation: Arc::new(()),
            records: Arc::default(),
        }
    }
}
impl Timeline {
    #[cfg(feature = "native_tests")]
    pub(crate) fn fixture(records: Vec<TimedComment>) -> Self {
        let mut timeline = Self::default();
        timeline.replace(records);
        timeline
    }
    pub fn records(&self) -> &[TimedComment] {
        &self.records
    }
    pub fn same_generation(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.generation, &other.generation)
    }
    pub fn same_snapshot(&self, other: &Self) -> bool {
        self.same_generation(other) && Arc::ptr_eq(&self.records, &other.records)
    }
    pub(super) fn replace(&mut self, records: Vec<TimedComment>) {
        if self.records.as_ref() != records {
            self.records = records.into();
        }
    }
    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unchanged_windows_reuse_storage_but_empty_resets_have_a_new_identity() {
        let mut timeline = Timeline::default();
        let empty = timeline.clone();
        timeline.replace(Vec::new());
        assert!(timeline.same_snapshot(&empty));
        let records =
            viewer_comments::danmaku::parse_timeline(r#"[{"id":"1","time":1,"text":"コメント"}]"#)
                .unwrap();
        timeline.replace(records.clone());
        assert!(timeline.same_generation(&empty));
        assert!(!timeline.same_snapshot(&empty));
        let populated = timeline.clone();
        timeline.replace(records);
        assert!(timeline.same_snapshot(&populated));
        timeline.reset();
        assert!(!timeline.same_generation(&populated));
        assert!(!timeline.same_snapshot(&empty));
        let reset = timeline.clone();
        timeline.reset();
        assert!(!timeline.same_generation(&reset));
    }
}
