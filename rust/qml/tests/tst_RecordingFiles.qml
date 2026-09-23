import QtQuick
import QtQuick.Controls
import QtTest
import MinimalViewer

TestCase {
    id: testCase
    name: "RecordingFiles"
    when: windowShown
    width: 640
    height: 480
    visible: true
    TestRecordingFiles { id: fixture }
    RecordingFiles { id: dialog; files: fixture.files; recordedId: "9007199254740993" }
    SignalSpy { id: chosen; target: dialog; signalName: "fileChosen" }
    function init() {
        failOnWarning(/.*/);
        fixture.populate();
        chosen.clear();
        dialog.open();
        tryCompare(dialog, "opened", true);
    }
    function cleanup() { dialog.close(); tryCompare(dialog, "visible", false); }
    function test_keyboard_selection_preserves_large_ids_and_plain_text() {
        const list = findChild(dialog, "recordingFileList");
        tryVerify(() => list.itemAtIndex(1) !== null);
        compare(list.itemAtIndex(1).contentItem.textFormat, Text.PlainText);
        verify(list.itemAtIndex(1).text.indexOf("<b>HEVC</b>") >= 0);
        keyClick(Qt.Key_Down);
        compare(list.currentIndex, 1);
        keyClick(Qt.Key_Return);
        tryCompare(chosen, "count", 1);
        compare(chosen.signalArguments[0][0], "9007199254740993");
        compare(chosen.signalArguments[0][1], "18446744073709551615");
        tryCompare(dialog, "visible", false);
    }
    function test_cancel_and_catalogue_reset_do_not_choose_a_file() {
        keyClick(Qt.Key_Escape);
        tryCompare(dialog, "visible", false);
        compare(chosen.count, 0);
        dialog.open();
        tryCompare(dialog, "opened", true);
        fixture.clear();
        tryCompare(dialog, "visible", false);
        compare(chosen.count, 0);
    }
}
