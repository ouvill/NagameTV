//! Authoritative channel identity and selection, independent of Qt and view lifetime.
use super::Channel;
use std::sync::Arc;

#[cfg(test)]
mod properties;

#[derive(Clone, Default)]
enum Selection {
    #[default]
    Initial,
    Service(u64),
}

#[derive(Clone, Default)]
pub struct Catalog {
    channels: Arc<[Channel]>,
    selection: Selection,
}

impl Catalog {
    pub fn channels(&self) -> &[Channel] {
        &self.channels
    }

    pub fn snapshot(&self) -> Arc<[Channel]> {
        Arc::clone(&self.channels)
    }

    pub fn selected_index(&self) -> Option<usize> {
        match self.selection {
            Selection::Service(id) => self.channels.iter().position(|channel| channel.id == id),
            Selection::Initial => None,
        }
    }

    pub fn selected(&self) -> Option<&Channel> {
        self.selected_index().map(|index| &self.channels[index])
    }

    /// A missing service retains its identity so a later refresh can restore it.
    /// Only the first nonempty catalog may choose a default station.
    pub fn replace(&mut self, channels: Vec<Channel>, preferred: Option<u64>) {
        if matches!(self.selection, Selection::Initial)
            && let Some(channel) = channels
                .iter()
                .find(|channel| Some(channel.id) == preferred)
                .or_else(|| channels.first())
        {
            self.selection = Selection::Service(channel.id);
        }
        self.channels = channels.into();
    }

    /// Invalid indices cannot change the selected station.
    pub fn select(&mut self, index: usize) -> Option<&Channel> {
        let channel = self.channels.get(index)?;
        self.selection = Selection::Service(channel.id);
        Some(channel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channels() -> Vec<Channel> {
        super::super::parse(
            br#"[
            {"id":10,"name":"A","type":1,"channel":{"type":"GR"}},
            {"id":18446744073709551615,"name":"B","type":1,"channel":{"type":"BS"}}
        ]"#,
        )
        .unwrap()
    }

    #[test]
    fn identity_survives_reordering_absence_and_empty_refreshes() {
        let mut catalog = Catalog::default();
        catalog.replace(vec![], Some(u64::MAX));
        catalog.replace(channels(), Some(u64::MAX));
        assert_eq!(catalog.selected_index(), Some(1));
        let old = catalog.snapshot();
        let mut reversed = channels();
        reversed.reverse();
        catalog.replace(reversed, Some(10));
        assert_eq!(catalog.selected_index(), Some(0));
        assert_eq!(old[1].id, u64::MAX);
        catalog.replace(channels().into_iter().take(1).collect(), Some(10));
        assert!(catalog.selected().is_none());
        catalog.replace(vec![], Some(10));
        catalog.replace(channels(), Some(10));
        assert_eq!(catalog.selected().unwrap().id, u64::MAX);
    }

    #[test]
    fn invalid_selection_is_rejected_and_new_connection_may_choose_default() {
        let mut catalog = Catalog::default();
        catalog.replace(channels(), Some(999));
        assert_eq!(catalog.selected().unwrap().id, 10);
        assert!(catalog.select(99).is_none());
        assert_eq!(catalog.selected().unwrap().id, 10);
        assert_eq!(catalog.select(1).unwrap().id, u64::MAX);
        catalog = Catalog::default();
        catalog.replace(channels(), Some(10));
        assert_eq!(catalog.selected().unwrap().id, 10);
    }
}
