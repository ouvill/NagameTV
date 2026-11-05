pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
import QtQuick.Controls
import QtQuick.Layouts

// Shown while Rust retries a dropped HTTP recording connection. Playback may
// continue from read-ahead meanwhile, so this informs rather than blocks.
Control {
    id: root
    required property bool reconnecting
    // Hide sub-second blips that the immediate retry already absorbs.
    property int revealDelayMs: 700

    visible: reconnecting && !delay.running
    padding: Theme.spaceMd
    leftPadding: Theme.spaceLg
    rightPadding: Theme.spaceLg
    Accessible.role: Accessible.AlertMessage
    Accessible.name: label.text
    onReconnectingChanged: if (reconnecting) delay.restart(); else delay.stop()
    Timer { id: delay; interval: root.revealDelayMs }
    contentItem: RowLayout {
        spacing: Theme.spaceMd
        BusyIndicator {
            running: root.visible
            Layout.preferredWidth: 24
            Layout.preferredHeight: 24
        }
        Label {
            id: label
            objectName: "recordingReconnectText"
            text: qsTranslate("Main", "Reconnecting to the recording server…")
            textFormat: Text.PlainText
            color: Theme.textPrimary
            font.pixelSize: Theme.fontBody
        }
    }
    background: PanelSurface {}
}
