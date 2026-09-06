import QtQuick
import QtTest
import ".."

TestCase {
    name: "StoppedPlayback"
    when: windowShown
    width: 900
    height: 560
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
        panel.loading = true;
        mouseClick(action);
        compare(play.count, 1);
        compare(channels.count, 1);
        compare(settings.count, 1);
        const status = findChild(panel, "stoppedStatus");
        compare(status.textFormat, Text.PlainText);
        compare(status.text, panel.status);
    }
}
