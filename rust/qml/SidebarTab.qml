import QtQuick
import MinimalViewer
import QtQuick.Controls

ToolButton {
    id: root
    required property url iconSource
    property bool selected: false
    implicitHeight: 54
    background: Rectangle {
        radius: Theme.panelRadius
        color: root.selected ? Theme.selection : "transparent"
        border.width: root.visualFocus ? 1 : 0
        border.color: Theme.accent
    }
    contentItem: Item {
        Column {
            anchors.centerIn: parent
            spacing: 3
            Image {
                anchors.horizontalCenter: parent.horizontalCenter
                width: 18
                height: 18
                sourceSize: Qt.size(18, 18)
                source: root.iconSource
                opacity: root.selected ? 1 : 0.68
            }
            Label {
                anchors.horizontalCenter: parent.horizontalCenter
                text: root.text
                color: root.selected ? Theme.textPrimary : Theme.textSecondary
                font.pixelSize: Theme.fontMicro
            }
        }
    }
}
