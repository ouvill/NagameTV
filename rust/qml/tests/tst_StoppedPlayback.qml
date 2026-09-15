import QtQuick
import QtTest
import ".."

TestCase {
    id: testCase
    name: "StoppedPlayback"
    when: windowShown
    width: 640
    height: 480
    visible: true
    StoppedPlayback {
        id: panel
        anchors.fill: parent
        status: "<b>Connection failed</b>"
        canPlay: false
        hasChannels: false
    }
    SignalSpy { id: play; target: panel; signalName: "playRequested" }
    SignalSpy { id: channels; target: panel; signalName: "channelsRequested" }
    SignalSpy { id: settings; target: panel; signalName: "settingsRequested" }
    SignalSpy { id: reconnect; target: panel; signalName: "reconnectRequested" }
    SignalSpy { id: openFile; target: panel; signalName: "openFileRequested" }
    function init() {
        panel.hasServer = false;
        panel.recording = false;
        openFile.clear();
        reconnect.clear();
        panel.loading = false;
        panel.canPlay = false;
        panel.hasChannels = false;
        panel.playbackError = "";
        play.clear(); channels.clear(); settings.clear();
    }
    function test_recording_can_be_replayed_without_channels() {
        panel.recording = true;
        panel.canPlay = true;
        verify(waitForRendering(panel));
        mouseClick(findChild(panel, "stoppedAction"));
        compare(play.count, 1);
        compare(reconnect.count, 0);
        mouseClick(findChild(panel, "openRecording"));
        compare(openFile.count, 1);
        compare(settings.count, 0);
    }
    function test_configured_server_failure_offers_reconnect_and_change() {
        failOnWarning(/.*/);
        panel.hasServer = true;
        verify(waitForRendering(panel));
        mouseClick(findChild(panel, "stoppedAction"));
        compare(reconnect.count, 1);
        compare(settings.count, 0);
        const change = findChild(panel, "changeConnection");
        verify(change.visible);
        mouseClick(change);
        compare(settings.count, 1);
        panel.loading = true;
        verify(!change.visible);
        compare(reconnect.count, 1);
    }
    function test_error_details_follow_failure_and_release_on_clear() {
        failOnWarning(/.*/);
        panel.playbackError = "<b>Stream failed</b>";
        verify(waitForRendering(panel));
        const b = findChild(panel, "errorDetailsAction");
        mouseClick(b);
        const loader = findChild(panel, "errorDetailsLoader");
        tryVerify(() => loader.item !== null);
        tryCompare(loader.item, "opened", true);
        const text = findChild(loader.item.contentItem, "playbackErrorText");
        compare(text.textFormat, TextEdit.PlainText);
        compare(text.text, panel.playbackError);
        panel.playbackError = "Updated failure";
        compare(text.text, panel.playbackError);
        keyClick(Qt.Key_Escape);
        tryVerify(() => loader.item === null);
        mouseClick(findChild(panel, "errorDetailsAction"));
        tryVerify(() => loader.item !== null);
        panel.playbackError = "";
        tryVerify(() => loader.item === null);
    }
    function test_state_routes_action_and_loading_prevents_requests() {
        failOnWarning(/.*/);
        const action = findChild(panel, "stoppedAction");
        mouseClick(action);
        compare(settings.count, 1);
        panel.hasChannels = true;
        mouseClick(action);
        compare(channels.count, 1);
        panel.canPlay = true;
        mouseClick(action);
        compare(play.count, 1);
        const spinner = findChild(panel, "stoppedLoading");
        verify(!spinner.visible);
        verify(!spinner.running);
        panel.loading = true;
        verify(!action.visible);
        verify(!action.enabled);
        verify(spinner.visible);
        verify(spinner.running);
        mouseClick(spinner);
        compare(play.count, 1);
        compare(channels.count, 1);
        compare(settings.count, 1);
        panel.loading = false;
        verify(action.visible);
        verify(action.enabled);
        verify(!spinner.visible);
        verify(!spinner.running);
        const status = findChild(panel, "stoppedStatus");
        compare(status.textFormat, Text.PlainText);
        compare(status.text, panel.status);
    }
}
