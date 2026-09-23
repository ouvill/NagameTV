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
    function init() {
        failOnWarning(/.*/);
        backend.calls = 0;
        backend.accept = true;
        backend.recording_loading = false;
        started.clear();
    }
    function cleanup() {
        findChild(input, "recordingSource").close();
        const error = findChild(input, "recordingOpenError");
        error.close();
        tryCompare(error, "visible", false);
    }
    function test_open_preserves_url_and_waits_for_inspection() {
        const url = "file:///tmp/%E9%8C%B2%E7%94%BB%20%23100%25.ts";
        verify(input.openUrl(url));
        compare(backend.lastUrl, url);
        compare(backend.calls, 1);
        compare(started.count, 0);
        backend.recording_loading = false;
        backend.recordingOpened(true);
        compare(started.count, 1);
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
