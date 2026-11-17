import QtQuick
import QtQuick.Controls
import QtTest
import MinimalViewer

TestCase {
    name: "RecordingInput"
    when: windowShown
    visible: true
    width: 640
    height: 480
    QtObject {
        id: backend
        property int calls: 0
        property string lastUrl: ""
        property bool accept: true
        property bool recording_loading: false
        property string file_error: "<b>Invalid recording</b>"
        property string playback_error: ""
        signal recordingOpened(bool success)
        function open_recording(url) { calls++; lastUrl = String(url); recording_loading = accept; return accept; }
        function open_recording_transfer(key) { return open_recording(key); }
        function cancel_recording_open() { recording_loading = false; }
    }
    RecordingInput { id: input; anchors.fill: parent; backend: backend }
    SignalSpy { id: started; target: input; signalName: "started" }
    SignalSpy { id: library; target: input; signalName: "libraryRequested" }
    TestInputMethod { id: inputEvents }
    function init() {
        failOnWarning(/.*/);
        backend.calls = 0;
        backend.accept = true;
        backend.recording_loading = false;
        started.clear();
        library.clear();
    }
    function cleanup() {
        findChild(input, "recordingSource").close();
        const error = findChild(input, "recordingOpenError");
        error.close();
        tryCompare(error, "visible", false);
    }
    function test_open_preserves_url_and_waits_for_inspection_data() {
        return [
            {tag: "ts", url: "file:///tmp/%E9%8C%B2%E7%94%BB%20%23100%25.ts"},
            {tag: "mp4", url: "file:///tmp/%E9%8C%B2%E7%94%BB%20%23100%25.mp4"},
            {tag: "mkv", url: "file:///tmp/recording.mkv"},
            {tag: "http", url: "https://example.invalid/api/videos/123?token=private"}
        ];
    }
    function test_open_preserves_url_and_waits_for_inspection(data) {
        const url = data.url;
        verify(input.openUrl(url));
        compare(backend.lastUrl, url);
        compare(backend.calls, 1);
        compare(started.count, 0);
        backend.recording_loading = false;
        backend.recordingOpened(true);
        compare(started.count, 1);
    }
    function test_browse_library_closes_source_without_changing_playback() {
        input.open();
        const dialog = findChild(input, "recordingSource");
        tryCompare(dialog, "opened", true);
        mouseClick(findChild(input, "recordingBrowseEpgstation"));
        tryCompare(dialog, "visible", false);
        compare(library.count, 1);
        compare(backend.calls, 0);
        compare(started.count, 0);
    }
    function test_url_dialog_submission_and_cancel() {
        input.open();
        const dialog = findChild(input, "recordingSource");
        const field = findChild(input, "recordingUrl");
        const open = findChild(input, "recordingOpenUrl");
        tryCompare(dialog, "opened", true);
        field.text = "  ";
        verify(!open.enabled);
        field.text = "http://epgstation:8888/api/videos/123?isDownload=true";
        verify(open.enabled);
        mouseClick(open);
        tryCompare(dialog, "visible", false);
        compare(backend.lastUrl, field.text);
        compare(started.count, 0);
        backend.recording_loading = false;
        backend.recordingOpened(true);
        compare(started.count, 1);
        input.open();
        tryCompare(dialog, "opened", true);
        dialog.reject();
        compare(backend.calls, 1);
    }
    function test_directional_url_editing_and_nested_back() {
        input.open();
        const dialog = findChild(input, "recordingSource");
        const field = findChild(input, "recordingUrl");
        tryCompare(dialog, "opened", true);
        field.text = "http://example.test/video.ts";
        compare(field.interaction, NavigationField.Navigating);
        keyClick(Qt.Key_Up);
        if (findChild(input, "recordingBrowseEpgstation").activeFocus) keyClick(Qt.Key_Left);
        verify(findChild(input, "recordingChooseFile").activeFocus);
        keyClick(Qt.Key_Right);
        verify(findChild(input, "recordingBrowseEpgstation").activeFocus);
        keyClick(Qt.Key_Down);
        verify(field.activeFocus);
        keyClick(Qt.Key_Return);
        compare(field.interaction, NavigationField.Editing);
        verify(inputEvents.forward_key(Qt.Key_Return, Qt.NoModifier, "", true));
        compare(backend.calls, 0);
        field.cursorPosition = field.text.length;
        keyClick(Qt.Key_Left);
        compare(field.cursorPosition, field.text.length - 1);
        keyClick(Qt.Key_Down);
        verify(field.activeFocus);
        verify(inputEvents.compose("けんさく", ""));
        keyClick(Qt.Key_Return);
        compare(backend.calls, 0); verify(dialog.opened);
        verify(inputEvents.compose("けんさく", ""));
        keyClick(Qt.Key_Escape);
        compare(field.interaction, NavigationField.Editing);
        verify(dialog.opened);
        verify(inputEvents.compose("", "検索"));
        keyClick(Qt.Key_Escape);
        compare(field.interaction, NavigationField.Navigating);
        verify(dialog.opened);
        keyClick(Qt.Key_Down);
        verify(findChild(input, "recordingCancelSource").activeFocus);
        keyClick(Qt.Key_Return);
        tryCompare(dialog, "visible", false);
        compare(backend.calls, 0);
    }
    function test_rejected_file_reports_plain_text_without_starting() {
        backend.accept = false;
        verify(!input.openUrl("file:///tmp/invalid.ts"));
        compare(started.count, 0);
        const error = findChild(input, "recordingOpenError");
        tryCompare(error, "opened", true);
        compare(error.contentItem.text, backend.file_error);
        compare(error.contentItem.textFormat, Text.PlainText);
    }
    function test_rejected_transfer_reports_error_without_starting() {
        backend.accept = false;
        verify(!input.openTransfer("expired-key"));
        compare(backend.calls, 1);
        compare(started.count, 0);
        const error = findChild(input, "recordingOpenError");
        tryCompare(error, "opened", true);
        compare(error.contentItem.text, backend.file_error);
    }
    function test_async_failure_and_cancel() {
        verify(input.openUrl("file:///tmp/invalid.ts"));
        const opening = findChild(input, "recordingOpening");
        tryCompare(opening, "opened", true);
        mouseClick(findChild(input, "cancelRecordingOpen"));
        tryCompare(opening, "visible", false);
        compare(started.count, 0);
        verify(input.openUrl("file:///tmp/invalid.ts"));
        backend.recording_loading = false;
        backend.recordingOpened(false);
        const error = findChild(input, "recordingOpenError");
        tryCompare(error, "opened", true);
        compare(started.count, 0);
        compare(error.contentItem.text, backend.file_error);
    }
}
