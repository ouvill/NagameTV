import QtQuick
import QtTest
import ".."
TestCase {
    id: testCase
    name: "SidebarChannels"
    when: windowShown
    visible: true
    width: 400
    height: 480
    SidebarChannels { id: view; anchors.fill: parent; rows: []; selected: -1 }
    SignalSpy { id: selections; target: view; signalName: "selectRequested" }
    function test_filter_keyboard_and_virtualized_catalog() {
        failOnWarning(/.*/);
        view.rows = Array.from({length: 500}, (_, i) => ({index: i, label: "局 " + i, band: i % 2 ? "BS" : "GR", logo: ""}));
        view.selected = 4;
        const list = findChild(view, "sidebarChannelList");
        verify(waitForRendering(view));
        compare(list.count, 250);
        compare(list.currentIndex, 2);
        list.forceActiveFocus();
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Return);
        compare(selections.count, 1);
        compare(selections.signalArguments[0][0], 6);
        view.band = "BS";
        compare(selections.count, 1);
        compare(list.count, 250);
        list.forceActiveFocus();
        keyClick(Qt.Key_Return);
        compare(selections.signalArguments[1][0], 1);
        list.positionViewAtEnd();
        verify(waitForRendering(view));
        const cards = list.contentItem.children.filter(item => item.objectName === "sidebarChannelCard");
        verify(cards.length > 0 && cards.length < 20, "virtualized delegates: " + cards.length);
        view.visibilityJson = "[1,5,7]";
        compare(list.count, 3);
        compare(view.filtered[1].index, 5);
        view.visibilityJson = "[]";
        compare(list.count, 250);
        view.rows = [];
        compare(list.count, 0);
        compare(list.currentIndex, -1);
    }
}
