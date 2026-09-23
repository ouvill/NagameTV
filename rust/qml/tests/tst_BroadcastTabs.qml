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
        compare(tabs.width, 234);
        compare(findChild(tabs, "band-CS").width, 76);
        Qt.uiLanguage = "en";
        compare(tabs.width, 294);
        const last = findChild(tabs, "band-CS");
        compare(last.width, 96);
        mouseClick(last, last.width - 4, last.height / 2);
        compare(selected.count, 1);
        compare(selected.signalArguments[0][0], "CS");
        Qt.uiLanguage = "ja";
        compare(tabs.width, 234);
    }
    function test_constrained_width_keeps_all_tabs_inside_background() {
        tabs.width = 216;
        verify(waitForRendering(tabs));
        const first = findChild(tabs, "band-GR");
        const last = findChild(tabs, "band-CS");
        compare(first.width, 70);
        compare(last.mapToItem(tabs, last.width, 0).x, tabs.width - 3);
        mouseClick(last, last.width - 4, last.height / 2);
        compare(selected.signalArguments[0][0], "CS");
    }
}
