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
