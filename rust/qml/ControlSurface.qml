import QtQuick
import MinimalViewer

Rectangle {
    required property bool focused
    property bool hovered: false
    property bool pressed: false
    radius: Theme.controlRadius
    color: pressed ? Theme.surfacePressed : hovered ? Theme.surfaceHover : Theme.surfaceRaised
    border.color: focused ? Theme.accent : Theme.border
    Behavior on color { ColorAnimation { duration: Theme.colorDuration } }
    Behavior on border.color { ColorAnimation { duration: Theme.colorDuration } }
}
