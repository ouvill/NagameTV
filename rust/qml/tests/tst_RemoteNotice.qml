import QtQuick
import QtTest
import MinimalViewer

TestCase {
    id: testCase
    name: "RemoteNotice"
    when: windowShown
    visible: true
    width: 480
    height: 200
    QtObject {
        id: backend
        property real volume_level: 0.3
        property bool audio_muted: false
        property bool subtitle_display: true
        property bool paused: false
        property int playback_rate: 15
    }
    RemoteNotice {
        id: notice
        x: 20; y: 20
        backend: backend
        timeoutMs: 150
        iconDirectory: Qt.resolvedUrl("../../../assets/icons/")
    }
    function init() { failOnWarning(/.*/); }
    function test_commands_show_translated_feedback_then_hide() {
        compare(notice.visible, false);
        notice.show(4);
        compare(notice.visible, true);
        compare(findChild(notice, "remoteNoticeText").text, "Volume 30%");
        notice.show(11);
        compare(notice.message, "Speed 1.5×");
        notice.show(1);
        compare(notice.message, "Channel changed");
        tryCompare(notice, "visible", false, 1000);
    }
    function test_key_forwarding_and_unknown_kinds_stay_silent() {
        notice.show(15);
        compare(notice.visible, false);
        notice.show(0);
        compare(notice.visible, false);
    }
}
