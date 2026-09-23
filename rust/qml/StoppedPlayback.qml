pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
import QtQuick.Controls

Rectangle {
    id: root
    required property string status
    required property bool canPlay
    required property bool hasChannels
    property bool hasServer: false
    property bool loading: false
    property string playbackError: ""
    property string playbackMessage: ""
    property bool recording: false
    property bool showDetails: false
    onPlaybackErrorChanged: if (!playbackError.length)
        showDetails = false
    signal playRequested
    signal channelsRequested
    signal settingsRequested
    signal reconnectRequested
    signal openFileRequested
    color: Theme.surface
    Column {
        anchors.centerIn: parent
        spacing: Theme.spaceLg
        Label {
            anchors.horizontalCenter: parent.horizontalCenter
            text: root.playbackError.length ? qsTranslate("Viewer", "Playback unavailable") : root.recording ? qsTranslate("Recording", "Recording") : qsTranslate("Main", "Live TV")
            color: Theme.textPrimary
            font.pixelSize: root.playbackError.length ? Theme.fontTitle : Theme.fontDisplay
            font.bold: true
        }
        Label {
            objectName: "stoppedStatus"
            anchors.horizontalCenter: parent.horizontalCenter
            width: Math.min(420, root.width - 32)
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: root.playbackError.length ? (root.playbackMessage.length ? qsTranslate("Backend", root.playbackMessage) : qsTranslate("Viewer", "Could not play the video. Retry or check the channel and connection settings.")) : root.status
            color: Theme.textSecondary
        }
        BusyIndicator {
            objectName: "stoppedLoading"
            anchors.horizontalCenter: parent.horizontalCenter
            implicitWidth: 44
            implicitHeight: 44
            visible: root.loading
            running: visible
            palette.dark: Theme.accent
            palette.text: Theme.accent
            Accessible.name: qsTranslate("Viewer", "Loading\u2026")
        }
        ActionButton {
            emphasis: ActionButton.Primary
            id: action
            objectName: "stoppedAction"
            anchors.horizontalCenter: parent.horizontalCenter
            implicitWidth: 180
            implicitHeight: 44
            visible: !root.loading
            enabled: !root.loading
            text: root.canPlay ? (root.playbackError.length ? qsTranslate("Main", "Retry") : root.recording ? qsTranslate("Viewer", "Play") : qsTranslate("Main", "Watch")) : root.hasChannels ? qsTranslate("Viewer", "Choose a channel") : root.hasServer ? qsTranslate("Connection", "Reconnect") : qsTranslate("Main", "Connection settings")
            onClicked: {
                if (root.canPlay)
                    root.playRequested();
                else if (root.hasChannels)
                    root.channelsRequested();
                else if (root.hasServer)
                    root.reconnectRequested();
                else
                    root.settingsRequested();
            }
        }
        ActionButton {
            objectName: "openRecording"
            anchors.horizontalCenter: parent.horizontalCenter
            text: qsTranslate("Recording", "Open TS file")
            onClicked: root.openFileRequested()
        }
        ActionButton {
            objectName: "changeConnection"
            anchors.horizontalCenter: parent.horizontalCenter
            visible: root.hasServer && !root.hasChannels && !root.loading
            text: qsTranslate("Connection", "Change server")
            onClicked: root.settingsRequested()
        }
        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: Theme.spaceSm
            visible: root.playbackError.length > 0
            ActionButton {
                text: qsTranslate("Viewer", "Choose a channel")
                onClicked: root.channelsRequested()
            }
            ActionButton {
                text: qsTranslate("Main", "Connection settings")
                onClicked: root.settingsRequested()
            }
            ActionButton {
                objectName: "errorDetailsAction"
                text: qsTranslate("Main", "Error details")
                onClicked: root.showDetails = true
            }
        }
    }
    Loader {
        objectName: "errorDetailsLoader"
        active: root.showDetails && root.playbackError.length > 0
        sourceComponent: PlaybackErrorDetails {
            details: root.playbackError
            onClosed: root.showDetails = false
        }
    }
}
