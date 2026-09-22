import QtQuick
import QtTest
import "../../rust/qml" as Viewer
import "../../rust/qml/tests" as Fixtures

TestCase {
    id: testCase
    name: "ChannelWheel"
    when: windowShown
    visible: true
    width: 960
    height: 360
    readonly property int wheelDetent: 120
    readonly property int quarterDetent: wheelDetent / 4
    readonly property int wheelStepPixels: 144
    readonly property int initialCandidate: 12

    SignalSpy { id: selections; signalName: "selectRequested" }

    function init() {
        failOnWarning(/.*/);
        selections.clear();
    }

    function rows() {
        return Array.from({length: 24}, (_, index) =>
            ({index: index, band: "GR", label: "Channel " + index, logo: ""}));
    }

    function createBrowser() {
        const browser = createTemporaryObject(browserComponent, testCase,
            {rows: rows(), selected: initialCandidate});
        verify(browser !== null);
        selections.target = browser;
        verify(waitForRendering(browser));
        const list = findChild(browser, "browserList");
        tryCompare(list, "currentIndex", initialCandidate);
        wait(250); // Let the initial layout center the selected card.
        return browser;
    }

    Component {
        id: browserComponent
        Viewer.ChannelBrowser {
            property alias rows: fixture.rows
            channels: Fixtures.ChannelFixture { id: fixture }
            width: 960
            height: 304
            iconDirectory: Qt.resolvedUrl("../../assets/icons/")
        }
    }

    Component {
        id: sidebarComponent
        Viewer.SidebarChannels {
            property alias rows: fixture.rows
            channels: Fixtures.ChannelFixture { id: fixture }
            width: 320; height: 360
        }
    }

    function test_browser_scrolls_small_angle_events_data() {
        return [{tag: "vertical", horizontal: false}, {tag: "horizontal", horizontal: true}];
    }

    function test_browser_scrolls_small_angle_events(data) {
        const browser = createBrowser();
        const list = findChild(browser, "browserList");
        const start = list.contentX;
        for (let part = 1; part <= 4; ++part) {
            mouseWheel(list, list.width / 2, 60,
                data.horizontal ? -quarterDetent : 0, data.horizontal ? 0 : -quarterDetent);
            compare(list.currentIndex, initialCandidate + Math.round(wheelStepPixels * part / 4 / list.cardStride));
            fuzzyCompare(list.contentX, start + wheelStepPixels * part / 4, 0.01);
        }
        // Coalesced wheel input has the same effect as separate detents.
        mouseWheel(list, list.width / 2, 60, 0, -3 * wheelDetent);
        fuzzyCompare(list.contentX, start + 4 * wheelStepPixels, 0.01);
        compare(list.currentIndex, initialCandidate + Math.round(4 * wheelStepPixels / list.cardStride));
        compare(selections.count, 0);
    }

    function test_browser_scroll_interrupts_centering_and_keyboard_recovers() {
        const browser = createBrowser();
        const list = findChild(browser, "browserList");
        const input = findChild(browser, "browserScrollArea");
        keyClick(Qt.Key_Right);
        compare(list.currentIndex, initialCandidate + 1);
        wait(50); // Interrupt the keyboard's 180ms centering animation.
        const pixels = 7;
        const start = list.contentX;
        verify(input.scroll(Qt.point(0, -pixels), Qt.point(0, 0)));
        fuzzyCompare(list.contentX, start + pixels, 0.01);
        wait(250);
        // ListView may align the interrupted fractional animation position to a pixel.
        fuzzyCompare(list.contentX, start + pixels, 1);
        compare(selections.count, 0);

        keyClick(Qt.Key_Left);
        compare(list.currentIndex, initialCandidate);
        wait(250);
        fuzzyCompare(list.currentItem.mapToItem(list, list.currentItem.width / 2, 0).x,
            list.width / 2, 1);
        keyClick(Qt.Key_Return);
        compare(selections.count, 1);
        compare(selections.signalArguments[0][0], initialCandidate);

        verify(input.scroll(Qt.point(0, -pixels), Qt.point(0, 0)));
        browser.openBrowser();
        wait(250);
        compare(list.currentIndex, initialCandidate);
        fuzzyCompare(list.currentItem.mapToItem(list, list.currentItem.width / 2, 0).x,
            list.width / 2, 1);
    }

    function test_browser_pixel_distance_takes_priority_data() {
        return [
            {tag: "vertical", horizontal: false, width: 960},
            {tag: "horizontal-with-drift", horizontal: true, width: 960},
            {tag: "narrow-vertical", horizontal: false, width: 320},
            {tag: "narrow-horizontal-with-drift", horizontal: true, width: 320}
        ];
    }

    function test_browser_pixel_distance_takes_priority(data) {
        const browser = createBrowser();
        browser.width = data.width;
        wait(250);
        const list = findChild(browser, "browserList");
        const input = findChild(browser, "browserScrollArea");
        const start = list.contentX;
        const touchpadEvents = 10;
        const pixels = 7;
        // Qt Quick Test mouseWheel exposes angleDelta only. Feed pixel deltas
        // through the same handler separately; this does not emulate OS delivery.
        for (let part = 1; part <= touchpadEvents; ++part) {
            verify(input.scroll(data.horizontal ? Qt.point(-pixels, -1) : Qt.point(0, -pixels),
                Qt.point(0, -wheelDetent)));
            fuzzyCompare(list.contentX, start + pixels * part, 0.01);
            compare(list.currentIndex, initialCandidate);
        }
        // Reversal and switching input units both take effect immediately.
        verify(input.scroll(Qt.point(0, pixels), Qt.point(0, 0)));
        const reversed = start + pixels * (touchpadEvents - 1);
        fuzzyCompare(list.contentX, reversed, 0.01);
        mouseWheel(list, list.width / 2, 60, 0, quarterDetent);
        fuzzyCompare(list.contentX, reversed - wheelStepPixels / 4, 0.01);
        verify(!input.scroll(Qt.point(0, 0), Qt.point(0, 0)));
        tryCompare(list, "motion", Viewer.ChannelBrowser.Idle);
        fuzzyCompare(list.contentX, start, 0.01);
        compare(list.currentIndex, initialCandidate);
        compare(selections.count, 0);
    }

    function test_browser_small_scroll_from_centered_endpoints_data() {
        return [
            {tag: "first", selected: 0, pixels: -7},
            {tag: "last", selected: 23, pixels: 7}
        ];
    }

    function test_carousel_expands_during_input_and_new_input_interrupts_snap() {
        const browser = createBrowser();
        const list = findChild(browser, "browserList");
        const input = findChild(browser, "browserScrollArea");
        const start = list.contentX;
        const previous = findChild(list.currentItem, "browserChannelCard");
        const next = findChild(list.itemAtIndex(initialCandidate + 1), "browserChannelCard");
        verify(previous !== null && next !== null);
        const events = 5;
        const eventIntervalMs = 40;
        for (let part = 1; part <= events; ++part) {
            mouseWheel(list, list.width / 2, 60, 0, -quarterDetent);
            wait(eventIntervalMs);
            // The sequence outlasts the idle timeout, but each fresh event
            // keeps snapping suspended and movement proportional to input.
            fuzzyCompare(list.contentX, start + part * wheelStepPixels / 4, 0.01);
            compare(list.motion, Viewer.ChannelBrowser.Scrolling);
            verify(previous.width < list.candidateWidth);
            verify(next.width > list.baseCardWidth);
            const gap = next.mapToItem(list, 0, 0).x
                - previous.mapToItem(list, previous.width, 0).x;
            fuzzyCompare(gap, list.spacing, 0.01);
        }
        compare(list.currentIndex, initialCandidate + 1);
        verify(next.width > previous.width);
        tryCompare(list, "motion", Viewer.ChannelBrowser.Snapping);
        const beforeReversal = list.contentX;
        const reversePixels = 7;
        verify(input.scroll(Qt.point(0, reversePixels), Qt.point(0, 0)));
        compare(list.motion, Viewer.ChannelBrowser.Scrolling);
        fuzzyCompare(list.contentX, beforeReversal - reversePixels, 0.01);
        wait(list.snapDurationMs);
        fuzzyCompare(list.contentX, beforeReversal - reversePixels, 1);
        input.finishGesture();
        tryCompare(list, "motion", Viewer.ChannelBrowser.Idle);
        fuzzyCompare(next.mapToItem(list, next.width / 2, 0).x, list.width / 2, 1);
        fuzzyCompare(next.width, list.candidateWidth, 0.01);
        compare(selections.count, 0);
        keyClick(Qt.Key_Return);
        compare(selections.signalArguments[0][0], initialCandidate + 1);
        // The expanded part outside the logical slot must also accept clicks.
        mouseClick(next, 4, next.height / 2);
        compare(selections.signalArguments[1][0], initialCandidate + 1);
        browser.width = 360;
        browser.openBrowser();
        wait(250);
        compare(list.currentIndex, initialCandidate);
        const restored = findChild(list.currentItem, "browserChannelCard");
        fuzzyCompare(restored.mapToItem(list, restored.width / 2, 0).x, list.width / 2, 1);
    }

    function test_browser_small_scroll_from_centered_endpoints(data) {
        const browser = createBrowser();
        browser.selected = data.selected;
        wait(250);
        const list = findChild(browser, "browserList");
        const input = findChild(browser, "browserScrollArea");
        const start = list.contentX;
        verify(input.scroll(Qt.point(0, data.pixels), Qt.point(0, 0)));
        fuzzyCompare(list.contentX, start - data.pixels, 0.01);
    }

    function swipe(input, pixels, horizontal) {
        const samples = 7;
        const sampleIntervalMs = 16;
        for (let i = 0; i < samples; ++i) {
            input.scroll(horizontal ? Qt.point(-pixels, -1) : Qt.point(0, -pixels), Qt.point(0, 0));
            if (i < samples - 1) wait(sampleIntervalMs);
        }
    }

    function test_pixel_swipe_coasts_then_centers_data() {
        return [
            {tag: "vertical-forward", horizontal: false, pixels: 24, width: 960},
            {tag: "horizontal-backward", horizontal: true, pixels: -24, width: 960},
            {tag: "narrow-horizontal", horizontal: true, pixels: 24, width: 360}
        ];
    }

    function test_pixel_swipe_coasts_then_centers(data) {
        const browser = createBrowser();
        browser.width = data.width;
        wait(100);
        const list = findChild(browser, "browserList");
        const input = findChild(browser, "browserScrollArea");
        const start = list.contentX;
        swipe(input, data.pixels, data.horizontal);
        fuzzyCompare(list.contentX, start + 7 * data.pixels, 0.01);
        const released = list.contentX;
        input.finishGesture();
        verify(list.flicking);
        compare(list.motion, Viewer.ChannelBrowser.Scrolling);
        wait(90);
        verify((list.contentX - released) * Math.sign(data.pixels) > 12);
        compare(list.motion, Viewer.ChannelBrowser.Scrolling);
        compare(list.currentIndex, list.nearestIndex);
        tryCompare(list, "motion", Viewer.ChannelBrowser.Idle);
        fuzzyCompare(list.contentX, list.centeredPosition(list.currentIndex), 1);
        compare(selections.count, 0);
    }

    function test_new_gesture_and_keyboard_interrupt_inertia() {
        const browser = createBrowser();
        const list = findChild(browser, "browserList");
        const input = findChild(browser, "browserScrollArea");
        swipe(input, 24, true);
        input.finishGesture();
        verify(list.flicking);
        wait(60);
        // A new gesture starts even before its first nonzero delta arrives.
        input.beginGesture();
        verify(!list.flicking);
        const stopped = list.contentX;
        wait(100);
        fuzzyCompare(list.contentX, stopped, 1);
        input.scroll(Qt.point(7, 0), Qt.point(0, 0));
        fuzzyCompare(list.contentX, stopped - 7, 1);
        swipe(input, -24, true);
        input.finishGesture();
        verify(list.flicking);
        const candidate = list.currentIndex;
        keyClick(Qt.Key_Right);
        verify(!list.flicking);
        tryCompare(list, "motion", Viewer.ChannelBrowser.Idle);
        compare(list.currentIndex, candidate + 1);
        fuzzyCompare(list.contentX, list.centeredPosition(candidate + 1), 1);
        // A delayed end from the interrupted input cannot restart its inertia.
        input.finishGesture();
        wait(100);
        verify(!list.flicking);
        compare(list.currentIndex, candidate + 1);
        compare(selections.count, 0);
    }

    function test_slow_or_finished_native_momentum_does_not_coast_again_data() {
        return [{tag: "slow-adjustment", nativeTail: false}, {tag: "native-momentum-tail", nativeTail: true}];
    }

    function test_slow_or_finished_native_momentum_does_not_coast_again(data) {
        const browser = createBrowser();
        const list = findChild(browser, "browserList");
        const input = findChild(browser, "browserScrollArea");
        if (data.nativeTail) swipe(input, 24, true);
        // Approximate the final small deltas of platform-provided momentum.
        // This exercises application behavior, not OS scroll-phase delivery.
        swipe(input, 1, true);
        input.finishGesture();
        verify(!list.flicking);
        tryCompare(list, "motion", Viewer.ChannelBrowser.Idle);
        fuzzyCompare(list.contentX, list.centeredPosition(list.currentIndex), 1);
    }

    function test_inertia_stops_at_bounds_and_reopen_cancels_it() {
        const browser = createBrowser();
        browser.selected = rows().length - 2;
        wait(100);
        const list = findChild(browser, "browserList");
        const input = findChild(browser, "browserScrollArea");
        swipe(input, 24, true);
        input.finishGesture();
        verify(list.flicking);
        tryCompare(list, "motion", Viewer.ChannelBrowser.Idle);
        compare(list.currentIndex, rows().length - 1);
        fuzzyCompare(list.contentX, list.centeredPosition(list.currentIndex), 1);
        swipe(input, -24, true);
        input.finishGesture();
        verify(list.flicking);
        browser.openBrowser();
        verify(!list.flicking);
        tryCompare(list, "motion", Viewer.ChannelBrowser.Idle);
        compare(list.currentIndex, browser.selected);
        input.finishGesture();
        wait(100);
        fuzzyCompare(list.contentX, list.centeredPosition(browser.selected), 1);
        compare(selections.count, 0);
    }

    function test_pause_or_reversal_does_not_reuse_old_speed() {
        const browser = createBrowser();
        const list = findChild(browser, "browserList");
        const input = findChild(browser, "browserScrollArea");
        swipe(input, 24, true);
        wait(200); // Holding still before release must discard the old speed.
        input.finishGesture();
        verify(!list.flicking);
        tryCompare(list, "motion", Viewer.ChannelBrowser.Idle);
        swipe(input, 24, true);
        input.scroll(Qt.point(7, 0), Qt.point(0, 0));
        input.finishGesture();
        verify(!list.flicking); // A short reversal cannot fling forward.
        tryCompare(list, "motion", Viewer.ChannelBrowser.Idle);
        compare(selections.count, 0);
    }

    function test_keyboard_or_hide_before_release_cancels_pending_inertia() {
        const browser = createBrowser();
        const list = findChild(browser, "browserList");
        const input = findChild(browser, "browserScrollArea");
        swipe(input, 24, true);
        keyClick(Qt.Key_Left);
        const target = list.currentIndex;
        input.finishGesture();
        verify(!list.flicking);
        tryCompare(list, "motion", Viewer.ChannelBrowser.Idle);
        fuzzyCompare(list.contentX, list.centeredPosition(target), 1);
        swipe(input, 24, true);
        browser.visible = false;
        input.finishGesture();
        verify(!list.flicking);
        verify(!input.active);
        browser.visible = true;
        browser.openBrowser();
        tryCompare(list, "motion", Viewer.ChannelBrowser.Idle);
        compare(list.currentIndex, browser.selected);
        compare(selections.count, 0);
    }

    function test_sidebar_distance_and_click_delivery() {
        const sidebar = createTemporaryObject(sidebarComponent, testCase,
            {rows: rows(), selected: 0});
        verify(sidebar !== null);
        selections.target = sidebar;
        verify(waitForRendering(sidebar));
        const list = findChild(sidebar, "sidebarChannelList");
        const input = findChild(sidebar, "sidebarScrollArea");
        list.positionViewAtBeginning();
        const start = list.contentY;
        for (let part = 1; part <= 4; ++part) {
            mouseWheel(list, list.width / 2, 60, 0, -quarterDetent);
            fuzzyCompare(list.contentY, start + wheelStepPixels * part / 4, 0.01);
        }
        const touchpadPixels = 7;
        const touchpadEvents = 10;
        for (let event = 0; event < touchpadEvents; ++event)
            verify(input.scroll(Qt.point(0, -touchpadPixels), Qt.point(0, -wheelDetent)));
        fuzzyCompare(list.contentY, start + wheelStepPixels + touchpadPixels * touchpadEvents, 0.01);
        verify(!input.scroll(Qt.point(0, 0), Qt.point(0, 0)));
        compare(selections.count, 0);

        list.positionViewAtBeginning();
        verify(waitForRendering(list));
        mouseWheel(list, list.width / 2, 60, 0, wheelDetent);
        fuzzyCompare(list.contentY, list.originY, 0.01);
        mouseClick(list, list.width / 2, 60);
        compare(selections.count, 1);
        compare(selections.signalArguments[0][0], 0);

        list.positionViewAtEnd();
        verify(waitForRendering(list));
        mouseWheel(list, list.width / 2, 60, 0, -wheelDetent);
        fuzzyCompare(list.contentY, list.originY + list.contentHeight - list.height, 0.01);
    }

    function test_reaches_both_ends_data() {
        return [
            { tag: "first-selected", selected: 0 },
            { tag: "middle-selected", selected: 12 },
            { tag: "last-selected", selected: 23 }
        ];
    }

    function test_reaches_both_ends(data) {
        const rows = [];
        for (let i = 0; i < 24; ++i)
            rows.push({index: i, band: "GR", label: "Channel " + i, logo: ""});
        const browser = createTemporaryObject(browserComponent, testCase,
            { rows: rows, selected: data.selected });
        verify(browser !== null);
        const list = findChild(browser, "browserList");
        verify(list !== null);
        wait(100);
        for (let i = 0; i < 90; ++i) {
            mouseWheel(list, list.width / 2, list.height / 2, 0, -120);
            wait(5);
        }
        const last = list.itemAtIndex(rows.length - 1);
        verify(last !== null, "Last channel must be instantiated");
        verify(last.x + last.width <= list.contentX + list.width + 1,
            "Last card clipped: " + JSON.stringify({right: last.x + last.width,
                viewportRight: list.contentX + list.width, origin: list.originX}));
        for (let i = 0; i < 90; ++i) {
            mouseWheel(list, list.width / 2, list.height / 2, 0, 120);
            wait(5);
        }
        const first = list.itemAtIndex(0);
        verify(first !== null);
        verify(first.x >= list.contentX - 1, "First card must be fully visible");
    }
}
