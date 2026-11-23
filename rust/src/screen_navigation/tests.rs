use super::*;
use proptest::prelude::*;

const ALL: Availability = Availability {
    watching: true,
    guide: true,
};
const NO_PLAYBACK: Availability = Availability {
    watching: false,
    guide: true,
};
fn assert_screen(navigation: &Navigation, screen: Screen, back: Option<Screen>) {
    assert_screen_with(navigation, ALL, screen, back);
}
fn assert_screen_with(
    navigation: &Navigation,
    availability: Availability,
    screen: Screen,
    back: Option<Screen>,
) {
    assert_eq!(
        navigation.projection(availability),
        Projection { screen, back }
    );
}

#[test]
fn each_visit_moves_one_screen_to_the_top_and_home_clears_history() {
    let mut navigation = Navigation::default();
    assert_screen_with(&navigation, NO_PLAYBACK, Screen::Home, None);
    assert!(!navigation.back(NO_PLAYBACK));
    assert!(navigation.open(Screen::Recordings, NO_PLAYBACK));
    assert!(navigation.open(Screen::Guide, NO_PLAYBACK));
    assert!(navigation.back(NO_PLAYBACK));
    assert_screen_with(
        &navigation,
        NO_PLAYBACK,
        Screen::Recordings,
        Some(Screen::Home),
    );
    navigation.open(Screen::Guide, ALL);
    navigation.open(Screen::Watching, ALL);
    navigation.open(Screen::Recordings, ALL);
    navigation.open(Screen::Recordings, ALL);
    navigation.open(Screen::Home, ALL);
    assert_screen_with(&navigation, NO_PLAYBACK, Screen::Home, None);
}

#[test]
fn during_playback_back_alternates_between_watching_and_the_screen_left_from_it() {
    let mut navigation = Navigation::default();
    navigation.open(Screen::Recordings, ALL);
    navigation.open(Screen::Watching, ALL);
    assert_screen(&navigation, Screen::Watching, Some(Screen::Recordings));
    for _ in 0..2 {
        assert!(navigation.back(ALL));
        assert_screen(&navigation, Screen::Recordings, Some(Screen::Watching));
        assert!(navigation.back(ALL));
        assert_screen(&navigation, Screen::Watching, Some(Screen::Recordings));
    }
    navigation.open(Screen::Guide, ALL);
    assert_screen(&navigation, Screen::Guide, Some(Screen::Watching));
    assert!(navigation.back(ALL));
    assert_screen(&navigation, Screen::Watching, Some(Screen::Recordings));
    // Choosing new content from the Guide makes it the screen to go back to.
    navigation.open(Screen::Guide, ALL);
    navigation.open(Screen::Watching, ALL);
    assert_screen(&navigation, Screen::Watching, Some(Screen::Guide));
    assert!(navigation.back(ALL));
    assert_screen(&navigation, Screen::Guide, Some(Screen::Watching));
    assert!(navigation.back(ALL));
    assert_screen(&navigation, Screen::Watching, Some(Screen::Guide));
    navigation.open(Screen::Home, ALL);
    assert_screen(&navigation, Screen::Home, Some(Screen::Watching));
    assert!(navigation.back(ALL));
    assert_screen(&navigation, Screen::Watching, Some(Screen::Home));
    assert!(navigation.back(ALL));
    assert_screen(&navigation, Screen::Home, Some(Screen::Watching));
    // Home clears the history, so the screen visited next becomes the back target.
    navigation.open(Screen::Guide, ALL);
    assert!(navigation.back(ALL));
    assert_screen(&navigation, Screen::Watching, Some(Screen::Guide));
    assert!(navigation.back(ALL));
    assert_screen(&navigation, Screen::Guide, Some(Screen::Watching));
}

#[test]
fn without_playback_back_walks_down_the_history_again() {
    let mut navigation = Navigation::default();
    navigation.open(Screen::Recordings, ALL);
    navigation.open(Screen::Guide, ALL);
    navigation.open(Screen::Watching, ALL);
    navigation.back(ALL);
    assert_screen_with(
        &navigation,
        NO_PLAYBACK,
        Screen::Guide,
        Some(Screen::Recordings),
    );
    assert!(navigation.back(NO_PLAYBACK));
    assert_screen_with(
        &navigation,
        NO_PLAYBACK,
        Screen::Recordings,
        Some(Screen::Home),
    );
    assert!(navigation.back(NO_PLAYBACK));
    assert!(!navigation.back(NO_PLAYBACK));
}

#[test]
fn closed_sessions_and_disabled_epg_are_removed_from_every_position() {
    let mut navigation = Navigation::default();
    navigation.open(Screen::Guide, ALL);
    navigation.open(Screen::Watching, ALL);
    navigation.open(Screen::Recordings, ALL);
    let no_playback = NO_PLAYBACK;
    navigation.reconcile(no_playback);
    assert_screen_with(
        &navigation,
        no_playback,
        Screen::Recordings,
        Some(Screen::Guide),
    );
    assert!(!navigation.open(Screen::Watching, no_playback));
    navigation.back(no_playback);
    let nothing = Availability {
        watching: false,
        guide: false,
    };
    navigation.reconcile(nothing);
    assert_screen_with(&navigation, nothing, Screen::Home, None);
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
