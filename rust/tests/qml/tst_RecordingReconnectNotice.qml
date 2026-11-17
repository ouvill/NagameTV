import QtQuick
import QtTest
import MinimalViewer

TestCase {
    id: testCase
    name: "RecordingReconnectNotice"
    when: windowShown
    visible: true
    width: 480
    height: 200
    property bool reconnecting: false
    RecordingReconnectNotice {
        id: notice
        x: 20; y: 20
        reconnecting: testCase.reconnecting
        revealDelayMs: 120
    }
    function init() { failOnWarning(/.*/); reconnecting = false; }
    function test_brief_reconnects_stay_hidden() {
        reconnecting = true;
        compare(notice.visible, false);
        reconnecting = false;
        wait(200);
        compare(notice.visible, false);
    }
    function test_lasting_reconnect_is_announced_until_recovered() {
        reconnecting = true;
        tryCompare(notice, "visible", true);
        compare(findChild(notice, "recordingReconnectText").text,
            "Reconnecting to the recording server…");
        reconnecting = false;
        compare(notice.visible, false);
    }
}
