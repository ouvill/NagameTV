import QtQuick
import QtTest
import ".." as Viewer

TestCase {
    id: testCase
    name: "ChannelBrowser"
    when: windowShown
    visible: true
    width: 800
    height: 340
    Component {
        id: component
        Viewer.ChannelBrowser {
            property alias rows: fixture.rows
            channels: ChannelFixture { id: fixture }
            iconDirectory: Qt.resolvedUrl("../../../assets/icons/")
            width: 780
            height: 304
            rows: [
                {
                    index: 0,
                    label: "GR",
                    band: "GR",
                    logo: ""
                },
                {
                    index: 1,
                    label: "BS 1",
                    band: "BS",
                    logo: ""
                },
                {
                    index: 2,
                    label: "BS 2",
                    band: "BS",
                    logo: ""
                }
            ]
            selected: 2
            programsJson: JSON.stringify([null,
                {
                    name: "BS First",
                    startAt: 100,
                    duration: 100
                },
                {
                    name: "BS Second",
                    startAt: 100,
                    duration: 100
                }
            ])
            now: 150
        }
    }
    SignalSpy {
        id: selection
        signalName: "selectRequested"
    }
    property var browser
    SignalSpy { id: closed; signalName: "closeRequested" }
    function initTestCase() {
        failOnWarning(/.*/);
    }
    function init() {
        failOnWarning(/.*/);
        browser = createTemporaryObject(component, testCase);
        verify(browser !== null);
        verify(waitForRendering(browser));
        selection.target = browser;
        selection.clear();
        closed.target = browser;
        closed.clear();
    }
    function test_close_button_supports_pointer_and_keyboard() {
        const button = findChild(browser, "browserCloseButton");
        verify(button !== null);
        mouseClick(button);
        compare(closed.count, 1);
        testCase.forceActiveFocus();
        button.forceActiveFocus(Qt.TabFocusReason);
        verify(button.visualFocus);
        keyClick(Qt.Key_Space);
        compare(closed.count, 2);
    }
    function test_filter_and_keyboard_preserve_backend_indices() {
        const list = findChild(browser, "browserList");
        compare(browser.band, "BS");
        compare(list.currentIndex, 1);
        list.forceLayout();
        verify(list.currentItem !== null);
        compare(findChild(list.currentItem, "cardProgramTitle").text, "BS Second");
        browser.focusBrowser();
        keyClick(Qt.Key_Left);
        compare(selection.count, 0);
        keyClick(Qt.Key_Return);
        compare(selection.signalArguments[0][0], 1);
        mouseClick(findChild(browser, "band-GR"));
        compare(selection.count, 1);
        compare(list.count, 1);
        browser.focusBrowser();
        keyClick(Qt.Key_Return);
        compare(selection.signalArguments[1][0], 0);
        browser.band = "CS";
        compare(list.count, 0);
        keyClick(Qt.Key_Return);
        compare(selection.count, 2);
    }
    function verifyCentered(list) {
        tryVerify(function() {
            const card = list.currentItem ? findChild(list.currentItem, "browserChannelCard") : null;
            return card !== null && Math.abs(card.width - list.candidateWidth) < 1
                && Math.abs(card.mapToItem(list, card.width / 2, 0).x - list.width / 2) < 2;
        }, 5000, "candidate index " + list.currentIndex);
    }
    function test_reopen_restores_playing_band_and_channel() {
        const list = findChild(browser, "browserList");
        browser.band = "GR";
        browser.openBrowser();
        compare(browser.band, "BS");
        compare(list.currentIndex, 1);
        verifyCentered(list);
        browser.selected = 0;
        browser.openBrowser();
        compare(browser.band, "GR");
        compare(list.currentIndex, 0);
        verifyCentered(list);
        compare(selection.count, 0);
    }
    function test_open_starts_on_playing_channel_far_into_catalog() {
        const rows = [];
        for (let i = 0; i < 100; ++i)
            rows.push({ index: i, label: "Channel " + i, band: i < 20 ? "GR" : "BS", logo: "" });
        const picker = createTemporaryObject(component, testCase, { rows: rows, selected: 87 });
        verify(picker !== null);
        verify(waitForRendering(picker));
        const list = findChild(picker, "browserList");
        compare(picker.band, "BS");
        tryCompare(list, "currentIndex", 67);
        verifyCentered(list);
        compare(list.currentItem.channelIndex, 87);
        picker.band = "GR";
        picker.openBrowser();
        tryCompare(list, "currentIndex", 67);
        verifyCentered(list);
        compare(list.currentItem.channelIndex, 87);
    }
    function test_open_terrestrial_and_visibility_update_keep_playing_channel() {
        const rows = [];
        for (let i = 0; i < 10; ++i)
            rows.push({ index: i, label: i === 0 ? "NHK総合" : "Channel " + i, band: "GR", logo: "" });
        const picker = createTemporaryObject(component, testCase, { rows: rows, selected: 7 });
        verify(picker !== null);
        verify(waitForRendering(picker));
        const list = findChild(picker, "browserList");
        tryCompare(list, "currentIndex", 7);
        picker.visibilityJson = "[0, 2, 4, 6, 7, 8]";
        wait(250);
        compare(list.currentItem.channelIndex, 7);
        verifyCentered(list);
    }
    function test_open_and_reopen_have_no_horizontal_transition() {
        const list = findChild(browser, "browserList");
        for (let frame = 0; frame < 16; ++frame) {
            wait(16);
            compare(list.motion, Viewer.ChannelBrowser.Idle);
            const card = findChild(list.currentItem, "browserChannelCard");
            compare(card.width, list.candidateWidth);
            verify(Math.abs(card.mapToItem(list, card.width / 2, 0).x - list.width / 2) < 2);
        }
        browser.focusBrowser();
        keyClick(Qt.Key_Left);
        wait(60);
        browser.openBrowser();
        for (let frame = 0; frame < 16; ++frame) {
            wait(16);
            compare(list.currentItem.channelIndex, browser.selected);
            compare(list.motion, Viewer.ChannelBrowser.Idle);
            const card = findChild(list.currentItem, "browserChannelCard");
            compare(card.width, list.candidateWidth);
            verify(Math.abs(card.mapToItem(list, card.width / 2, 0).x - list.width / 2) < 2);
        }
    }
    function test_width_animates_and_reverses_without_losing_center() {
        const list = findChild(browser, "browserList");
        verifyCentered(list);
        wait(200); // Finish opening before measuring a new keyboard transition.
        const previous = findChild(list.currentItem, "browserChannelCard");
        browser.focusBrowser();
        keyClick(Qt.Key_Left);
        const candidate = findChild(list.currentItem, "browserChannelCard");
        wait(60);
        verify(candidate.width > 270 && candidate.width < 356);
        verify(previous.width > 270 && previous.width < 356);
        const center = candidate.mapToItem(list, candidate.width / 2, 0).x;
        verify(center < list.width / 2 - 2, "scroll should still be moving toward the center: " + center);
        keyClick(Qt.Key_Right);
        verifyCentered(list);
        tryCompare(candidate, "width", 270);
        compare(findChild(list.currentItem, "browserChannelCard"), previous);
        compare(selection.count, 0);
    }
    function test_gap_stays_fourteen_during_expansion_and_reversal() {
        const list = findChild(browser, "browserList");
        verifyCentered(list);
        browser.focusBrowser();
        keyClick(Qt.Key_Left);
        for (let frame = 0; frame < 24; ++frame) {
            if (frame === 5) keyClick(Qt.Key_Right);
            wait(16);
            const left = findChild(list.itemAtIndex(0), "browserChannelCard");
            const right = findChild(list.itemAtIndex(1), "browserChannelCard");
            verify(left !== null && right !== null);
            const gap = right.mapToItem(list, 0, 0).x - left.mapToItem(list, left.width, 0).x;
            verify(Math.abs(gap - 14) < 1, "gap during animation: " + gap);
        }
        verifyCentered(list);
        compare(selection.count, 0);
    }
    function test_vertical_navigation_and_band_selection() {
        const list = findChild(browser, "browserList");
        browser.focusBrowser();
        keyClick(Qt.Key_Up);
        verify(findChild(browser, "band-BS").activeFocus);
        verify(findChild(browser, "band-BS").visualFocus);
        keyClick(Qt.Key_Left);
        compare(browser.band, "GR");
        verify(findChild(browser, "band-GR").activeFocus);
        keyClick(Qt.Key_Left);
        compare(browser.band, "GR");
        keyClick(Qt.Key_Down);
        verify(list.activeFocus);
        compare(list.currentItem.channelIndex, 0);
        keyClick(Qt.Key_Up);
        keyClick(Qt.Key_Right);
        compare(browser.band, "BS");
        verify(findChild(browser, "band-BS").activeFocus);
        keyClick(Qt.Key_Down);
        verify(list.activeFocus);
        keyClick(Qt.Key_Left);
        compare(list.currentItem.channelIndex, 1);
        keyClick(Qt.Key_Up);
        keyClick(Qt.Key_Down);
        compare(list.currentItem.channelIndex, 1);
        compare(selection.count, 0);
        keyClick(Qt.Key_Return);
        compare(selection.signalArguments[0][0], 1);
    }
    function test_keyboard_candidate_expands_and_centers_including_endpoints() {
        const rows = [];
        for (let i = 0; i < 12; ++i)
            rows.push({ index: i, label: "Channel " + i, band: "BS", logo: "" });
        browser.rows = rows;
        browser.openBrowser();
        const list = findChild(browser, "browserList");
        verifyCentered(list);
        keyClick(Qt.Key_Right);
        compare(list.currentIndex, 3);
        verifyCentered(list);
        const playingCard = list.itemAtIndex(2);
        if (playingCard)
            tryCompare(findChild(playingCard, "browserChannelCard"), "width", 270);
        compare(browser.selected, 2);
        compare(selection.count, 0);
        for (let i = 0; i < 20; ++i) keyClick(Qt.Key_Right);
        compare(list.currentIndex, 11);
        verifyCentered(list);
        for (let i = 0; i < 20; ++i) keyClick(Qt.Key_Left);
        compare(list.currentIndex, 0);
        verifyCentered(list);
        browser.width = 360;
        verifyCentered(list);
        keyClick(Qt.Key_Return);
        compare(selection.signalArguments[0][0], 0);
    }
    function test_playing_channel_remains_available_and_wheel_snaps_without_selecting() {
        browser.visibilityJson = "[1]";
        browser.openBrowser();
        const list = findChild(browser, "browserList");
        compare(list.count, 2);
        compare(list.currentItem.channelIndex, 2);
        verifyCentered(list);
        const start = list.contentX;
        mouseWheel(list, list.width / 2, 60, 0, 120);
        compare(list.currentIndex, 0);
        verify(list.contentX < start);
        verifyCentered(list);
        compare(selection.count, 0);
        browser.openBrowser();
        compare(list.currentItem.channelIndex, 2);
        verifyCentered(list);
    }
    function test_catalog_arriving_after_open_reveals_selected_band() {
        browser.rows = [];
        browser.selected = 7;
        browser.openBrowser();
        browser.rows = [{ index: 7, label: "CS", band: "CS", logo: "" }];
        compare(browser.band, "CS");
        const list = findChild(browser, "browserList");
        tryCompare(list, "count", 1);
        verifyCentered(list);
        compare(list.currentItem.channelIndex, 7);
    }
    function test_viewing_indicator_stays_with_playback_and_fits_after_channel_name() {
        const list = findChild(browser, "browserList");
        browser.viewingIndex = 2;
        verifyCentered(list);
        const viewedCard = findChild(list.currentItem, "browserChannelCard");
        const viewedDot = findChild(viewedCard, "browserWatchingIndicator");
        const name = findChild(viewedCard, "browserChannelName");
        verify(viewedDot.visible);
        verify(viewedDot.mapToItem(viewedCard, 0, 0).x
            > name.mapToItem(viewedCard, name.width, 0).x);
        browser.focusBrowser();
        keyClick(Qt.Key_Left);
        verifyCentered(list);
        verify(viewedDot.visible);
        const candidateDot = findChild(list.currentItem, "browserWatchingIndicator");
        verify(!candidateDot.visible);
        compare(selection.count, 0);
        browser.selected = 1;
        verify(viewedDot.visible, "Changing the requested channel cannot move the viewing indicator");
        browser.viewingIndex = -1;
        verify(!viewedDot.visible && !candidateDot.visible);
        browser.viewingIndex = 1;
        verify(candidateDot.visible);
        browser.width = 320;
        verifyCentered(list);
        const card = findChild(list.currentItem, "browserChannelCard");
        verify(candidateDot.mapToItem(card, candidateDot.width, 0).x < card.width - card.padding);
    }
    function test_reopen_reveals_viewed_band_even_if_request_is_elsewhere() {
        browser.selected = 0;
        browser.viewingIndex = 2;
        browser.visibilityJson = "[0,1]";
        browser.band = "GR";
        browser.openBrowser();
        const list = findChild(browser, "browserList");
        tryCompare(browser, "band", "BS");
        verifyCentered(list);
        compare(list.currentItem.channelIndex, 2);
        compare(selection.count, 0);
    }
    function test_large_catalog_is_virtualized_and_replacement_clears() {
        const rows = [];
        for (let i = 0; i < 500; ++i)
            rows.push({
                index: i,
                label: "Channel " + i,
                band: "GR",
                logo: ""
            });
        browser.band = "GR";
        browser.rows = rows;
        const list = findChild(browser, "browserList");
        tryCompare(list, "count", 500);
        list.forceLayout();
        verify(list.contentItem.children.length < 20);
        list.positionViewAtIndex(499, ListView.End);
        list.forceLayout();
        verify(list.contentItem.children.length < 20);
        browser.rows = [];
        tryCompare(list, "count", 0);
        compare(selection.count, 0);
    }
    function test_unselected_satellite_only_server_starts_with_available_band() {
        const satellite = createTemporaryObject(component, testCase, {
            rows: [
                {
                    index: 0,
                    label: "BS",
                    band: "BS",
                    logo: ""
                }
            ],
            selected: -1
        });
        verify(satellite !== null);
        compare(satellite.band, "BS");
        compare(findChild(satellite, "browserList").count, 1);
    }
}
