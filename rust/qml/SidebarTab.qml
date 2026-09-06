import QtQuick
import QtQuick.Controls

ToolButton {
    id: root
    required property url iconSource
    property bool selected: false
    implicitHeight: 54
    background: Rectangle {
        radius: 12
        color: root.selected ? "#249caf9f" : "transparent"
        border.width: root.visualFocus ? 1 : 0
        border.color: "#9caf9f"
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
                color: root.selected ? "#f4f5f3" : "#b6bab6"
                font.pixelSize: 10
            }
        }
    }
}
