import QtQuick
import QtQuick.Controls
import QtTest
import MinimalViewer

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
        property string server_product: ""
        property int requests: 0
        property int searches: 0
        property string requestedUrl: ""
        property string lan_search_kind: ""
        property string lan_search_status: "idle"
        property ListModel lan_discovery_model: ListModel {}
        signal connectionFinished(bool success, int channels)
        function connect_server(url) { requests++; requestedUrl = url; loading = true; return true; }
        function search_lan(kind) {
            searches++;
            lan_search_kind = kind;
            lan_search_status = "searching";
        }
        function cancel_lan_search(kind) {
            if (lan_search_kind !== kind) return;
            lan_search_status = "idle";
            lan_search_kind = "";
            lan_discovery_model.clear();
        }
    }
    FirstRunSetup { id: setup; backend: backend }
    SignalSpy { id: completed; target: setup; signalName: "completed" }
    SignalSpy { id: openFile; target: setup; signalName: "openFileRequested" }
    function form() { return findChild(setup.contentItem, "setupConnectionForm"); }
    function field() { return findChild(setup.contentItem, "serverField"); }
    function action() { return findChild(setup.contentItem, "connectServer"); }
    function search() { return findChild(setup.contentItem, "lanSearch_tuner"); }
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
        backend.server_product = "";
        backend.requests = 0;
        backend.searches = 0;
        backend.requestedUrl = "";
        backend.cancel_lan_search("tuner");
        completed.clear();
        openFile.clear();
        setup.open();
        tryCompare(setup, "opened", true);
    }
    function cleanup() {
        setup.close();
        tryCompare(setup, "visible", false);
        testCase.Window.window.showNormal();
    }
    function test_open_recording_does_not_require_a_server() {
        const button = findChild(setup.contentItem, "setupOpenRecording");
        verify(button.visible);
        verify(waitForRendering(button));
        mouseClick(button);
        compare(openFile.count, 1);
        compare(backend.requests, 0);
    }
    function test_initial_input_is_empty_and_does_not_connect_automatically() {
        compare(field().text, "");
        verify(search().activeFocus);
        compare(backend.searches, 1);
        compare(backend.lan_search_status, "searching");
        compare(action().enabled, false);
        compare(backend.requests, 0);
        keyClick(Qt.Key_Down);
        compare(setup.opened, true);
        verify(field().activeFocus);
        compare(field().interaction, NavigationField.Navigating);
        field().text = "   ";
        form().connectToServer();
        compare(backend.requests, 0);
    }
    function test_connection_failure_retains_visible_retry_in_fullscreen() {
        const host = setup.contentItem.Window.window;
        host.showFullScreen();
        tryCompare(host, "visibility", Window.FullScreen);
        keyClick(Qt.Key_Down);
        verify(field().activeFocus);
        keyClick(Qt.Key_Return);
        compare(field().interaction, NavigationField.Editing);
        field().text = "http://example.test:40772";
        keyClick(Qt.Key_Return);
        compare(form().phase, ConnectionForm.Checking);
        backend.status = "HTTP 403";
        finish(false, 0);
        verify(action().activeFocus);
        verify(action().focusVisible);
        keyClick(Qt.Key_Up);
        const disclosure = findChild(setup.contentItem, "connectionDetailsToggle");
        verify(disclosure.focusVisible);
        keyClick(Qt.Key_Return);
        verify(findChild(setup.contentItem, "connectionErrorDetails").visible);
        keyClick(Qt.Key_Down);
        verify(action().focusVisible);
        keyClick(Qt.Key_Return);
        backend.settings_error = "Disk full";
        finish(true, 4);
        compare(form().phase, ConnectionForm.SaveFailed);
        keyClick(Qt.Key_Up);
        verify(disclosure.focusVisible);
        keyClick(Qt.Key_Return);
        compare(findChild(setup.contentItem, "connectionErrorDetails").text, "Disk full");
        keyClick(Qt.Key_Escape);
        verify(setup.opened);
        verify(disclosure.activeFocus);
    }
    function test_discovered_server_can_be_selected_and_connected_with_the_dpad() {
        backend.lan_discovery_model.append({serverUrl: "http://192.168.0.2:40772", productName: "Mirakurun", serverVersion: "4.1.3"});
        backend.lan_discovery_model.append({serverUrl: "http://192.168.0.3:40772", productName: "mirakc", serverVersion: "3.4.89"});
        backend.lan_search_status = "finished";
        const first = findChild(setup.contentItem, "lanResult_tuner_0");
        const second = findChild(setup.contentItem, "lanResult_tuner_1");
        verify(first.text.includes("Mirakurun 4.1.3"));
        keyClick(Qt.Key_Down);
        verify(first.activeFocus);
        keyClick(Qt.Key_Down);
        verify(second.activeFocus);
        keyClick(Qt.Key_Return);
        compare(backend.requests, 1);
        compare(backend.requestedUrl, "http://192.168.0.3:40772");
        backend.status = "HTTP 403";
        finish(false, 0);
        compare(form().phase, ConnectionForm.Failed);
        verify(action().activeFocus);
        verify(action().focusVisible);
        keyClick(Qt.Key_Return);
        compare(backend.requests, 2);
        compare(backend.requestedUrl, "http://192.168.0.3:40772");
        finish(true, 2);
        verify(action().activeFocus);
        verify(action().focusVisible);
        keyClick(Qt.Key_Return);
        compare(completed.count, 1);
    }
    function test_cancel_search_and_start_again() {
        keyClick(Qt.Key_Return);
        compare(backend.lan_search_status, "idle");
        keyClick(Qt.Key_Return);
        compare(backend.searches, 2);
        compare(backend.lan_search_status, "searching");
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
        verify(field().editingEnabled);
        verify(action().enabled);
        const details = findChild(setup.contentItem, "connectionErrorDetails");
        form().detailsVisible = true;
        compare(details.textFormat, Text.PlainText);
        compare(details.text, backend.status);
        backend.status = "Translated connection failure";
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
        backend.settings_error = "Translated save failure";
        compare(form().errorDetails, backend.settings_error);
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
    function test_empty_catalog_names_the_responding_server_only_when_known() {
        const result = findChild(setup.contentItem, "connectionResult");
        field().text = "http://example.test:40772";
        form().connectToServer();
        finish(true, 0);
        compare(form().phase, ConnectionForm.Empty);
        verify(result.text.indexOf("the server") >= 0);
        backend.server_product = "mirakc";
        verify(result.text.indexOf("mirakc") >= 0);
        verify(result.text.indexOf("the server") < 0);
    }
    function test_setup_help_offers_both_servers_and_is_reachable_by_keys() {
        const mirakurun = findChild(setup.contentItem, "mirakurunSetupHelp");
        const mirakc = findChild(setup.contentItem, "mirakcSetupHelp");
        verify(mirakurun.visible);
        verify(mirakc.visible);
        keyClick(Qt.Key_Down);
        field().text = "http://example.test:40772";
        keyClick(Qt.Key_Down);
        verify(action().activeFocus);
        keyClick(Qt.Key_Down);
        verify(mirakurun.activeFocus);
        keyClick(Qt.Key_Down);
        verify(mirakc.activeFocus);
        keyClick(Qt.Key_Down);
        verify(findChild(setup.contentItem, "setupOpenRecording").activeFocus);
    }
    function test_editing_successful_address_requires_another_check() {
        field().text = "http://example.test:40772";
        form().connectToServer();
        finish(true, 4);
        field().focusForEditing();
        keyClick(Qt.Key_End);
        keyClick(Qt.Key_Backspace);
        compare(form().phase, ConnectionForm.Idle);
        compare(completed.count, 0);
        keyClick(Qt.Key_Return);
        compare(backend.requests, 2);
        compare(form().phase, ConnectionForm.Checking);
    }
    function test_initial_editing_then_directional_navigation() {
        keyClick(Qt.Key_Down);
        verify(field().activeFocus);
        keyClick(Qt.Key_Return);
        keyClick(Qt.Key_H);
        compare(field().text, "h");
        keyClick(Qt.Key_Down);
        verify(field().activeFocus);
        keyClick(Qt.Key_Escape);
        keyClick(Qt.Key_Down);
        verify(action().activeFocus);
        keyClick(Qt.Key_Up);
        verify(field().activeFocus);
        compare(field().interaction, NavigationField.Navigating);
        keyClick(Qt.Key_Return);
        compare(field().interaction, NavigationField.Editing);
        compare(backend.requests, 0);
        keyClick(Qt.Key_Return);
        compare(backend.requests, 1);
        compare(field().interaction, NavigationField.Navigating);
    }
}
