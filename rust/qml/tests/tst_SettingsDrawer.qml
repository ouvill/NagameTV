import QtQuick
import QtTest
import ".."

TestCase {
    id: testCase
    name: "SettingsDrawer"
    when: windowShown
    visible: true
    width: 900
    height: 560
    QtObject {
        id: backend
        property string server: "http://example.test:40772"
        property bool loading: false
        property bool subtitles_enabled: false
        property bool subtitles_allowed: true
        property bool subtitle_display: true
        property bool epg_enabled: true
        property bool epg_allowed: true
        property string settings_error: ""
        property string diagnostics: ""
        property string status: ""
        signal connectRequested(string url)
        property bool acceptConnection: false
        function connect_server(url) { connectRequested(url); return acceptConnection; }
    }
    SettingsDrawer {
        id: drawer
        backend: backend
        collapseIcon: Qt.resolvedUrl("../../../assets/icons/panel-right-close.svg")
    }
    SignalSpy { id: connections; target: backend; signalName: "connectRequested" }
    function init() {
        failOnWarning(/.*/);
        backend.loading = false;
        backend.acceptConnection = false;
        connections.clear();
        drawer.open();
        tryCompare(drawer, "opened", true);
    }
    function cleanup() { drawer.close(); tryCompare(drawer, "visible", false); }
    function test_connection_uses_edited_url_without_duplicate_loading_request() {
        const field = findChild(drawer.contentItem, "serverField");
        const connect = findChild(drawer.contentItem, "connectServer");
        field.text = "http://new.example:40772";
        mouseClick(connect);
        compare(connections.count, 1);
        compare(connections.signalArguments[0][0], field.text);
        backend.loading = true;
        field.forceActiveFocus();
        keyClick(Qt.Key_Return);
        compare(connections.count, 1);
        compare(connect.enabled, false);
    }
    function test_accepted_connection_closes_and_rejection_stays_open() {
        drawer.connectToServer();
        compare(drawer.opened, true);
        compare(findChild(drawer.contentItem, "connectionError").visible, true);
        backend.acceptConnection = true;
        drawer.connectToServer();
        tryCompare(drawer, "visible", false);
        compare(findChild(drawer.contentItem, "connectionError").visible, false);
    }
    function test_right_edge_and_escape_close() {
        compare(drawer.width, 420);
        compare(drawer.height, 560);
        compare(drawer.edge, Qt.RightEdge);
        compare(drawer.contentItem.x, 28);
        compare(drawer.contentItem.y, 28);
        compare(drawer.contentItem.width, 364);
        fuzzyCompare(drawer.x + drawer.width, 900, 1);
        keyClick(Qt.Key_Escape);
        tryCompare(drawer, "visible", false);
    }
}
