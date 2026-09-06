pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls

ListView {
    id: root
    required property string commentsJson
    required property string status
    clip: true
    model: JSON.parse(commentsJson)
    onModelChanged: Qt.callLater(root.positionViewAtEnd)
    delegate: Item {
        id: row
        required property var modelData
        width: ListView.view.width
        height: Math.max(62, body.implicitHeight + 28)
        Label {
            x: 0; y: 16; width: 62
            text: row.modelData.time
            color: "#929497"; font.pixelSize: 10
        }
        Label {
            id: body
            x: 70; y: 13; width: parent.width - 70
            text: row.modelData.text; textFormat: Text.PlainText
            color: "#e5e5e4"; font.pixelSize: 14; wrapMode: Text.Wrap
        }
        Label {
            anchors.right: parent.right; anchors.bottom: parent.bottom; anchors.bottomMargin: 5
            text: row.modelData.source; textFormat: Text.PlainText
            color: "#b6bab6"; font.pixelSize: 9
        }
        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: "#12ffffff" }
    }
    Label {
        anchors.centerIn: parent
        visible: root.count === 0
        width: parent.width - 24
        horizontalAlignment: Text.AlignHCenter; wrapMode: Text.Wrap
        text: root.status; textFormat: Text.PlainText
        color: "#b6bab6"; font.pixelSize: 13
    }
}
