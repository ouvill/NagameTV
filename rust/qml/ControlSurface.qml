import QtQuick
import MinimalViewer

Rectangle {
    id: surface
    required property bool focused
    property bool hovered: false
    property bool pressed: false
    radius: Theme.controlRadius
    color: pressed ? Theme.surfacePressed : hovered ? Theme.surfaceHover : Theme.surfaceRaised
    border.color: Theme.border
    Behavior on color { ColorAnimation { duration: Theme.colorDuration } }
    FocusOutline { focused: surface.focused; cornerRadius: surface.radius }
}
