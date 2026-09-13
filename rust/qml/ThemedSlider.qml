import QtQuick
import QtQuick.Controls

Slider {
    id: slider
    property bool subdued: false
    implicitWidth: 132
    implicitHeight: 28
    padding: 8
    hoverEnabled: true
    background: Rectangle {
        x: slider.leftPadding
        y: slider.topPadding + slider.availableHeight / 2 - height / 2
        width: slider.availableWidth
        height: slider.pressed || slider.hovered ? 6 : 4
        radius: height / 2
        color: "#32ffffff"
        Behavior on height { NumberAnimation { duration: 100; easing.type: Easing.OutCubic } }
        Rectangle {
            x: slider.mirrored ? parent.width - width : 0
            width: slider.position * parent.width
            height: parent.height
            radius: parent.radius
            color: slider.subdued ? "#b6bab6" : "#9caf9f"
        }
    }
    handle: Rectangle {
        x: slider.leftPadding + slider.visualPosition * (slider.availableWidth - width)
        y: slider.topPadding + slider.availableHeight / 2 - height / 2
        implicitWidth: 12
        implicitHeight: 12
        radius: 6
        scale: slider.pressed ? 1.4 : slider.hovered || slider.visualFocus ? 1.2 : 1
        Behavior on scale { NumberAnimation { duration: 100; easing.type: Easing.OutCubic } }
        Rectangle {
            anchors.centerIn: parent
            width: 26; height: 26; radius: 13
            color: "#9caf9f"
            opacity: slider.pressed ? 0.18 : slider.hovered || slider.visualFocus ? 0.1 : 0
            Behavior on opacity { NumberAnimation { duration: 120 } }
        }
        color: slider.pressed || slider.hovered ? "#f4f5f3" : (slider.subdued ? "#b6bab6" : "#9caf9f")
        border.width: slider.visualFocus ? 2 : 0
        border.color: "#f4f5f3"
        Behavior on color {
            ColorAnimation {
                duration: 100
            }
        }
    }
}
