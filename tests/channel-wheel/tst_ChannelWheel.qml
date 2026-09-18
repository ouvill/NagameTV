import QtQuick
import QtTest
import "../../rust/qml" as Viewer

TestCase {
    id: testCase
    name: "ChannelWheel"
    when: windowShown
    visible: true
    width: 960
    height: 360
    readonly property int wheelDetent: 120
    readonly property int quarterDetent: wheelDetent / 4
    readonly property int sidebarStepPixels: 144
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
            width: 960
            height: 304
            iconDirectory: Qt.resolvedUrl("../../assets/icons/")
        }
    }

    Component {
        id: sidebarComponent
        Viewer.SidebarChannels { width: 320; height: 360 }
    }

    function test_browser_accumulates_small_angle_events_data() {
        return [{tag: "vertical", horizontal: false}, {tag: "horizontal", horizontal: true}];
    }

    function test_browser_accumulates_small_angle_events(data) {
        const browser = createBrowser();
        const list = findChild(browser, "browserList");
        const start = list.contentX;
        for (let part = 1; part <= 4; ++part) {
            mouseWheel(list, list.width / 2, 60,
                data.horizontal ? -quarterDetent : 0, data.horizontal ? 0 : -quarterDetent);
            compare(list.currentIndex, initialCandidate + (part === 4 ? 1 : 0));
            if (part < 4)
                fuzzyCompare(list.contentX, start, 1);
        }
        // Coalesced wheel input has the same effect as separate detents.
        mouseWheel(list, list.width / 2, 60, 0, -3 * wheelDetent);
        compare(list.currentIndex, initialCandidate + 4);
        compare(selections.count, 0);
    }

    function test_browser_reversal_idle_and_reopen_clear_partial_steps() {
        const browser = createBrowser();
        const list = findChild(browser, "browserList");
        const input = findChild(browser, "browserScrollArea");
        mouseWheel(list, list.width / 2, 60, 0, -3 * quarterDetent);
        mouseWheel(list, list.width / 2, 60, 0, 3 * quarterDetent);
        compare(list.currentIndex, initialCandidate);
        mouseWheel(list, list.width / 2, 60, 0, quarterDetent);
        compare(list.currentIndex, initialCandidate - 1);

        browser.openBrowser();
        mouseWheel(list, list.width / 2, 60, 0, -3 * quarterDetent);
        wait(input.gestureIdleMs + 50);
        mouseWheel(list, list.width / 2, 60, 0, -quarterDetent);
        compare(list.currentIndex, initialCandidate);

        browser.openBrowser();
        mouseWheel(list, list.width / 2, 60, 0, -3 * quarterDetent);
        compare(list.currentIndex, initialCandidate);
        mouseWheel(list, list.width / 2, 60, 0, -quarterDetent);
        compare(list.currentIndex, initialCandidate + 1);
        compare(selections.count, 0);
    }

    function test_browser_pixel_distance_takes_priority_data() {
        return [{tag: "vertical", horizontal: false}, {tag: "horizontal-with-drift", horizontal: true}];
    }

    function test_browser_pixel_distance_takes_priority(data) {
        const browser = createBrowser();
        const list = findChild(browser, "browserList");
        const input = findChild(browser, "browserScrollArea");
        const partsPerCard = 10;
        const pixels = (list.candidateWidth + list.spacing) / partsPerCard;
        // Qt Quick Test mouseWheel exposes angleDelta only. Feed pixel deltas
        // through the same handler separately; this does not emulate OS delivery.
        for (let part = 1; part <= partsPerCard; ++part) {
            verify(input.scroll(data.horizontal ? Qt.point(-pixels, -1) : Qt.point(0, -pixels),
                Qt.point(0, -wheelDetent)));
            compare(list.currentIndex, initialCandidate + (part === partsPerCard ? 1 : 0));
        }
        // Pixel and angle-only streams must not share a partial step.
        verify(input.scroll(Qt.point(0, -pixels), Qt.point(0, 0)));
        mouseWheel(list, list.width / 2, 60, 0, -3 * quarterDetent);
        compare(list.currentIndex, initialCandidate + 1);
        mouseWheel(list, list.width / 2, 60, 0, -quarterDetent);
        compare(list.currentIndex, initialCandidate + 2);
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
            fuzzyCompare(list.contentY, start + sidebarStepPixels * part / 4, 0.01);
        }
        const touchpadPixels = 7;
        const touchpadEvents = 10;
        for (let event = 0; event < touchpadEvents; ++event)
            verify(input.scroll(Qt.point(0, -touchpadPixels), Qt.point(0, -wheelDetent)));
        fuzzyCompare(list.contentY, start + sidebarStepPixels + touchpadPixels * touchpadEvents, 0.01);
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
