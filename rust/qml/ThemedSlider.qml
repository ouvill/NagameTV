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
        height: 4
        radius: 2
        color: "#32ffffff"
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
