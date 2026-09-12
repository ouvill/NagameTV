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
        property string language: "en"
        property string subtitle_status: ""
        property bool comments_enabled: false
        property bool comments_allowed: true
        property bool danmaku_enabled: false
        property real comment_font_size: 21
        property real comment_opacity: 1
        property real comment_speed: 1
        property string comment_status: ""
        property bool comment_send_on_enter: false
        function configure_comment_send_on_enter(value) { comment_send_on_enter = value; }
        property string log_error: ""
        function request_language(value) { language = value; return true; }
        function configure_features(subtitles, epg) {
            subtitles_enabled = subtitles;
            epg_enabled = epg;
        }
        function display_subtitles(value) { subtitle_display = value; }
        function enable_comments(value) { comments_enabled = value; }
        function configure_danmaku(value, size, opacity, speed) {
            danmaku_enabled = value;
            comment_font_size = size;
            comment_opacity = opacity;
            comment_speed = speed;
            return true;
        }
        function open_log_folder() { return true; }
        signal connectRequested(string url)
        property bool acceptConnection: false
        function connect_server(url) { connectRequested(url); return acceptConnection; }
    }
    SettingsDrawer {
        id: drawer
        backend: backend
        collapseIcon: Qt.resolvedUrl("../../../assets/icons/panel-right-close.svg")
        dropdownIcon: Qt.resolvedUrl("../../../assets/icons/chevron-down.svg")
    }
    SignalSpy { id: connections; target: backend; signalName: "connectRequested" }
    SignalSpy { id: accepted; target: drawer; signalName: "connectionAccepted" }
    function init() {
        failOnWarning(/.*/);
        backend.loading = false;
        backend.acceptConnection = false;
        backend.diagnostics = "";
        connections.clear();
        accepted.clear();
        drawer.open();
        tryCompare(drawer, "opened", true);
    }
    function cleanup() { drawer.close(); tryCompare(drawer, "visible", false); }
    function test_comment_send_shortcut_tracks_setting_and_can_be_toggled() {
        const choice = findChild(drawer.contentItem, "commentSendOnEnter");
        backend.comment_send_on_enter = false;
        compare(choice.checked, false);
        choice.forceActiveFocus();
        keyClick(Qt.Key_Space);
        compare(backend.comment_send_on_enter, true);
        compare(choice.checked, true);
        keyClick(Qt.Key_Space);
        compare(backend.comment_send_on_enter, false);
    }
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
        compare(accepted.count, 0);
        compare(findChild(drawer.contentItem, "connectionError").visible, true);
        backend.acceptConnection = true;
        drawer.connectToServer();
        tryCompare(drawer, "visible", false);
        compare(accepted.count, 1);
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
    function test_diagnostics_remain_readable_data() {
        return [
            {tag: "English", text: "Subtitles: subscriptions 8, pending 128, received 18446744073709551615 | EPG: tasks 1, programs 50000, stopping Yes"},
            {tag: "Japanese", text: "字幕: 購読 8, 待機 128, 受信 18446744073709551615 | EPG: タスク 1, 番組 50000, 停止待ち はい"}
        ];
    }
    function test_diagnostics_remain_readable(data) {
        backend.diagnostics = data.text;
        const label = findChild(drawer.contentItem, "featureDiagnostics");
        verify(label !== null);
        tryVerify(() => label.lineCount > 1);
        compare(label.truncated, false);
        compare(label.text, data.text);
        verify(label.x + label.width <= drawer.contentItem.availableWidth);
        tryVerify(() => label.height >= label.contentHeight);
    }
}
