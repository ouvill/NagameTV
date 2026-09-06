import QtQuick
import QtQuick.Controls

Rectangle {
    id: root
    required property string status
    required property bool canPlay
    required property bool hasChannels
    property bool loading: false
    signal playRequested
    signal channelsRequested
    signal settingsRequested
    color: "#141516"
    Column {
        anchors.centerIn: parent
        spacing: 14
        Label {
            anchors.horizontalCenter: parent.horizontalCenter
            text: "テレビ視聴"
            color: "#f4f5f3"
            font.pixelSize: 32
        }
        Label {
            objectName: "stoppedStatus"
            anchors.horizontalCenter: parent.horizontalCenter
            width: Math.min(420, root.width - 32)
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: root.status
            color: "#b6bab6"
        }
        Button {
            id: action
            objectName: "stoppedAction"
            anchors.horizontalCenter: parent.horizontalCenter
            implicitWidth: 180
            implicitHeight: 44
            enabled: !root.loading
            text: root.loading ? "取得中…" : root.canPlay ? "視聴する" : root.hasChannels ? "チャンネルを選択" : "接続設定"
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
    }
}
