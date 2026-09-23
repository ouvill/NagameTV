pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
import QtQuick.Templates as T

T.Switch {
    id: control
    hoverEnabled: true
    focusPolicy: Qt.StrongFocus
    implicitWidth: implicitIndicatorWidth + leftPadding + rightPadding
    implicitHeight: implicitIndicatorHeight + topPadding + bottomPadding
    padding: 0
    opacity: enabled ? 1 : Theme.disabledOpacity
    Accessible.name: text
    HoverHandler { cursorShape: Qt.PointingHandCursor }
    indicator: Rectangle {
        implicitWidth: 42
        implicitHeight: 24
        x: control.width - width - control.rightPadding
        y: control.topPadding + (control.availableHeight - height) / 2
        radius: Theme.panelRadius
        color: control.checked ? Theme.accent : Theme.switchTrack
        border.color: control.visualFocus ? Theme.textPrimary : "transparent"
        scale: control.down ? 0.96 : control.hovered || control.visualFocus ? 1.04 : 1
        Behavior on color { ColorAnimation { duration: Theme.moveDuration } }
        Behavior on scale { NumberAnimation { duration: Theme.colorDuration; easing.type: Easing.OutCubic } }
        Rectangle {
            property real position: control.checked ? 1 : 0
            width: control.down ? 23 : 18
            height: 18
            radius: height / 2
            x: 3 + position * (parent.width - width - 6)
            anchors.verticalCenter: parent.verticalCenter
            color: control.checked ? Theme.textOnAccent : Theme.textSecondary
            Behavior on width { NumberAnimation { duration: Theme.colorDuration; easing.type: Easing.OutCubic } }
            Behavior on position { NumberAnimation { duration: Theme.moveDuration; easing.type: Easing.OutCubic } }
            Behavior on color { ColorAnimation { duration: Theme.fadeInDuration } }
        }
    }
}
