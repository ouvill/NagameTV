import QtQuick
import QtTest
import MinimalViewer

TestCase {
    id: testCase
    name: "LiveRecoveryNotice"
    when: windowShown
    visible: true
    width: 640
    height: 240
    property string recoveryState: "idle"
    property string message: ""
    property int retries: 0
    property int dismissals: 0
    LiveRecoveryNotice {
        id: notice
        x: 20; y: 20
        recoveryState: testCase.recoveryState
        message: testCase.message
        revealDelayMs: 120
        onRetryRequested: testCase.retries += 1
        onDismissFocusRequested: testCase.dismissals += 1
    }
    function init() {
        failOnWarning(/.*/);
        recoveryState = "idle"; message = ""; retries = 0; dismissals = 0;
    }
    function test_brief_recovery_stays_hidden() {
        recoveryState = "recovering"; message = "Reconnecting";
        compare(notice.visible, false);
        recoveryState = "idle"; message = "";
        wait(200);
        compare(notice.visible, false);
    }
    function test_lasting_recovery_shows_rust_message_without_retry() {
        recoveryState = "recovering"; message = "Video or audio stopped. Reconnecting to the same channel…";
        tryCompare(notice, "visible", true);
        compare(findChild(notice, "liveRecoveryText").text, message);
        verify(!findChild(notice, "liveRecoveryRetry").visible);
        recoveryState = "idle"; message = "";
        compare(notice.visible, false);
    }
    function test_failure_shows_immediately_and_retry_is_keyboard_operable() {
        recoveryState = "failed"; message = "Could not restore video or audio. Please retry.";
        compare(notice.visible, true);
        const retry = findChild(notice, "liveRecoveryRetry");
        verify(retry.visible);
        retry.forceActiveFocus(Qt.TabFocusReason);
        keyClick(Qt.Key_Return);
        compare(retries, 1);
        keyClick(Qt.Key_Escape);
        compare(dismissals, 1);
        compare(retries, 1);
    }
}
