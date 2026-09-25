import QtQuick
import QtTest
import MinimalViewer

TestCase {
    name: "BroadcastTabs"
    width: 450
    height: 100
    visible: true
    when: windowShown
    property string previousLanguage
    property var tabs
    Component {
        id: component
        BroadcastTabs {
            property alias rows: fixture.rows
            channels: ChannelFixture { id: fixture }
            x: 10; y: 10
            rows: [{band: "GR"}, {band: "BS"}, {band: "CS"}]
            value: "GR"
        }
    }
    SignalSpy { id: selected; signalName: "selected" }
    function init() {
        failOnWarning(/.*/);
        previousLanguage = Qt.uiLanguage;
        tabs = createTemporaryObject(component, this);
        verify(tabs !== null);
        selected.target = tabs;
        selected.clear();
    }
    function cleanup() { Qt.uiLanguage = previousLanguage; }
    function test_language_change_resizes_hit_targets() {
        Qt.uiLanguage = "ja";
        const japaneseWidth = tabs.width;
        Qt.uiLanguage = "en";
        verify(waitForRendering(tabs));
        verify(tabs.width > japaneseWidth, "English labels need wider hit targets");
        for (const band of ["GR", "BS", "CS"]) {
            const tab = findChild(tabs, "band-" + band);
            verify(!tab.contentItem.truncated, "Tab label should fit: " + band);
        }
        const last = findChild(tabs, "band-CS");
        mouseClick(last, last.width - 4, last.height / 2);
        compare(selected.count, 1);
        compare(selected.signalArguments[0][0], "CS");
        Qt.uiLanguage = "ja";
        compare(tabs.width, japaneseWidth);
    }
    function test_constrained_width_keeps_all_tabs_inside_background() {
        tabs.width = 216;
        verify(waitForRendering(tabs));
        let previousRight = 0;
        for (const band of ["GR", "BS", "CS"]) {
            const tab = findChild(tabs, "band-" + band);
            const left = tab.mapToItem(tabs, 0, 0).x;
            const right = tab.mapToItem(tabs, tab.width, 0).x;
            verify(tab.width > 0 && left >= previousRight && right <= tabs.width,
                   "Tab must be inside the background without overlap: " + band);
            previousRight = right;
            selected.clear();
            mouseClick(tab, tab.width - 4, tab.height / 2);
            compare(selected.count, 1);
            compare(selected.signalArguments[0][0], band);
        }
    }
}
