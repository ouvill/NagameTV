import QtQuick
import QtTest
import MinimalViewer

TestCase {
    id: testCase
    name: "RecordingDeletion"
    when: windowShown
    width: 900
    height: 650
    visible: true

    Window { id: chromeTarget }
    TestRecordingLibrary { id: fixture }
    Timer {
        interval: 10
        repeat: true
        running: true
        onTriggered: fixture.backend.poll()
    }
    RecordingLibrary {
        id: library
        anchors.fill: parent
        backend: fixture.backend
        targetWindow: chromeTarget
    }

    function init() {
        failOnWarning(/.*/);
        verify(fixture.load());
        tryCompare(list(), "count", 3);
        list().currentIndex = 0;
        library.focusRecordings(false);
        tryCompare(list(), "activeFocus", true);
    }
    function list() { return findChild(library, "epgstationRecordings"); }
    function dialog() { return findChild(library, "recordingDeletionDialog"); }
    function openDeletion() {
        keyClick(Qt.Key_Right);
        tryCompare(findChild(list().currentItem, "epgstationDelete"), "activeFocus", true);
        keyClick(Qt.Key_Return);
        tryCompare(dialog(), "opened", true);
        tryCompare(findChild(dialog(), "recordingDeletionCancel"), "activeFocus", true);
    }

    function test_cancel_is_default_and_back_cancels_even_on_delete() {
        openDeletion();
        const title = findChild(dialog(), "recordingDeletionProgram");
        compare(title.text, "<b>First recording</b>");
        compare(title.textFormat, Text.PlainText);
        verify(findChild(dialog(), "recordingDeletionChannelDate").text.startsWith("Test TV  "));
        keyClick(Qt.Key_Return);
        tryCompare(dialog(), "visible", false);
        tryCompare(list(), "activeFocus", true);
        compare(list().count, 3);
        openDeletion();
        keyClick(Qt.Key_Right);
        tryCompare(findChild(dialog(), "recordingDeletionConfirm"), "activeFocus", true);
        keyClick(Qt.Key_Back);
        tryCompare(dialog(), "visible", false);
        compare(list().count, 3);
    }

    function test_delete_removes_the_row_and_focuses_its_neighbour() {
        keyClick(Qt.Key_Down);
        compare(list().currentIndex, 1);
        openDeletion();
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Return);
        tryCompare(dialog(), "visible", false);
        tryCompare(list(), "count", 2);
        tryCompare(list(), "activeFocus", true);
        compare(list().currentIndex, 1);
        compare(list().currentItem.recordedId, "3");
    }

    function test_failed_delete_keeps_the_row_and_returns_focus() {
        keyClick(Qt.Key_Down); keyClick(Qt.Key_Down);
        openDeletion();
        keyClick(Qt.Key_Right);
        keyClick(Qt.Key_Return);
        tryCompare(fixture.backend, "epgstation_deletion_state", "failed");
        tryCompare(findChild(dialog(), "recordingDeletionCancel"), "activeFocus", true);
        verify(findChild(dialog(), "recordingDeletionMessage").text.length > 0);
        compare(list().count, 3);
        keyClick(Qt.Key_Back);
        tryCompare(dialog(), "visible", false);
        tryCompare(list(), "activeFocus", true);
        compare(list().currentItem.recordedId, "3");
    }
}
