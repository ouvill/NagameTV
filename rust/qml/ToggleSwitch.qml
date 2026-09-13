pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Templates as T

T.Switch {
    id: control
    hoverEnabled: true
    focusPolicy: Qt.StrongFocus
    implicitWidth: implicitIndicatorWidth + leftPadding + rightPadding
    implicitHeight: implicitIndicatorHeight + topPadding + bottomPadding
    padding: 0
    opacity: enabled ? 1 : 0.42
    Accessible.name: text
    HoverHandler { cursorShape: Qt.PointingHandCursor }
    indicator: Rectangle {
        implicitWidth: 42
        implicitHeight: 24
        x: control.width - width - control.rightPadding
        y: control.topPadding + (control.availableHeight - height) / 2
        radius: 12
        color: control.checked ? "#9caf9f" : "#4c4f4c"
        border.color: control.visualFocus ? "#f4f5f3" : "transparent"
        scale: control.down ? 0.96 : control.hovered || control.visualFocus ? 1.04 : 1
        Behavior on color { ColorAnimation { duration: 140 } }
        Behavior on scale { NumberAnimation { duration: 100; easing.type: Easing.OutCubic } }
        Rectangle {
            property real position: control.checked ? 1 : 0
            width: control.down ? 23 : 18
            height: 18
            radius: 9
            x: 3 + position * (parent.width - width - 6)
            anchors.verticalCenter: parent.verticalCenter
            color: control.checked ? "#17201a" : "#d7d9d7"
            Behavior on width { NumberAnimation { duration: 90; easing.type: Easing.OutCubic } }
            Behavior on position { NumberAnimation { duration: 160; easing.type: Easing.OutCubic } }
            Behavior on color { ColorAnimation { duration: 120 } }
        }
    }
}
