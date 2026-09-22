use super::*;
use proptest::{collection, prelude::*};
use std::collections::HashSet;

const MAX_CHANNELS: usize = 32;
const MAX_OPERATIONS: usize = 64;

fn channels(ids: impl IntoIterator<Item = u64>) -> Vec<Channel> {
    ids.into_iter()
        .map(|id| Channel {
            id,
            name: format!("Station {id}"),
            label: format!("Station {id}"),
            band: crate::channels::Band::Other,
            has_logo_data: false,
            broadcast: None,
            physical: None,
            number: None,
            service_id: None,
        })
        .collect()
}

fn service_id() -> impl Strategy<Value = u64> {
    prop_oneof![Just(u64::MAX), 1_u64..u64::MAX]
}

proptest! {
    #[test]
    fn refresh_sequences_preserve_selection_and_published_snapshots(
        ids in collection::btree_set(service_id(), 1..=MAX_CHANNELS),
        selected_index in any::<usize>(),
        refreshes in collection::vec(
            (collection::vec(0..MAX_CHANNELS, 0..=MAX_CHANNELS), service_id()), 0..=MAX_OPERATIONS
        ),
    ) {
        let ids: Vec<_> = ids.into_iter().collect();
        let selected_id = ids[selected_index % ids.len()];
        let mut catalog = Catalog::default();
        catalog.replace(channels(ids.iter().copied()), Some(selected_id));
        let published = catalog.snapshot();

        for (indices, preferred) in refreshes {
            let mut seen = HashSet::new();
            let refreshed: Vec<_> = indices.into_iter()
                .map(|index| ids[index % ids.len()])
                .filter(|id| seen.insert(*id))
                .collect();
            catalog.replace(channels(refreshed.iter().copied()), Some(preferred));
            let expected = refreshed.contains(&selected_id).then_some(selected_id);
            prop_assert_eq!(catalog.selected().map(|channel| channel.id), expected);
            prop_assert_eq!(
                catalog.selected_index().map(|index| catalog.channels()[index].id),
                expected,
            );
            prop_assert!(published.iter().map(|channel| channel.id).eq(ids.iter().copied()));
            // An invalid UI cursor must not alter the remembered identity, even
            // while the station is absent from the refreshed catalog.
            prop_assert!(catalog.select(catalog.channels().len()).is_none());
        }

        catalog.replace(channels([selected_id]), None);
        prop_assert_eq!(catalog.selected().map(|channel| channel.id), Some(selected_id));
    }

    #[test]
    fn explicit_selection_survives_removal_and_only_reset_reapplies_the_default(
        ids in collection::btree_set(service_id(), 2..=MAX_CHANNELS),
        selections in collection::vec(any::<usize>(), 1..=MAX_OPERATIONS),
    ) {
        let ids: Vec<_> = ids.into_iter().collect();
        let preferred = *ids.last().unwrap();
        let mut catalog = Catalog::default();
        catalog.replace(Vec::new(), Some(preferred));
        catalog.replace(channels(ids.iter().copied()), Some(preferred));
        prop_assert_eq!(catalog.selected().unwrap().id, preferred);

        for index in selections {
            let index = index % ids.len();
            let chosen = ids[index];
            prop_assert_eq!(catalog.select(index).unwrap().id, chosen);
            catalog.replace(Vec::new(), Some(preferred));
            prop_assert!(catalog.selected().is_none());
            catalog.replace(channels(ids.iter().copied()), Some(preferred));
            prop_assert_eq!(catalog.selected().unwrap().id, chosen);
        }

        catalog = Catalog::default();
        catalog.replace(channels(ids.iter().copied()), Some(preferred));
        prop_assert_eq!(catalog.selected().unwrap().id, preferred);
    }
}
