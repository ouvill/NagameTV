use super::*;
use proptest::prelude::*;

const ALL: Availability = Availability {
    watching: true,
    guide: true,
};
fn assert_screen(navigation: &Navigation, screen: Screen, back: Option<Screen>) {
    assert_eq!(navigation.projection(), Projection { screen, back });
}

#[test]
fn each_visit_moves_one_screen_to_the_top_and_home_clears_history() {
    let mut navigation = Navigation::default();
    assert_screen(&navigation, Screen::Home, None);
    assert!(!navigation.back(ALL));
    assert!(navigation.open(Screen::Recordings, ALL));
    assert!(navigation.open(Screen::Guide, ALL));
    assert!(navigation.back(ALL));
    assert_screen(&navigation, Screen::Recordings, Some(Screen::Home));
    navigation.open(Screen::Guide, ALL);
    navigation.open(Screen::Watching, ALL);
    navigation.open(Screen::Recordings, ALL);
    assert_screen(&navigation, Screen::Recordings, Some(Screen::Watching));
    navigation.open(Screen::Recordings, ALL);
    navigation.back(ALL);
    assert_screen(&navigation, Screen::Watching, Some(Screen::Guide));
    navigation.back(ALL);
    assert_screen(&navigation, Screen::Guide, Some(Screen::Home));
    navigation.open(Screen::Home, ALL);
    assert_screen(&navigation, Screen::Home, None);
}

#[test]
fn closed_sessions_and_disabled_epg_are_removed_from_every_position() {
    let mut navigation = Navigation::default();
    navigation.open(Screen::Guide, ALL);
    navigation.open(Screen::Watching, ALL);
    navigation.open(Screen::Recordings, ALL);
    let no_playback = Availability {
        watching: false,
        ..ALL
    };
    navigation.reconcile(no_playback);
    assert_screen(&navigation, Screen::Recordings, Some(Screen::Guide));
    assert!(!navigation.open(Screen::Watching, no_playback));
    navigation.back(no_playback);
    navigation.reconcile(Availability {
        watching: false,
        guide: false,
    });
    assert_screen(&navigation, Screen::Home, None);
    assert!(!navigation.open(
        Screen::Guide,
        Availability {
            guide: false,
            ..ALL
        }
    ));
    navigation.open(Screen::Guide, ALL);
    navigation.open(Screen::Watching, ALL);
    navigation.reconcile(Availability {
        guide: false,
        ..ALL
    });
    assert_screen(&navigation, Screen::Watching, Some(Screen::Home));
}

proptest! {
    #[test]
    fn arbitrary_visits_and_capability_changes_keep_a_bounded_valid_stack(
        operations in prop::collection::vec((0u8..6, any::<bool>(), any::<bool>()), 0..200)
    ) {
        let mut navigation = Navigation::default();
        for (operation, watching, guide) in operations {
            let available = Availability { watching, guide };
            navigation.reconcile(available);
            match operation {
                0..=3 => { navigation.open(Screen::from_number(i32::from(operation)).unwrap(), available); }
                4 => { navigation.back(available); }
                5 => navigation.reconcile(available),
                _ => unreachable!(),
            }
            let stack = &navigation.history;
            prop_assert!((1..=4).contains(&stack.len()));
            prop_assert_eq!(stack[0], Screen::Home);
            for (index, screen) in stack.iter().enumerate() {
                prop_assert!(available.allows(*screen));
                prop_assert!(!stack[..index].contains(screen));
            }
        }
    }
}
