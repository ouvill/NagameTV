pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
import QtQuick.Controls
import QtQuick.Layouts

// Shows Rust's live stall recovery state. Rust decides when playback stalled,
// how often it retries and the translated message; this only presents it.
Control {
    id: root
    enum Phase { Idle, Recovering, Failed }
    required property string recoveryState
    required property string message
    // Hide a recovery that output resumes from almost immediately.
    property int revealDelayMs: 700
    readonly property int phase: recoveryState === "recovering" ? LiveRecoveryNotice.Recovering
        : recoveryState === "failed" ? LiveRecoveryNotice.Failed : LiveRecoveryNotice.Idle
    readonly property Item retryButton: retry
    signal retryRequested
    signal dismissFocusRequested

    visible: phase === LiveRecoveryNotice.Failed || (phase === LiveRecoveryNotice.Recovering && !delay.running)
    padding: Theme.spaceMd
    leftPadding: Theme.spaceLg
    rightPadding: Theme.spaceLg
    Accessible.role: Accessible.AlertMessage
    Accessible.name: label.text
    onPhaseChanged: if (phase === LiveRecoveryNotice.Recovering) delay.restart(); else delay.stop()
    Timer { id: delay; interval: root.revealDelayMs }
    contentItem: RowLayout {
        spacing: Theme.spaceMd
        BusyIndicator {
            visible: root.phase === LiveRecoveryNotice.Recovering
            running: visible && root.visible
            Layout.preferredWidth: 24
            Layout.preferredHeight: 24
        }
        Label {
            id: label
            objectName: "liveRecoveryText"
            Layout.maximumWidth: 420
            text: root.message
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            color: Theme.textPrimary
            font.pixelSize: Theme.fontBody
        }
        ActionButton {
            id: retry
            objectName: "liveRecoveryRetry"
            visible: root.phase === LiveRecoveryNotice.Failed
            emphasis: ActionButton.Primary
            text: qsTranslate("Main", "Retry")
            onClicked: root.retryRequested()
            Keys.onEscapePressed: function(event) { event.accepted = true; root.dismissFocusRequested(); }
            Keys.onBackPressed: function(event) { event.accepted = true; root.dismissFocusRequested(); }
        }
    }
    background: PanelSurface {}
}
