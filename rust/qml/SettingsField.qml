pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls

TextField {
    id: control
    implicitHeight: 44
    color: "#f4f5f3"
    placeholderTextColor: "#8c918c"
    selectionColor: "#527359"
    font.pixelSize: 16
    selectByMouse: true
    leftPadding: 14
    rightPadding: 14
    background: Rectangle {
        radius: 8
        color: "#222622"
        border.color: control.activeFocus ? "#9caf9f" : "#3b423c"
        Behavior on border.color { ColorAnimation { duration: 100 } }
    }
}
