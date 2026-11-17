import QtQuick
import QtQuick.Controls
import QtTest
import MinimalViewer

TestCase {
    id: testCase
    name: "ScreenshotNotice"
    when: windowShown
    visible: true
    width: 640
    height: 360
    ScreenshotNotice {
        id: notice
        x: 40; y: 40
        width: 400
        timeoutMs: 180
        iconDirectory: Qt.resolvedUrl("../../../assets/icons/")
    }
    SignalSpy { id: opened; target: notice; signalName: "openFolderRequested" }
    function init() {
        failOnWarning(/.*/);
        testCase.forceActiveFocus();
        mouseMove(testCase, 5, 5);
        notice.dismiss();
        opened.clear();
    }
    function cleanup() { notice.dismiss(); }
    function test_success_has_folder_action_and_failure_does_not() {
        compare(notice.visible, false);
        notice.showSaved();
        const folder = findChild(notice, "screenshotNoticeFolderButton");
        verify(folder.visible);
        mouseClick(folder);
        compare(opened.count, 1);
        notice.showFailure("Could not open the screenshot folder.");
        verify(!folder.visible);
        compare(notice.message, "Could not open the screenshot folder.");
        notice.showSaved();
        verify(!folder.visible);
        compare(notice.message, "Could not open the screenshot folder.");
        notice.dismiss();
        verify(folder.visible);
        compare(notice.message, qsTranslate("Main", "Screenshot saved."));
    }
    function test_multiple_saves_keep_latest_folder_and_do_not_cover_failure() {
        notice.showSaved("file:///first/one.png");
        notice.showSaved("file:///first/two.webp");
        compare(notice.savedCount, 2);
        notice.showSaved("file:///second/three.jpg");
        compare(notice.savedCount, 1);
        const folder = findChild(notice, "screenshotNoticeFolderButton");
        folder.forceActiveFocus(); keyClick(Qt.Key_Space);
        compare(opened.signalArguments[0][0].toString(), "file:///second/three.jpg");
        notice.showFailure("Save failed");
        notice.showSaved("file:///second/four.png");
        compare(notice.message, "Save failed");
        notice.dismiss();
        compare(notice.savedFile.toString(), "file:///second/four.png");
    }
    function test_timeout_waits_for_hover_and_keyboard_focus() {
        notice.showSaved();
        mouseMove(notice, 10, notice.height / 2);
        wait(notice.timeoutMs * 2);
        verify(notice.visible);
        const folder = findChild(notice, "screenshotNoticeFolderButton");
        folder.forceActiveFocus(Qt.TabFocusReason);
        mouseMove(testCase, 5, 5);
        wait(notice.timeoutMs * 2);
        verify(notice.visible);
        keyClick(Qt.Key_Space);
        compare(opened.count, 1);
        testCase.forceActiveFocus();
        tryCompare(notice, "visible", false);
    }
    function test_failure_after_focused_action_can_expire() {
        notice.showSaved();
        findChild(notice, "screenshotNoticeFolderButton").forceActiveFocus(Qt.TabFocusReason);
        notice.showFailure("Could not open the screenshot folder.");
        keyClick(Qt.Key_Space);
        compare(opened.count, 0);
        tryCompare(notice, "visible", false);
    }
}
