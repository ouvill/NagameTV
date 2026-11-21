import QtQuick
import QtTest
import MinimalViewer

TestCase {
    id: testCase
    name: "RecordingActions"
    when: windowShown
    width: 900
    height: 650
    visible: true
    property bool deletionEnabled: true
    readonly property Player backend: deletionEnabled ? fixture.backend : fixture.browse_backend

    Window { id: chromeTarget }
    TestRecordingLibrary { id: fixture }
    Timer {
        id: backendPoll
        interval: 10
        repeat: true
        running: true
        onTriggered: testCase.backend.poll()
    }
    RecordingLibrary {
        id: library
        anchors.fill: parent
        backend: testCase.backend
        targetWindow: chromeTarget
    }

    function init() {
        failOnWarning(/.*/);
        load(true);
    }
    function load(deletion) {
        deletionEnabled = deletion;
        verify(fixture.load(deletion));
        tryCompare(list(), "count", 3);
        list().currentIndex = 0;
        library.focusRecordings(false);
        tryCompare(list(), "activeFocus", true);
    }
    function cleanup() {
        backend.stop();
        backendPoll.start();
    }
    function list() { return findChild(library, "epgstationRecordings"); }
    function dialog() { return findChild(library, "recordingDeletionDialog"); }
    function panel() { return findChild(library, "recordingActionPanel"); }
    function openActions() {
        keyClick(Qt.Key_Right);
        tryCompare(panel(), "opened", true);
        tryCompare(findChild(panel(), "recordingActionPlay"), "activeFocus", true);
    }
    function openDeletion() {
        openActions();
        keyClick(Qt.Key_Down); keyClick(Qt.Key_Down); keyClick(Qt.Key_Down);
        tryCompare(findChild(panel(), "recordingActionDelete"), "activeFocus", true);
        keyClick(Qt.Key_Return);
        tryCompare(dialog(), "opened", true);
        tryCompare(findChild(dialog(), "recordingDeletionCancel"), "activeFocus", true);
    }

    function test_enter_starts_playback_directly_from_the_selected_row() {
        backendPoll.stop();
        keyClick(Qt.Key_Return);
        verify(backend.recording_loading);
        verify(!panel().visible);
        verify(!findChild(library, "recordingFiles").visible);
    }

    function test_right_opens_and_each_dismiss_key_returns_to_the_same_row() {
        keyClick(Qt.Key_Down);
        for (const key of [Qt.Key_Back, Qt.Key_Escape, Qt.Key_Left]) {
            openActions();
            compare(panel().programName, "Second recording");
            keyClick(key);
            tryCompare(panel(), "visible", false);
            tryCompare(list(), "activeFocus", true);
            compare(list().currentItem.recordedId, "2");
        }
    }

    function test_delete_is_present_only_with_the_feature_and_details_remain_available() {
        openActions();
        verify(findChild(panel(), "recordingActionDelete").visible);
        keyClick(Qt.Key_Back);
        tryCompare(panel(), "visible", false);
        load(false);
        openActions();
        verify(!findChild(panel(), "recordingActionDelete").visible);
        keyClick(Qt.Key_Down); keyClick(Qt.Key_Down);
        tryCompare(findChild(panel(), "recordingActionDetails"), "activeFocus", true);
        keyClick(Qt.Key_Return);
        tryCompare(panel(), "showingDetails", true);
        const description = findChild(panel(), "recordingActionDescription");
        compare(description.text, "Original program description\n\nFull program notes");
        compare(description.textFormat, Text.PlainText);
        keyClick(Qt.Key_Back);
        tryCompare(panel(), "visible", false);
        compare(list().currentItem.recordedId, "1");
    }

    function test_more_is_pointer_only_and_appears_for_current_or_hovered_rows() {
        mouseMove(library, library.width / 2, 15);
        tryVerify(() => list().itemAtIndex(1) !== null);
        const first = findChild(list().itemAtIndex(0), "recordingMore");
        const secondRow = list().itemAtIndex(1);
        const second = findChild(secondRow, "recordingMore");
        verify(first.visible);
        compare(first.focusPolicy, Qt.NoFocus);
        verify(!second.visible);
        mouseMove(secondRow, secondRow.width / 2, secondRow.height / 2);
        tryCompare(second, "visible", true);
        mouseClick(second);
        tryCompare(panel(), "opened", true);
        compare(list().currentItem.recordedId, "2");
        keyClick(Qt.Key_Left);
        tryCompare(list(), "activeFocus", true);
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
        tryCompare(backend, "epgstation_deletion_state", "failed");
        tryCompare(findChild(dialog(), "recordingDeletionCancel"), "activeFocus", true);
        verify(findChild(dialog(), "recordingDeletionMessage").text.length > 0);
        compare(list().count, 3);
        keyClick(Qt.Key_Back);
        tryCompare(dialog(), "visible", false);
        tryCompare(list(), "activeFocus", true);
        compare(list().currentItem.recordedId, "3");
    }
}
