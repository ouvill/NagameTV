pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls

Rectangle {
    id: root
    required property string status
    required property bool canPlay
    required property bool hasChannels
    property bool loading: false
    property string playbackError: ""
    property bool showDetails: false
    onPlaybackErrorChanged: if (!playbackError.length)
        showDetails = false
    signal playRequested
    signal channelsRequested
    signal settingsRequested
    color: "#141516"
    Column {
        anchors.centerIn: parent
        spacing: 14
        Label {
            anchors.horizontalCenter: parent.horizontalCenter
            text: root.playbackError.length ? "再生できません" : "テレビ視聴"
            color: "#f4f5f3"
            font.pixelSize: root.playbackError.length ? 26 : 32
        }
        Label {
            objectName: "stoppedStatus"
            anchors.horizontalCenter: parent.horizontalCenter
            width: Math.min(420, root.width - 32)
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: root.playbackError.length ? "映像を再生できませんでした。再試行するか、チャンネルや接続設定を確認してください。" : root.status
            color: "#b6bab6"
        }
        Button {
            id: action
            objectName: "stoppedAction"
            anchors.horizontalCenter: parent.horizontalCenter
            implicitWidth: 180
            implicitHeight: 44
            enabled: !root.loading
            text: root.loading ? "取得中…" : root.canPlay ? (root.playbackError.length ? "再試行" : "視聴する") : root.hasChannels ? "チャンネルを選択" : "接続設定"
            contentItem: Label {
                text: action.text
                color: "#191a1b"
                font.bold: true
                horizontalAlignment: Text.AlignHCenter
                verticalAlignment: Text.AlignVCenter
            }
            background: Rectangle {
                radius: 22
                color: "#9caf9f"
            }
            onClicked: {
                if (root.canPlay)
                    root.playRequested();
                else if (root.hasChannels)
                    root.channelsRequested();
                else
                    root.settingsRequested();
            }
        }
        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: 8
            visible: root.playbackError.length > 0
            TextAction {
                text: "チャンネルを選択"
                onClicked: root.channelsRequested()
            }
            TextAction {
                text: "接続設定"
                onClicked: root.settingsRequested()
            }
            TextAction {
                objectName: "errorDetailsAction"
                text: "エラー詳細"
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
