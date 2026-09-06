pragma ComponentBehavior: Bound
import QtQuick

Rectangle {
    id: toggle
    property bool checked: false
    signal toggled
    implicitWidth: 42
    implicitHeight: 24
    radius: 12
    color: checked ? "#9caf9f" : "#4c4f4c"
    Behavior on color {
        ColorAnimation {
            duration: 120
        }
    }
    Rectangle {
        width: 18
        height: 18
        radius: 9
        x: toggle.checked ? toggle.width - width - 3 : 3
        anchors.verticalCenter: parent.verticalCenter
        color: toggle.checked ? "#17201a" : "#d7d9d7"
        Behavior on x {
            NumberAnimation {
                duration: 120
                easing.type: Easing.OutCubic
            }
        }
    }
    MouseArea {
        anchors.fill: parent
        cursorShape: Qt.PointingHandCursor
        onClicked: toggle.toggled()
    }
}
