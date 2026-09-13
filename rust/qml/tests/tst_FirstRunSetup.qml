import QtQuick
import QtQuick.Controls
import QtTest
import ".."

TestCase {
    id: testCase
    name: "FirstRunSetup"
    when: windowShown
    visible: true
    width: 900
    height: 560
    QtObject {
        id: backend
        property string server: ""
        property bool loading: false
        property string status: ""
        property string settings_error: ""
        property int requests: 0
        signal connectionFinished(bool success, int channels)
        function connect_server(url) { requests++; loading = true; return true; }
    }
    FirstRunSetup { id: setup; backend: backend }
    SignalSpy { id: completed; target: setup; signalName: "completed" }
    function form() { return findChild(setup.contentItem, "setupConnectionForm"); }
    function field() { return findChild(setup.contentItem, "serverField"); }
    function action() { return findChild(setup.contentItem, "connectServer"); }
    function finish(success, count) {
        backend.loading = false;
        backend.connectionFinished(success, count);
    }
    function init() {
        failOnWarning(/.*/);
        backend.server = "";
        backend.loading = false;
        backend.status = "";
        backend.settings_error = "";
        backend.requests = 0;
        completed.clear();
        setup.open();
        tryCompare(setup, "opened", true);
    }
    function cleanup() { setup.close(); tryCompare(setup, "visible", false); }
    function test_initial_input_is_empty_and_does_not_connect_automatically() {
        compare(field().text, "");
        verify(field().activeFocus);
        compare(action().enabled, false);
        compare(backend.requests, 0);
        keyClick(Qt.Key_Escape);
        compare(setup.opened, true);
        field().text = "   ";
        form().connectToServer();
        compare(backend.requests, 0);
    }
    function test_failure_empty_catalog_and_save_failure_all_allow_retry() {
        field().text = "http://example.test:40772";
        form().connectToServer();
        compare(form().phase, ConnectionForm.Checking);
        verify(field().readOnly);
        verify(!action().enabled);
        form().connectToServer();
        compare(backend.requests, 1);
        backend.status = "<b>HTTP 403</b>";
        finish(false, 0);
        compare(form().phase, ConnectionForm.Failed);
        compare(field().text, "http://example.test:40772");
        verify(!field().readOnly);
        verify(action().enabled);
        const details = findChild(setup.contentItem, "connectionErrorDetails");
        form().detailsVisible = true;
        compare(details.textFormat, Text.PlainText);
        compare(details.text, backend.status);
        form().connectToServer();
        finish(true, 0);
        compare(form().phase, ConnectionForm.Empty);
        compare(completed.count, 0);
        verify(action().enabled);
        form().connectToServer();
        backend.settings_error = "Disk full";
        finish(true, 4);
        compare(form().phase, ConnectionForm.SaveFailed);
        compare(form().errorDetails, "Disk full");
        compare(completed.count, 0);
        form().connectToServer();
        backend.settings_error = "";
        finish(true, 4);
        compare(form().phase, ConnectionForm.Ready);
        compare(setup.opened, true);
        compare(completed.count, 0);
        compare(backend.requests, 4);
        verify(action().activeFocus);
        keyClick(Qt.Key_Space);
        tryCompare(setup, "visible", false);
        compare(completed.count, 1);
        compare(backend.requests, 4);
    }
    function test_editing_successful_address_requires_another_check() {
        field().text = "http://example.test:40772";
        form().connectToServer();
        finish(true, 4);
        field().forceActiveFocus();
        keyClick(Qt.Key_End);
        keyClick(Qt.Key_Backspace);
        compare(form().phase, ConnectionForm.Idle);
        compare(completed.count, 0);
        keyClick(Qt.Key_Return);
        compare(backend.requests, 2);
        compare(form().phase, ConnectionForm.Checking);
    }
}
