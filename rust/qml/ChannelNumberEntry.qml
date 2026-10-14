import QtQuick
import QtQuick.Controls
import MinimalViewer

// Station buttons typed as digits, as on a remote: 1–9 tune at once, and a
// leading 1 waits briefly for the second digit of 10, 11 or 12.
Item {
    id: root
    property string digits: ""
    readonly property int secondDigitWait: 1000
    signal committed(int key)
    implicitWidth: badge.implicitWidth
    implicitHeight: badge.implicitHeight
    function press(digit: int) {
        if (pending.running) {
            pending.stop();
            // 13–19 do not exist: the second key replaces the first.
            commit(digit <= 2 ? 10 + digit : digit === 0 ? 10 : digit);
        } else if (digit === 1) {
            digits = "1";
            pending.start();
        } else commit(digit === 0 ? 10 : digit);
    }
    function commit(key: int) {
        digits = String(key);
        shown.restart();
        committed(key);
    }
    function cancel() {
        pending.stop();
        shown.stop();
        digits = "";
    }
    Timer { id: pending; interval: root.secondDigitWait; onTriggered: root.commit(1) }
    Timer { id: shown; interval: Theme.seekFeedbackDuration; onTriggered: root.digits = "" }
    Label {
        id: badge
        objectName: "channelNumberBadge"
        text: root.digits + (pending.running ? "–" : "")
        textFormat: Text.PlainText
        color: Theme.textPrimary
        font.pixelSize: Theme.fontDisplay
        font.bold: true
        horizontalAlignment: Text.AlignHCenter
        leftPadding: Theme.spaceXl
        rightPadding: Theme.spaceXl
        topPadding: Theme.spaceSm
        bottomPadding: Theme.spaceSm
        opacity: root.digits.length > 0 ? 1 : 0
        visible: opacity > 0
        Behavior on opacity { NumberAnimation { duration: Theme.fadeOutDuration } }
        background: PanelSurface { color: Theme.overlaySurface }
    }
}
