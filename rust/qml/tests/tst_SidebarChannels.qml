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
    function init() {
        failOnWarning(/.*/);
        view.rows = [];
        view.selected = -1;
        view.viewingIndex = -1;
        view.visibilityJson = "[]";
        view.programsJson = "[]";
        selections.clear();
    }
    function test_reopen_restores_viewed_band_and_offscreen_channel() {
        view.rows = Array.from({length: 50}, (_, index) =>
            ({index:index, label:"長いチャンネル名の表示 " + index, band:index % 2 ? "BS" : "GR", logo:""}));
        view.selected = 0;
        view.viewingIndex = 47;
        const list = findChild(view, "sidebarChannelList");
        view.band = "GR";
        view.openBrowser();
        tryCompare(view, "band", "BS");
        tryCompare(list, "currentIndex", 23);
        tryVerify(function() {
            const card = list.currentItem;
            return card && card.y >= list.contentY && card.y + card.height <= list.contentY + list.height;
        });
        const dot = findChild(list.currentItem, "sidebarWatchingIndicator");
        verify(dot.visible);
        verify(dot.mapToItem(list.currentItem, dot.width, 0).x <= list.currentItem.width - list.currentItem.padding);
        list.forceActiveFocus();
        keyClick(Qt.Key_Up);
        verify(dot.visible);
        verify(!findChild(list.currentItem, "sidebarWatchingIndicator").visible);
        view.visibilityJson = "[1,3]";
        view.openBrowser();
        tryCompare(list, "currentIndex", 2);
        compare(list.currentItem.modelData.index, 47);
        compare(selections.count, 0);
        view.viewingIndex = -1;
        tryCompare(view, "band", "GR");
        tryCompare(list, "currentIndex", 0);
        verify(!findChild(list.currentItem, "sidebarWatchingIndicator").visible);
    }
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
    function test_next_program_row() {
        failOnWarning(/.*/);
        view.band = "GR";
        view.rows = [{index: 0, label: "Channel", band: "GR", logo: ""}];
        view.programsJson = JSON.stringify([{name: "Current", startAt: 100, duration: 100,
            next: {name: "Next show", startAt: 200, duration: 100}}]);
        verify(waitForRendering(view));
        const next = findChild(view, "sidebarNextProgram");
        verify(next !== null);
        verify(next.visible);
        verify(next.text.endsWith("Next show"));
        compare(next.textFormat, Text.PlainText);
        verify(next.y + next.height <= next.parent.height);
        view.programsJson = "[]";
        tryCompare(next, "visible", false);
        view.rows = [];
    }

}
