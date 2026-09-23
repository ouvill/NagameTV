import QtQuick
import QtTest
import MinimalViewer as Viewer

TestCase {
    id: testCase
    name: "ChannelSelector"
    when: windowShown
    width: 640
    height: 180

    Component {
        id: component
        Viewer.ChannelSelector {
            property alias rows: fixture.rows
            channels: ChannelFixture { id: fixture }
            width: 600
            rows: [
                { index: 0, label: "01 GR", band: "GR" },
                { index: 1, label: "02 GR", band: "GR" },
                { index: 2, label: "101 BS", band: "BS" },
                { index: 3, label: "201 BS", band: "BS" }
            ]
            selected: 0
        }
    }
    SignalSpy { id: selection; signalName: "selectRequested" }
    property var selector
    function init() {
        selector = createTemporaryObject(component, testCase)
        verify(selector !== null)
        selection.target = selector
        selection.clear()
    }
    function channel() { return findChild(selector, "channelSelector") }
    function bands() { return findChild(selector, "bandSelector") }
    function test_filter_does_not_select_and_uses_original_index() {
        bands().forceActiveFocus()
        keyClick(Qt.Key_Down) // ALL -> GR
        keyClick(Qt.Key_Down) // GR -> BS
        compare(selector.band, "BS")
        compare(channel().count, 2)
        compare(channel().currentIndex, -1)
        compare(selection.count, 0)
        channel().forceActiveFocus()
        keyClick(Qt.Key_Down)
        compare(selection.count, 1)
        compare(selection.signalArguments[0][0], 2) // Not filtered index zero.
        selector.selected = 2
        compare(channel().currentValue, 2)
        keyClick(Qt.Key_Down)
        compare(selection.signalArguments[1][0], 3)
    }
    function test_external_selection_reveals_channel_without_emitting() {
        selector.band = "BS"
        selector.selected = 2
        compare(channel().currentIndex, 0)
        selector.selected = 1
        compare(selector.band, "ALL")
        compare(bands().currentValue, "ALL")
        compare(channel().currentValue, 1)
        compare(selection.count, 0)
    }
    function test_empty_filter_and_server_replacement() {
        selector.band = "CS"
        compare(channel().count, 0)
        compare(channel().enabled, false)
        compare(channel().currentIndex, -1)
        selector.rows = []
        selector.selected = -1
        selector.band = "ALL"
        compare(channel().count, 0)
        selector.rows = [{ index: 0, label: "Replacement", band: "SKY" }]
        selector.selected = 0
        compare(channel().currentValue, 0)
        compare(channel().currentText, "Replacement")
        compare(selection.count, 0)
    }
    function test_same_size_update_refreshes_label_without_selecting() {
        const updated = selector.rows.slice()
        updated[0] = { index: 0, label: "Renamed station", band: "GR" }
        selector.rows = updated
        compare(channel().count, 4)
        compare(channel().currentValue, 0)
        compare(channel().currentText, "Renamed station")
        compare(selection.count, 0)
    }
}
