import QtQuick
import MinimalViewer
import QtQuick.Controls

Slider {
    id: slider
    property bool subdued: false
    implicitWidth: 132
    implicitHeight: 28
    padding: Theme.spaceSm
    hoverEnabled: true
    opacity: enabled ? 1 : Theme.disabledOpacity
    background: Rectangle {
        x: slider.leftPadding
        y: slider.topPadding + slider.availableHeight / 2 - height / 2
        width: slider.availableWidth
        height: slider.pressed || slider.hovered ? 6 : 4
        radius: height / 2
        color: Theme.overlayBorder
        Behavior on height { NumberAnimation { duration: Theme.colorDuration; easing.type: Easing.OutCubic } }
        Rectangle {
            x: slider.mirrored ? parent.width - width : 0
            width: slider.position * parent.width
            height: parent.height
            radius: parent.radius
            color: slider.subdued ? Theme.textSecondary : Theme.accent
        }
    }
    handle: Rectangle {
        x: slider.leftPadding + slider.visualPosition * (slider.availableWidth - width)
        y: slider.topPadding + slider.availableHeight / 2 - height / 2
        implicitWidth: 12
        implicitHeight: 12
        radius: height / 2
        scale: slider.pressed ? 1.4 : slider.hovered || slider.visualFocus ? 1.2 : 1
        Behavior on scale { NumberAnimation { duration: Theme.colorDuration; easing.type: Easing.OutCubic } }
        Rectangle {
            anchors.centerIn: parent
            width: 26; height: 26; radius: height / 2
            color: Theme.accent
            opacity: slider.pressed ? 0.18 : slider.hovered || slider.visualFocus ? 0.1 : 0
            Behavior on opacity { NumberAnimation { duration: Theme.fadeInDuration } }
        }
        color: slider.pressed || slider.hovered ? Theme.textPrimary : (slider.subdued ? Theme.textSecondary : Theme.accent)
        border.width: slider.visualFocus ? 2 : 0
        border.color: Theme.textPrimary
        Behavior on color {
            ColorAnimation {
                duration: Theme.colorDuration
            }
        }
    }
}
